//! Code generation utilities

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// Apply type hint generics to type name if needed
pub fn apply_type_hint_to_pattern(
    type_name: TokenStream2,
    hint_generics: &Option<TokenStream2>,
) -> TokenStream2 {
    if let Some(generics) = hint_generics {
        let type_str = type_name.to_string();
        // Check if type_name doesn't already have generics
        if !type_str.contains('<') {
            return quote! { #type_name #generics };
        }
    }
    type_name
}
