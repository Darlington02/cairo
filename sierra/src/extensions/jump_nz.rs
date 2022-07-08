use crate::{extensions::*, utils::gas_type};

struct JumpNzExtension {}

impl ExtensionImplementation for JumpNzExtension {
    fn get_signature(
        self: &Self,
        tmpl_args: &Vec<TemplateArg>,
        _: &TypeRegistry,
    ) -> Result<ExtensionSignature, Error> {
        if tmpl_args.len() != 1 {
            return Err(Error::WrongNumberOfTypeArgs);
        }
        let numeric_type = match &tmpl_args[0] {
            TemplateArg::Type(t) => Ok(t),
            TemplateArg::Value(_) => Err(Error::UnsupportedTypeArg),
        }?;
        Ok(ExtensionSignature {
            args: vec![numeric_type.clone(), gas_type(1)],
            results: vec![vec![], vec![]],
            fallthrough: Some(1),
        })
    }
}

pub(super) fn extensions() -> [(String, ExtensionBox); 1] {
    [("jump_nz".to_string(), Box::new(JumpNzExtension {}))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{as_type, type_arg, val_arg};

    #[test]
    fn legal_usage() {
        assert_eq!(
            JumpNzExtension {}.get_signature(&vec![type_arg(as_type("int"))], &TypeRegistry::new()),
            Ok(ExtensionSignature {
                args: vec![as_type("int"), gas_type(1)],
                results: vec![vec![], vec![]],
                fallthrough: Some(1),
            })
        );
    }

    #[test]
    fn wrong_num_of_args() {
        assert_eq!(
            JumpNzExtension {}.get_signature(&vec![], &TypeRegistry::new()),
            Err(Error::WrongNumberOfTypeArgs)
        );
    }

    #[test]
    fn wrong_arg_type() {
        assert_eq!(
            JumpNzExtension {}.get_signature(&vec![val_arg(1)], &TypeRegistry::new()),
            Err(Error::UnsupportedTypeArg)
        );
    }
}
