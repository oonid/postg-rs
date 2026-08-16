extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, FnArg, PatType, Pat, Error};

#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let sig = &input.sig;
    
    if sig.asyncness.is_none() {
        let err = Error::new_spanned(sig, "the #[postg::test] macro requires an async function");
        return TokenStream::from(err.into_compile_error());
    }

    if sig.inputs.len() > 1 {
        let err = Error::new_spanned(&sig.inputs, "the #[postg::test] macro requires 0 or 1 argument");
        return TokenStream::from(err.into_compile_error());
    }

    let mut param_injection = quote! {};
    
    if let Some(FnArg::Typed(PatType { pat, ty, .. })) = sig.inputs.first() {
        let ident = if let Pat::Ident(pat_ident) = &**pat {
            &pat_ident.ident
        } else {
            let err = Error::new_spanned(pat, "expected a simple identifier");
            return TokenStream::from(err.into_compile_error());
        };

        // Determine if they want a String or a Postg instance based on the type
        let ty_str = quote!(#ty).to_string();
        if ty_str == "String" {
            param_injection = quote! {
                let #ident: String = _postg_db.connection_string();
            };
        } else if ty_str.contains("Postg") {
            param_injection = quote! {
                let #ident = _postg_db;
            };
        } else {
            let err = Error::new_spanned(ty, "unsupported parameter type. Expected `String` or `postg::engine::Postg`");
            return TokenStream::from(err.into_compile_error());
        }
    }

    // Strip the parameters from the generated function signature
    let mut new_sig = sig.clone();
    new_sig.inputs.clear();
    let block = &input.block;

    // Simple config parsing (if engine="postgresql-spock" is passed in attr)
    let mut config_injection = quote! { let mut _postg_config = postg::config::Config::default(); };
    if !attr.is_empty() {
        let attr_str = attr.to_string();
        if attr_str.contains("postgresql-spock") {
            config_injection = quote! {
                let mut _postg_config = postg::config::Config::default();
                _postg_config.engine = postg::config::Engine::PostgresqlSpock;
            };
        }
    }

    let expanded = quote! {
        #[tokio::test]
        #new_sig {
            #config_injection
            _postg_config.temporary = true;
            let _postg_db = postg::engine::Postg::start(_postg_config)
                .await
                .expect("Failed to start embedded postg-rs instance");
            
            #param_injection

            {
                #block
            }
        }
    };
    
    TokenStream::from(expanded)
}
