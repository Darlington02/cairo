use crate::{
    extensions::*,
    utils::{gas_builtin_type, gas_type},
};

struct GetGasExtension {}

impl ExtensionImplementation for GetGasExtension {
    fn get_signature(
        self: &Self,
        tmpl_args: &Vec<TemplateArg>,
    ) -> Result<ExtensionSignature, Error> {
        if tmpl_args.is_empty() {
            return Err(Error::WrongNumberOfTypeArgs);
        }
        let mut success_types = vec![(gas_builtin_type(), ResLoc::NewMem)];
        tmpl_args.iter().try_for_each(|tmpl_arg| match tmpl_arg {
            TemplateArg::Value(v) => {
                success_types.push((gas_type(*v), ResLoc::NewMem));
                Ok(())
            }
            TemplateArg::Type(_) => Err(Error::UnsupportedTypeArg),
        })?;
        Ok(ExtensionSignature {
            args: vec![gas_builtin_type(), gas_type(1)],
            results: vec![success_types, vec![(gas_builtin_type(), ResLoc::NewMem)]],
            fallthrough: Some(1),
        })
    }
}

struct RefundGasExtension {}

impl ExtensionImplementation for RefundGasExtension {
    fn get_signature(
        self: &Self,
        tmpl_args: &Vec<TemplateArg>,
    ) -> Result<ExtensionSignature, Error> {
        if tmpl_args.len() != 1 {
            return Err(Error::WrongNumberOfTypeArgs);
        }
        let value = match &tmpl_args[0] {
            TemplateArg::Value(v) => Ok(*v),
            TemplateArg::Type(_) => Err(Error::UnsupportedTypeArg),
        }?;
        Ok(simple_invoke_ext_sign(
            vec![gas_builtin_type(), gas_type(value)],
            vec![(gas_builtin_type(), ResLoc::NewMem)],
        ))
    }
}

struct SplitGasExtension {}

impl ExtensionImplementation for SplitGasExtension {
    fn get_signature(
        self: &Self,
        tmpl_args: &Vec<TemplateArg>,
    ) -> Result<ExtensionSignature, Error> {
        if tmpl_args.len() <= 1 {
            return Err(Error::WrongNumberOfTypeArgs);
        }
        let mut res_types = vec![];
        let mut total = 0;
        tmpl_args.iter().try_for_each(|tmpl_arg| match tmpl_arg {
            TemplateArg::Value(v) => {
                res_types.push((gas_type(*v), ResLoc::NewMem));
                total += v;
                Ok(())
            }
            TemplateArg::Type(_) => Err(Error::UnsupportedTypeArg),
        })?;
        Ok(simple_invoke_ext_sign(vec![gas_type(total)], res_types))
    }
}

pub(super) fn extensions() -> [(String, ExtensionBox); 3] {
    [
        ("get_gas".to_string(), Box::new(GetGasExtension {})),
        ("refund_gas".to_string(), Box::new(RefundGasExtension {})),
        ("split_gas".to_string(), Box::new(SplitGasExtension {})),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{type_arg, val_arg};

    #[test]
    fn legal_usage() {
        assert_eq!(
            GetGasExtension {}.get_signature(&vec![val_arg(1), val_arg(2)]),
            Ok(ExtensionSignature {
                args: vec![gas_builtin_type(), gas_type(1)],
                results: vec![
                    vec![
                        (gas_builtin_type(), ResLoc::NewMem),
                        (gas_type(1), ResLoc::NewMem),
                        (gas_type(2), ResLoc::NewMem)
                    ],
                    vec![(gas_builtin_type(), ResLoc::NewMem)]
                ],
                fallthrough: Some(1),
            })
        );
        assert_eq!(
            RefundGasExtension {}.get_signature(&vec![val_arg(5)]),
            Ok(simple_invoke_ext_sign(
                vec![gas_builtin_type(), gas_type(5)],
                vec![(gas_builtin_type(), ResLoc::NewMem)],
            ))
        );
        assert_eq!(
            SplitGasExtension {}.get_signature(&vec![val_arg(1), val_arg(2)]),
            Ok(simple_invoke_ext_sign(
                vec![gas_type(3)],
                vec![(gas_type(1), ResLoc::NewMem), (gas_type(2), ResLoc::NewMem)],
            ))
        );
    }

    #[test]
    fn wrong_num_of_args() {
        assert_eq!(
            GetGasExtension {}.get_signature(&vec![]),
            Err(Error::WrongNumberOfTypeArgs)
        );
        assert_eq!(
            RefundGasExtension {}.get_signature(&vec![]),
            Err(Error::WrongNumberOfTypeArgs)
        );
        assert_eq!(
            SplitGasExtension {}.get_signature(&vec![]),
            Err(Error::WrongNumberOfTypeArgs)
        );
        assert_eq!(
            SplitGasExtension {}.get_signature(&vec![val_arg(1)]),
            Err(Error::WrongNumberOfTypeArgs)
        );
    }

    #[test]
    fn wrong_arg_type() {
        assert_eq!(
            GetGasExtension {}.get_signature(&vec![type_arg(gas_type(1))]),
            Err(Error::UnsupportedTypeArg)
        );
        assert_eq!(
            RefundGasExtension {}.get_signature(&vec![type_arg(gas_type(1))]),
            Err(Error::UnsupportedTypeArg)
        );
        assert_eq!(
            SplitGasExtension {}.get_signature(&vec![val_arg(1), type_arg(gas_type(1))]),
            Err(Error::UnsupportedTypeArg)
        );
    }
}
