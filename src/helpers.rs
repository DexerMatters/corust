//! Helper functions for type parameter handling and code generation

use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use std::collections::HashSet;
use syn::{GenericParam, Generics};

/// Collect type parameter names in order
pub fn collect_ordered_type_params(generics: &Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(t) => Some(t.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// Add 'static bounds to all generic type parameters
pub fn add_static_bounds(generics: &Generics) -> Generics {
    let mut generics_with_static = generics.clone();
    for param in generics_with_static.type_params_mut() {
        param.bounds.push(syn::parse_quote!('static));
    }
    generics_with_static
}

/// Strip generic type parameters from a pattern (e.g., "Lift<i32>(x)" -> "Lift(x)")
pub fn strip_pattern_generics(pattern: &TokenStream2) -> TokenStream2 {
    let mut result_tokens = Vec::new();
    let mut skip_until_gt = false;

    for tt in pattern.clone() {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == '<' => {
                skip_until_gt = true;
            }
            TokenTree::Punct(p) if p.as_char() == '>' && skip_until_gt => {
                skip_until_gt = false;
                continue;
            }
            _ if skip_until_gt => continue,
            _ => result_tokens.push(tt),
        }
    }

    result_tokens.into_iter().collect()
}

/// Extract type arguments from a trait type TokenStream (e.g., "Pair<B, A>" -> [B, A])
pub fn extract_trait_type_args(trait_type: &TokenStream2) -> Vec<Vec<TokenTree>> {
    let mut trait_type_args = Vec::new();
    let mut in_angles = false;
    let mut current_arg = Vec::new();

    for tt in trait_type.clone() {
        match tt {
            TokenTree::Punct(ref p) if p.as_char() == '<' => {
                in_angles = true;
            }
            TokenTree::Punct(ref p) if p.as_char() == '>' => {
                if !current_arg.is_empty() {
                    trait_type_args.push(current_arg.drain(..).collect());
                }
                break;
            }
            TokenTree::Punct(ref p) if p.as_char() == ',' && in_angles => {
                if !current_arg.is_empty() {
                    trait_type_args.push(current_arg.drain(..).collect());
                }
            }
            _ if in_angles => {
                current_arg.push(tt);
            }
            _ => {}
        }
    }

    trait_type_args
}

/// Substitute type parameters in a signature based on trait type mapping
/// For example, if trait_type is "Pair<B, A>" and enum params are [A, B],
/// it will replace A->B and B->A in the signature
pub fn substitute_type_params(
    sig_str: &str,
    trait_type: &TokenStream2,
    enum_params: &[String],
) -> String {
    let trait_type_args = extract_trait_type_args(trait_type);

    if trait_type_args.is_empty() {
        return sig_str.to_string();
    }

    // First pass: replace each enum param with a placeholder to avoid conflicts
    let mut result = sig_str.to_string();
    for (i, enum_param) in enum_params.iter().enumerate() {
        if i < trait_type_args.len() {
            let placeholder = format!("__PLACEHOLDER_{}__", i);

            // Replace type parameter in various contexts
            result = result
                .replace(&format!("& {}", enum_param), &format!("&{}", placeholder))
                .replace(&format!("&{}", enum_param), &format!("&{}", placeholder))
                .replace(&format!("( {}", enum_param), &format!("({}", placeholder))
                .replace(&format!("({}", enum_param), &format!("({}", placeholder))
                .replace(&format!("{} ,", enum_param), &format!("{},", placeholder))
                .replace(&format!("{},", enum_param), &format!("{},", placeholder))
                .replace(&format!("{} )", enum_param), &format!("{})", placeholder))
                .replace(&format!("{})", enum_param), &format!("{})", placeholder))
                .replace(
                    &format!("-> {}", enum_param),
                    &format!("-> {}", placeholder),
                );
        }
    }

    // Second pass: replace placeholders with actual trait type args
    for (i, _) in enum_params.iter().enumerate() {
        if i < trait_type_args.len() {
            let trait_arg: TokenStream2 = trait_type_args[i].iter().cloned().collect();
            let trait_arg_str = trait_arg.to_string().trim().to_string();
            let placeholder = format!("__PLACEHOLDER_{}__", i);

            result = result.replace(&placeholder, &trait_arg_str);
        }
    }

    result
}

/// Build variant-specific generics containing only used type parameters
pub fn build_variant_generics(
    generics_with_static: &Generics,
    used_params: &HashSet<String>,
) -> Generics {
    let mut variant_generics = generics_with_static.clone();
    variant_generics.params = variant_generics
        .params
        .iter()
        .filter(|param| match param {
            GenericParam::Type(t) => used_params.contains(&t.ident.to_string()),
            _ => true, // Keep lifetime and const parameters
        })
        .cloned()
        .collect();
    variant_generics
}
