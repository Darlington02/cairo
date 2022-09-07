use std::sync::Arc;

use diagnostics_proc_macros::with_diagnostics;
use filesystem::db::{FilesDatabase, FilesGroup};
use filesystem::ids::{FileId, FileLongId, VirtualFile};
use filesystem::span::{TextOffset, TextSpan};
use indoc::indoc;

use super::{DiagnosticEntry, DiagnosticLocation, Diagnostics, WithDiagnostics};

// Test salsa database.
#[salsa::database(FilesDatabase)]
#[derive(Default)]
pub struct DatabaseImpl {
    storage: salsa::Storage<DatabaseImpl>,
}
impl salsa::Database for DatabaseImpl {}

// Test diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SimpleDiagnostic {
    file_id: FileId,
}
impl DiagnosticEntry for SimpleDiagnostic {
    type DbType = dyn FilesGroup;

    fn format(&self, _db: &dyn filesystem::db::FilesGroup) -> String {
        "Simple diagnostic.".into()
    }

    fn location(&self, _db: &dyn filesystem::db::FilesGroup) -> DiagnosticLocation {
        DiagnosticLocation {
            file_id: self.file_id,
            span: TextSpan { start: TextOffset(0), end: TextOffset(6) },
        }
    }
}

fn setup() -> (DatabaseImpl, FileId) {
    let db_val = DatabaseImpl::default();
    let file_id = db_val.intern_file(FileLongId::Virtual(VirtualFile {
        parent: None,
        name: "dummy_file.sierra".into(),
        content: Arc::new("abcd\nefg.\n".into()),
    }));
    (db_val, file_id)
}

#[test]
fn test_diagnostics() {
    let (_db_val, file_id) = setup();

    let mut diagnostics: Diagnostics<SimpleDiagnostic> = Diagnostics::new();
    let diagnostic = SimpleDiagnostic { file_id };
    diagnostics.add(diagnostic);
}

#[test]
fn test_option_with_diagnostics() {
    let (db_val, file_id) = setup();
    let db = &db_val;

    let res = dummy_compute_macro(2, file_id);
    assert_eq!(
        res.diagnostics.format(db),
        indoc! { "
            error: Simple diagnostic.
             --> dummy_file.sierra:1:1
            abcd
            ^**^

        " }
    );
}
#[with_diagnostics(SimpleDiagnostic, diagnostics)]
fn dummy_compute_macro(x: usize, file_id: FileId) -> Option<usize> {
    let param = WithDiagnostics::pure(Some(x * x));
    let res = param.unwrap(diagnostics)?;
    // This should add one diagnostic entry, and return.
    dummy_compute_fail(file_id).unwrap(diagnostics)?;
    Some(res)
}

#[with_diagnostics(SimpleDiagnostic, diagnostics)]
fn dummy_compute_fail(file_id: FileId) -> Option<usize> {
    diagnostics.add(SimpleDiagnostic { file_id });
    None
}
