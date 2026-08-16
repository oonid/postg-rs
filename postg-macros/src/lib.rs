extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let sig = &input.sig;
    let block = &input.block;

    // Minimal pass-through for now
    let expanded = quote! {
        #[tokio::test]
        #sig {
            #block
        }
    };
    TokenStream::from(expanded)
}
