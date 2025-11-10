use proc_macro2::TokenStream as TokenStream2;
use syn::{Generics, Ident};

/// Represents a parsed variant constructor in GADT-like syntax
#[derive(Debug, Clone)]
pub struct VariantDef {
    /// The name of the variant (e.g., "Left", "Right")
    pub name: Ident,

    /// Variant-specific type parameters (e.g., <T> in Exist<T>)
    pub variant_generics: Vec<Ident>,

    /// The constructor parameters (types before ->)
    pub param_types: Vec<TokenStream2>,

    /// The return type after all arrows (e.g., Either<L, R, U>)
    pub return_type: TokenStream2,
}

/// Represents the entire GADT-like enum definition
pub struct GadtEnum {
    /// Visibility (pub, pub(crate), etc.)
    pub vis: syn::Visibility,

    /// The enum name (e.g., "Either")
    pub name: Ident,

    /// The enum's type parameters (e.g., <L, R, U>)
    pub enum_generics: Generics,

    /// All variant definitions
    pub variants: Vec<VariantDef>,
}
