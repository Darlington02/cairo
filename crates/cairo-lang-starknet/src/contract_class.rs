use std::path::Path;

use anyhow::{bail, ensure, Context, Result};
use cairo_lang_compiler::db::RootDatabase;
use cairo_lang_compiler::diagnostics::check_diagnostics;
use cairo_lang_compiler::project::setup_project;
use cairo_lang_compiler::CompilerConfig;
use cairo_lang_defs::ids::TopLevelLanguageElementId;
use cairo_lang_diagnostics::ToOption;
use cairo_lang_filesystem::ids::CrateId;
use cairo_lang_semantic::db::SemanticGroup;
use cairo_lang_semantic::{ConcreteFunctionWithBodyId, FunctionLongId};
use cairo_lang_sierra_generator::canonical_id_replacer::CanonicalReplacer;
use cairo_lang_sierra_generator::db::SierraGenGroup;
use cairo_lang_sierra_generator::replace_ids::{replace_sierra_ids_in_program, SierraIdReplacer};
use itertools::{chain, Itertools};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::abi::Contract;
use crate::casm_contract_class::{deserialize_big_uint, serialize_big_uint, BigIntAsHex};
use crate::contract::{
    find_contracts, get_abi, get_module_functions, starknet_keccak, ContractDeclaration,
};
use crate::db::StarknetRootDatabaseBuilderEx;
use crate::felt_serde::sierra_to_felts;
use crate::plugin::{CONSTRUCTOR_MODULE, EXTERNAL_MODULE};

#[cfg(test)]
#[path = "contract_class_test.rs"]
mod test;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum StarknetCompilationError {
    #[error("Invalid entry point.")]
    EntryPointError,
}

/// Represents a contract in the StarkNet network.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractClass {
    pub sierra_program: Vec<BigIntAsHex>,
    pub sierra_program_debug_info: cairo_lang_sierra::debug_info::DebugInfo,
    pub entry_points_by_type: ContractEntryPoints,
    pub abi: Contract,
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEntryPoints {
    #[serde(rename = "EXTERNAL")]
    pub external: Vec<ContractEntryPoint>,
    #[serde(rename = "L1_HANDLER")]
    pub l1_handler: Vec<ContractEntryPoint>,
    #[serde(rename = "CONSTRUCTOR")]
    pub constructor: Vec<ContractEntryPoint>,
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEntryPoint {
    /// A field element that encodes the signature of the called function.
    #[serde(serialize_with = "serialize_big_uint", deserialize_with = "deserialize_big_uint")]
    pub selector: BigUint,
    /// The idx of the user function declaration in the sierra program.
    pub function_idx: usize,
}

/// Compile the contract given by path.
///
/// Errors if no contracts or more than 1 are found.
pub fn compile_path(path: &Path, compiler_config: CompilerConfig) -> Result<ContractClass> {
    let mut db = {
        let mut b = RootDatabase::builder();
        b.with_dev_corelib().unwrap();
        b.with_starknet();
        b.build()
    };

    let main_crate_ids = setup_project(&mut db, Path::new(&path))?;

    compile_only_contract_in_prepared_db(&mut db, main_crate_ids, compiler_config)
}

fn compile_only_contract_in_prepared_db(
    db: &mut RootDatabase,
    main_crate_ids: Vec<CrateId>,
    compiler_config: CompilerConfig,
) -> Result<ContractClass> {
    let contracts = find_contracts(db, &main_crate_ids);
    ensure!(!contracts.is_empty(), "Contract not found.");
    // TODO(ilya): Add contract names.
    ensure!(contracts.len() == 1, "Compilation unit must include only one contract.");

    let contracts = contracts.iter().collect::<Vec<_>>();
    let mut classes = compile_prepared_db(db, &contracts, compiler_config)?;
    assert_eq!(classes.len(), 1);
    Ok(classes.remove(0))
}

/// Runs StarkNet contracts compiler.
///
/// # Arguments
/// * `db` - Preloaded compilation database.
/// * `contracts` - [`ContractDeclaration`]s to compile. Use [`find_contracts`] to find contracts in
///   `db`.
/// * `compiler_config` - The compiler configuration.
/// # Returns
/// * `Ok(Vec<ContractClass>)` - List of all compiled contract classes found in main crates.
/// * `Err(anyhow::Error)` - Compilation failed.
pub fn compile_prepared_db(
    db: &mut RootDatabase,
    contracts: &[&ContractDeclaration],
    mut compiler_config: CompilerConfig,
) -> Result<Vec<ContractClass>> {
    if check_diagnostics(db, compiler_config.on_diagnostic.as_deref_mut()) {
        bail!("Compilation failed.");
    }

    contracts
        .iter()
        .map(|contract| {
            compile_contract_with_prepared_and_checked_db(db, contract, &compiler_config)
        })
        .try_collect()
}

fn compile_contract_with_prepared_and_checked_db(
    db: &mut RootDatabase,
    contract: &ContractDeclaration,
    compiler_config: &CompilerConfig,
) -> Result<ContractClass> {
    let external_functions: Vec<_> = get_module_functions(db, contract, EXTERNAL_MODULE)?
        .into_iter()
        .flat_map(|f| ConcreteFunctionWithBodyId::from_no_generics_free(db, f))
        .collect();
    let constructor_functions: Vec<_> = get_module_functions(db, contract, CONSTRUCTOR_MODULE)?
        .into_iter()
        .flat_map(|f| ConcreteFunctionWithBodyId::from_no_generics_free(db, f))
        .collect();
    let sierra_program = db
        .get_sierra_program_for_functions(
            chain!(&external_functions, &constructor_functions).cloned().collect(),
        )
        .to_option()
        .with_context(|| "Compilation failed without any diagnostics.")?;

    let replacer = CanonicalReplacer::from_program(&sierra_program);
    let sierra_program = if compiler_config.replace_ids {
        replace_sierra_ids_in_program(db, &sierra_program)
    } else {
        replacer.apply(&sierra_program)
    };

    let entry_points_by_type = ContractEntryPoints {
        external: get_entry_points(db, &external_functions, &replacer)?,
        l1_handler: vec![],
        /// TODO(orizi): Validate there is at most one constructor.
        constructor: get_entry_points(db, &constructor_functions, &replacer)?,
    };
    Ok(ContractClass {
        sierra_program: sierra_to_felts(&sierra_program)?,
        sierra_program_debug_info: cairo_lang_sierra::debug_info::DebugInfo::extract(
            &sierra_program,
        ),
        entry_points_by_type,
        abi: Contract::from_trait(db, get_abi(db, contract)?).with_context(|| "ABI error")?,
    })
}

/// Returns the entry points given their IDs.
fn get_entry_points(
    db: &mut RootDatabase,
    entry_point_functions: &[ConcreteFunctionWithBodyId],
    replacer: &CanonicalReplacer,
) -> Result<Vec<ContractEntryPoint>> {
    let mut entry_points = vec![];
    for function_with_body_id in entry_point_functions {
        let function_id =
            db.intern_function(FunctionLongId { function: function_with_body_id.concrete(db) });

        let sierra_id = db.intern_sierra_function(function_id);

        entry_points.push(ContractEntryPoint {
            selector: starknet_keccak(
                function_with_body_id.function_with_body_id(db).name(db).as_bytes(),
            ),
            function_idx: replacer.replace_function_id(&sierra_id).id as usize,
        });
    }
    Ok(entry_points)
}
