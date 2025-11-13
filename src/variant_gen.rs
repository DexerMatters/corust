//! Variant struct and implementation code generation

use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use std::collections::HashSet;
use syn::{Fields, Generics, Ident, Visibility};

use crate::enum_parser::{ParsedMethod, ParsedVariant};
use crate::helpers::{
    add_static_bounds, merge_generics, strip_pattern_generics, substitute_type_params,
};
use crate::type_analysis::{
    collect_all_type_param_names, collect_variant_type_params, extract_trait_type_from_attrs,
};

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

    let arm = matching_arms[0];
    let body = &arm.body;
    let pattern_raw = &arm.pattern;
    let cleaned_pattern = strip_pattern_generics(pattern_raw);

    let sig_str = method.sig.to_string();
    let new_sig_str = substitute_type_params(&sig_str, trait_type, all_type_params_ordered);
    let new_sig: TokenStream2 = new_sig_str.parse().unwrap_or_else(|_| method.sig.clone());

    let is_boxed_self =
        sig_str.contains("self : Box < Self >") || sig_str.contains("self: Box<Self>");

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
    impl_generics: &Generics,
    variant_ty_generics: &TokenStream2,
    where_clause: &TokenStream2,
    trait_type: &TokenStream2,
    all_type_params_ordered: &[String],
) -> TokenStream2 {
    let variant_name = &variant.ident;

    // Build impl generics token stream
    let (impl_generics_tokens, _, _) = impl_generics.split_for_impl();

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
        quote! {
            impl #impl_generics_tokens #trait_type
                for #variant_name #variant_ty_generics #where_clause {}
        }
    } else {
        quote! {
            impl #impl_generics_tokens #trait_type
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

    // Add 'static bounds to variant generics
    let variant_generics_with_static = add_static_bounds(&variant.generics);

    // Collect all available type params (variant-level + enum-level)
    let mut combined_type_params = collect_all_type_param_names(&variant_generics_with_static);
    combined_type_params.extend(all_type_params.iter().cloned());

    // Collect type parameters used in variant fields (for struct definition)
    let struct_type_params = collect_variant_type_params(&variant.fields, &combined_type_params);

    // Build merged generics for the struct: variant generics + ONLY used enum generics
    let struct_generics = merge_generics(
        &variant_generics_with_static,
        generics_with_static,
        &struct_type_params,
    );

    let (_struct_impl_generics, variant_ty_generics, _struct_where_clause) =
        struct_generics.split_for_impl();

    // Generate struct definition using struct-specific generics
    let struct_def = generate_variant_struct(variant_name, &struct_generics, &variant.fields, vis);

    // For impl block, we need ALL type params from BOTH the struct AND the trait type
    // Determine trait type first
    let trait_type = if let Some(ref tt) = variant.trait_type {
        tt.clone()
    } else if let Some(tt) = extract_trait_type_from_attrs(&variant.attrs) {
        tt
    } else {
        let ty_generics = generics_with_static.split_for_impl().1;
        quote! { #enum_name #ty_generics }
    };

    // Extract type params used in trait type
    let trait_type_params = extract_type_params_from_trait(&trait_type, all_type_params);

    // Combine struct params and trait params for impl
    let mut impl_type_params = struct_type_params.clone();
    impl_type_params.extend(trait_type_params);

    // Build impl generics: variant generics + ALL enum generics used in struct OR trait type
    let impl_generics = merge_generics(
        &variant_generics_with_static,
        generics_with_static,
        &impl_type_params,
    );

    let (_impl_generics_tokens, _, where_clause_impl) = impl_generics.split_for_impl();

    // Generate trait implementation
    let trait_impl = generate_combined_trait_impl(
        variant,
        methods,
        &impl_generics,
        &variant_ty_generics.to_token_stream(),
        &where_clause_impl.to_token_stream(),
        &trait_type,
        all_type_params_ordered,
    );

    quote! {
        #struct_def
        #trait_impl
    }
}
