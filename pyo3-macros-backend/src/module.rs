// Copyright (c) 2017-present PyO3 Project and Contributors
//! Code generation for the function that initializes a python module and adds classes and function.

use crate::pyfunction::{impl_wrap_pyfunction, PyFunctionOptions};
use crate::{
    attributes::{is_attribute_ident, take_attributes, NameAttribute},
    deprecations::Deprecation,
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse::Parse, spanned::Spanned, token::Comma, Ident, Path};

/// Generates the function that is called by the python interpreter to initialize the native
/// module
pub fn py_init(fnname: &Ident, name: &Ident, doc: syn::LitStr) -> TokenStream {
    let cb_name = Ident::new(&format!("PyInit_{}", name), Span::call_site());
    assert!(doc.value().ends_with('\0'));

    quote! {
        #[no_mangle]
        #[allow(non_snake_case)]
        /// This autogenerated function is called by the python interpreter when importing
        /// the module.
        pub unsafe extern "C" fn #cb_name() -> *mut pyo3::ffi::PyObject {
            use pyo3::derive_utils::ModuleDef;
            static NAME: &str = concat!(stringify!(#name), "\0");
            static DOC: &str = #doc;
            static MODULE_DEF: ModuleDef = unsafe { ModuleDef::new(NAME, DOC) };

            pyo3::callback::handle_panic(|_py| { MODULE_DEF.make_module(_py, #fnname) })
        }
    }
}

/// Finds and takes care of the #[pyfn(...)] in `#[pymodule]`
pub fn process_functions_in_module(func: &mut syn::ItemFn) -> syn::Result<()> {
    let mut stmts: Vec<syn::Stmt> = Vec::new();

    for stmt in func.block.stmts.iter_mut() {
        if let syn::Stmt::Item(syn::Item::Fn(func)) = stmt {
            if let Some(pyfn_args) = get_pyfn_attr(&mut func.attrs)? {
                let module_name = pyfn_args.modname;
                let (ident, wrapped_function) = impl_wrap_pyfunction(func, pyfn_args.options)?;
                let item: syn::ItemFn = syn::parse_quote! {
                    fn block_wrapper() {
                        #wrapped_function
                        #module_name.add_function(#ident(#module_name)?)?;
                    }
                };
                stmts.extend(item.block.stmts.into_iter());
            }
        };
        stmts.push(stmt.clone());
    }

    func.block.stmts = stmts;
    Ok(())
}

pub struct PyFnArgs {
    modname: Path,
    options: PyFunctionOptions,
}

impl Parse for PyFnArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let modname = input.parse().map_err(
            |e| err_spanned!(e.span() => "expected module as first argument to #[pyfn()]"),
        )?;

        if input.is_empty() {
            return Ok(Self {
                modname,
                options: Default::default(),
            });
        }

        let _: Comma = input.parse()?;

        let mut deprecated_name_argument = None;
        if let Ok(lit_str) = input.parse::<syn::LitStr>() {
            deprecated_name_argument = Some(lit_str);
            if !input.is_empty() {
                let _: Comma = input.parse()?;
            }
        }

        let mut options: PyFunctionOptions = input.parse()?;
        if let Some(lit_str) = deprecated_name_argument {
            options.set_name(NameAttribute(lit_str.parse()?))?;
            options
                .deprecations
                .push(Deprecation::PyfnNameArgument, lit_str.span());
        }

        Ok(Self { modname, options })
    }
}

/// Extracts the data from the #[pyfn(...)] attribute of a function
fn get_pyfn_attr(attrs: &mut Vec<syn::Attribute>) -> syn::Result<Option<PyFnArgs>> {
    let mut pyfn_args: Option<PyFnArgs> = None;

    take_attributes(attrs, |attr| {
        if is_attribute_ident(attr, "pyfn") {
            ensure_spanned!(
                pyfn_args.is_none(),
                attr.span() => "`#[pyfn] may only be specified once"
            );
            pyfn_args = Some(attr.parse_args()?);
            Ok(true)
        } else {
            Ok(false)
        }
    })?;

    if let Some(pyfn_args) = &mut pyfn_args {
        pyfn_args.options.take_pyo3_options(attrs)?;
    }

    Ok(pyfn_args)
}
