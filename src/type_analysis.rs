//! Type parameter analysis utilities

use proc_macro2::TokenStream as TokenStream2;
use std::collections::HashSet;
use syn::{Attribute, Fields, Meta, Type, TypePath};

/// Extract trait type from variant attributes like #[impl_trait(Term<bool>)]
pub fn extract_trait_type_from_attrs(attrs: &[Attribute]) -> Option<TokenStream2> {
    for attr in attrs {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("impl_trait") {
                return Some(meta_list.tokens.clone());
            }
        }
    }
    None
}

/// Extract all type parameters used in a given type
pub fn extract_used_type_params(ty: &Type, available_params: &HashSet<String>) -> HashSet<String> {
    let mut used = HashSet::new();
    collect_type_params(ty, available_params, &mut used);
    used
}

/// Recursively collect type parameter names from a type
fn collect_type_params(ty: &Type, available: &HashSet<String>, used: &mut HashSet<String>) {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            for segment in &path.segments {
                let ident = segment.ident.to_string();
                if available.contains(&ident) {
                    used.insert(ident);
                }

                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner_ty) = arg {
                            collect_type_params(inner_ty, available, used);
                        }
                    }
                }
            }
        }
        Type::Reference(r) => collect_type_params(&r.elem, available, used),
        Type::Tuple(t) => t
            .elems
            .iter()
            .for_each(|elem| collect_type_params(elem, available, used)),
        Type::Array(a) => collect_type_params(&a.elem, available, used),
        Type::Ptr(p) => collect_type_params(&p.elem, available, used),
        Type::Slice(s) => collect_type_params(&s.elem, available, used),
        Type::Paren(p) => collect_type_params(&p.elem, available, used),
        _ => {}
    }
}

/// Collect all type parameters from variant fields
pub fn collect_variant_type_params(
    fields: &Fields,
    available_params: &HashSet<String>,
) -> HashSet<String> {
    let mut used_params = HashSet::new();

    match fields {
        Fields::Named(fields_named) => {
            for field in &fields_named.named {
                used_params.extend(extract_used_type_params(&field.ty, available_params));
            }
        }
        Fields::Unnamed(fields_unnamed) => {
            for field in &fields_unnamed.unnamed {
                used_params.extend(extract_used_type_params(&field.ty, available_params));
            }
        }
        Fields::Unit => {}
    }

    used_params
}

/// Collect all type parameter names from generics (variant-level or enum-level)
pub fn collect_all_type_param_names(generics: &syn::Generics) -> HashSet<String> {
    generics
        .type_params()
        .map(|tp| tp.ident.to_string())
        .collect()
}
