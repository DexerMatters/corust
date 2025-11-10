//! # Corust - Type-Based Enum Pattern Matching
//!
//! This crate provides procedural macros for converting enums into trait objects
//! and performing type-based pattern matching on them.
//!
//! ## Features
//!
//! - `type_enum!`: Function-like macro for converting enums to traits
//! - `match_t`: Pattern match on trait objects (&dyn Trait or Box<dyn Trait>)
//! - Smart generic type parameter filtering (only includes used type params)
//!
//! ## Example
//!
//! ```ignore
//! type_enum! {
//!     pub enum Shape {
//!         Circle { radius: f64 },
//!         Rectangle { width: f64, height: f64 },
//!     }
//! }
//! ```

mod codegen;
mod enum_parser;
mod helpers;
mod pattern_parser;
mod type_analysis;
mod variant_gen;

use proc_macro::TokenStream;
use quote::quote;
use std::collections::HashSet;

use codegen::apply_type_hint_to_pattern;
use enum_parser::ParsedEnum;
use helpers::{add_static_bounds, collect_ordered_type_params};
use pattern_parser::{extract_generics_from_type_hint, extract_type_and_pattern, parse_match_t};
use variant_gen::generate_variant_code;

//=============================================================================
// Main Macro Implementation
//=============================================================================

/// Function-like macro for converting enums to traits with struct variants.
///
/// ```ignore
/// type_enum! {
///     pub enum Either<A, E> {
///         Right(A),
///         Left(E),
///     }
/// }
/// ```
#[proc_macro]
pub fn type_enum(input: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<ParsedEnum>(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    let enum_name = &parsed.ident;
    let vis = &parsed.vis;
    let generics = &parsed.generics;

    // Collect type parameters
    let all_type_params_ordered = collect_ordered_type_params(generics);
    let all_type_params: HashSet<String> = all_type_params_ordered.iter().cloned().collect();

    // Add 'static bounds
    let generics_with_static = add_static_bounds(generics);
    let (_impl_generics_static, _, where_clause_static) = generics_with_static.split_for_impl();

    // Generate code for each variant
    let structs_and_impls: Vec<_> = parsed
        .variants
        .iter()
        .map(|variant| {
            generate_variant_code(
                variant,
                &parsed.methods,
                &generics_with_static,
                &all_type_params,
                &all_type_params_ordered,
                vis,
                enum_name,
            )
        })
        .collect();

    // Generate the trait with method declarations if present
    let trait_def = if !parsed.methods.is_empty() {
        let method_sigs: Vec<_> = parsed.methods.iter().map(|m| &m.sig).collect();
        quote! {
            #vis trait #enum_name #generics_with_static: std::any::Any #where_clause_static {
                #(#method_sigs;)*
            }
        }
    } else {
        quote! {
            #vis trait #enum_name #generics_with_static: std::any::Any #where_clause_static {}
        }
    };

    let expanded = quote! {
        #trait_def
        #(#structs_and_impls)*
    };

    TokenStream::from(expanded)
}

//=============================================================================
// Pattern Matching Macros
//=============================================================================

#[proc_macro]
pub fn match_t(input: TokenStream) -> TokenStream {
    let input_parsed = match parse_match_t(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };

    let expr = &input_parsed.expr;
    let is_move = input_parsed.is_move;
    let type_hint = &input_parsed.type_hint;

    // Extract generics from type hint if provided
    let hint_generics = type_hint
        .as_ref()
        .and_then(|hint| extract_generics_from_type_hint(hint));

    if is_move {
        // Move semantics for Box<dyn Trait>
        let type_checks = input_parsed.arms.iter().enumerate().map(|(idx, arm)| {
            let pattern = &arm.pattern;
            let (type_name, _) = extract_type_and_pattern(pattern);
            let type_name = apply_type_hint_to_pattern(type_name, &hint_generics);

            quote! {
                if (&*__expr as &dyn std::any::Any).is::<#type_name>() {
                    __matched_idx = Some(#idx);
                }
            }
        });

        let match_arms = input_parsed.arms.iter().enumerate().map(|(idx, arm)| {
            let pattern = &arm.pattern;
            let body = &arm.body;
            let (type_name, pattern_for_match) = extract_type_and_pattern(pattern);
            let type_name = apply_type_hint_to_pattern(type_name, &hint_generics);

            quote! {
                #idx => {
                    let __any_box: Box<dyn std::any::Any> = __expr;
                    if let Ok(__concrete_box) = __any_box.downcast::<#type_name>() {
                        match *__concrete_box {
                            #pattern_for_match => #body,
                            _ => panic!("Pattern match failed in match_t!")
                        }
                    } else {
                        panic!("Downcast failed in match_t!");
                    }
                }
            }
        });

        let expanded = quote! {
            {
                let __expr = #expr;
                let mut __matched_idx: Option<usize> = None;

                #(#type_checks)*

                match __matched_idx {
                    Some(__idx) => {
                        match __idx {
                            #(#match_arms,)*
                            _ => panic!("Invalid match index in match_t!")
                        }
                    }
                    None => panic!("No matching type found in match_t!")
                }
            }
        };

        TokenStream::from(expanded)
    } else {
        // Reference semantics for &dyn Trait
        let match_arms = input_parsed.arms.iter().map(|arm| {
            let pattern = &arm.pattern;
            let body = &arm.body;
            let (type_name, pattern_for_match) = extract_type_and_pattern(pattern);
            let type_name = apply_type_hint_to_pattern(type_name, &hint_generics);

            quote! {
                if let Some(__value_ref) = (&*__expr as &dyn std::any::Any).downcast_ref::<#type_name>() {
                    if let #pattern_for_match = __value_ref {
                        return Some(#body);
                    }
                }
            }
        });

        let expanded = quote! {
            {
                (|| -> Option<_> {
                    let __expr = #expr;
                    #(#match_arms)*
                    None
                })().expect("No matching type found in match_t!")
            }
        };

        TokenStream::from(expanded)
    }
}

/// A macro for type-based pattern matching on `Box<dyn Trait>` that moves values out
///
/// Syntax: match_t_box!(expr { Pattern1(fields) => expr1, Pattern2 { fields } => expr2, ... })
///
/// Unlike `match_t!`, this moves values out of the Box instead of using references.
#[proc_macro]
pub fn match_t_box(input: TokenStream) -> TokenStream {
    let input_parsed = match parse_match_t(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };

    let expr = &input_parsed.expr;

    // Generate type checks
    let type_checks = input_parsed.arms.iter().enumerate().map(|(idx, arm)| {
        let pattern = &arm.pattern;

        let (type_name, _) = extract_type_and_pattern(pattern);

        quote! {
            if (&*__expr as &dyn std::any::Any).is::<#type_name>() {
                __matched_idx = Some(#idx);
            }
        }
    });

    // Generate match arms that consume the box
    let match_arms = input_parsed.arms.iter().enumerate().map(|(idx, arm)| {
        let pattern = &arm.pattern;
        let body = &arm.body;

        let (type_name, pattern_for_match) = extract_type_and_pattern(pattern);

        quote! {
            #idx => {
                let __any_box: Box<dyn std::any::Any> = __expr;
                if let Ok(__concrete_box) = __any_box.downcast::<#type_name>() {
                    let __value = *__concrete_box;
                    if let #pattern_for_match = __value {
                        #body
                    } else {
                        panic!("Pattern match failed in match_t_box!");
                    }
                } else {
                    panic!("Downcast failed in match_t_box!");
                }
            }
        }
    });

    let expanded = quote! {
        {
            let __expr = #expr;
            let mut __matched_idx: Option<usize> = None;

            // First pass: find which type matches
            #(#type_checks)*

            // Second pass: consume the box and extract the value
            match __matched_idx {
                Some(__idx) => {
                    match __idx {
                        #(#match_arms,)*
                        _ => panic!("Invalid match index in match_t_box!")
                    }
                }
                None => panic!("No matching type found in match_t_box!")
            }
        }
    };

    TokenStream::from(expanded)
}
