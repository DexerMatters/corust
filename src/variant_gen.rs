//! Variant struct and implementation code generation

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use std::collections::HashSet;
use syn::{Fields, Generics, Ident, Visibility};

use crate::enum_parser::{ParsedMethod, ParsedVariant};
use crate::helpers::{build_variant_generics, strip_pattern_generics, substitute_type_params};
use crate::type_analysis::{collect_variant_type_params, extract_trait_type_from_attrs};

/// Extract type parameters used in a trait type (e.g., "Term<bool>" -> {}, "Term<T>" -> {"T"})
fn extract_type_params_from_trait(
    trait_type: &TokenStream2,
    all_type_params: &HashSet<String>,
) -> HashSet<String> {
    use proc_macro2::TokenTree;
    let mut used_params = HashSet::new();

    for token in trait_type.clone() {
        match token {
            TokenTree::Ident(ident) => {
                let ident_str = ident.to_string();
                if all_type_params.contains(&ident_str) {
                    used_params.insert(ident_str);
                }
            }
            TokenTree::Group(group) => {
                used_params.extend(extract_type_params_from_trait(
                    &group.stream(),
                    all_type_params,
                ));
            }
            _ => {}
        }
    }

    used_params
}

/// Generate struct definition for a variant
pub fn generate_variant_struct(
    variant_name: &Ident,
    variant_generics: &Generics,
    fields: &Fields,
    vis: &Visibility,
) -> TokenStream2 {
    match fields {
        Fields::Named(fields) => quote! {
            #vis struct #variant_name #variant_generics #fields
        },
        Fields::Unnamed(fields) => quote! {
            #vis struct #variant_name #variant_generics #fields;
        },
        Fields::Unit => quote! {
            #vis struct #variant_name #variant_generics;
        },
    }
}

/// Generate a single method implementation body for a variant
pub fn generate_method_body(
    variant: &ParsedVariant,
    method: &ParsedMethod,
    variant_ty_generics: &TokenStream2,
    trait_type: &TokenStream2,
    all_type_params_ordered: &[String],
) -> Option<(TokenStream2, bool)> {
    let variant_name = &variant.ident;
    let variant_name_str = variant_name.to_string();

    // Find all matching arms for this variant
    let matching_arms: Vec<_> = method
        .arms
        .iter()
        .filter(|arm| {
            let pattern_string = arm.pattern.to_string();
            pattern_string.contains(&variant_name_str)
        })
        .collect();

    if matching_arms.is_empty() {
        return None;
    }

    // For now, take the first matching arm (we can enhance this later)
    let arm = matching_arms[0];
    let body = &arm.body;
    let pattern_raw = &arm.pattern;
    let cleaned_pattern = strip_pattern_generics(pattern_raw);

    // Substitute type parameters based on trait type
    let sig_str = method.sig.to_string();
    let new_sig_str = substitute_type_params(&sig_str, trait_type, all_type_params_ordered);
    let new_sig: TokenStream2 = new_sig_str.parse().unwrap_or_else(|_| method.sig.clone());

    // Check if the signature uses `self: Box<Self>` for consuming the box
    let is_boxed_self =
        sig_str.contains("self : Box < Self >") || sig_str.contains("self: Box<Self>");

    // Generate appropriate match expression
    let match_expr = if is_boxed_self {
        quote! {
            let __concrete_box = (self as Box<dyn std::any::Any>)
                .downcast::<#variant_name #variant_ty_generics>()
                .expect("Downcast failed");
            match *__concrete_box {
                #cleaned_pattern => #body,
                _ => unreachable!(),
            }
        }
    } else {
        quote! {
            match self {
                #cleaned_pattern => #body,
                _ => unreachable!(),
            }
        }
    };

    let method_impl = quote! {
        #new_sig {
            #match_expr
        }
    };

    Some((method_impl, is_boxed_self))
}

/// Generate a single trait impl block containing all methods for a variant
pub fn generate_combined_trait_impl(
    variant: &ParsedVariant,
    methods: &[ParsedMethod],
    impl_generics: &TokenStream2,
    variant_ty_generics: &TokenStream2,
    where_clause: &TokenStream2,
    trait_type: &TokenStream2,
    all_type_params_ordered: &[String],
    all_type_params: &HashSet<String>,
) -> TokenStream2 {
    let variant_name = &variant.ident;

    // Extract which type params are used in the trait type
    let trait_type_params = extract_type_params_from_trait(trait_type, all_type_params);

    // Build impl generics with only the type params used in trait type
    let filtered_impl_generics = if trait_type_params.is_empty() {
        quote! {}
    } else {
        impl_generics.clone()
    };

    // Generate method implementations for all methods
    let method_impls: Vec<_> = methods
        .iter()
        .filter_map(|method| {
            generate_method_body(
                variant,
                method,
                variant_ty_generics,
                trait_type,
                all_type_params_ordered,
            )
            .map(|(method_impl, _)| method_impl)
        })
        .collect();

    if method_impls.is_empty() {
        // No methods for this variant - empty impl
        quote! {
            impl #filtered_impl_generics #trait_type
                for #variant_name #variant_ty_generics #where_clause {}
        }
    } else {
        // Impl with all methods
        quote! {
            impl #filtered_impl_generics #trait_type
                for #variant_name #variant_ty_generics #where_clause {
                #(#method_impls)*
            }
        }
    }
}

/// Generate complete code for a single variant (struct + trait impl + methods)
pub fn generate_variant_code(
    variant: &ParsedVariant,
    methods: &[ParsedMethod],
    generics_with_static: &Generics,
    all_type_params: &HashSet<String>,
    all_type_params_ordered: &[String],
    vis: &Visibility,
    enum_name: &Ident,
) -> TokenStream2 {
    let variant_name = &variant.ident;

    // Collect type parameters used in variant fields (for struct definition)
    let struct_type_params = collect_variant_type_params(&variant.fields, all_type_params);

    // Build variant-specific generics for the struct
    let variant_generics = build_variant_generics(generics_with_static, &struct_type_params);
    let (_variant_impl_generics, variant_ty_generics, _variant_where_clause) =
        variant_generics.split_for_impl();

    // Generate struct definition
    let struct_def = generate_variant_struct(variant_name, &variant_generics, &variant.fields, vis);

    // Generate trait implementation (uses full generics from enum)
    let (impl_generics_static, _, where_clause_static) = generics_with_static.split_for_impl();
    let trait_impl = if let Some(ref trait_type) = variant.trait_type {
        // Generate combined trait impl with all methods
        generate_combined_trait_impl(
            variant,
            methods,
            &impl_generics_static.to_token_stream(),
            &variant_ty_generics.to_token_stream(),
            &where_clause_static.to_token_stream(),
            trait_type,
            all_type_params_ordered,
            all_type_params,
        )
    } else if let Some(trait_type) = extract_trait_type_from_attrs(&variant.attrs) {
        // Use custom attribute #[impl_trait(...)]
        generate_combined_trait_impl(
            variant,
            methods,
            &impl_generics_static.to_token_stream(),
            &variant_ty_generics.to_token_stream(),
            &where_clause_static.to_token_stream(),
            &trait_type,
            all_type_params_ordered,
            all_type_params,
        )
    } else {
        // Default: implement the base trait
        let ty_generics = generics_with_static.split_for_impl().1;
        let default_trait_type = quote! { #enum_name #ty_generics };
        generate_combined_trait_impl(
            variant,
            methods,
            &impl_generics_static.to_token_stream(),
            &variant_ty_generics.to_token_stream(),
            &where_clause_static.to_token_stream(),
            &default_trait_type,
            all_type_params_ordered,
            all_type_params,
        )
    };

    quote! {
        #struct_def
        #trait_impl
    }
}
