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

/// Function-like macro for converting enums to traits with struct variants.
/// It supports optional type indexing per variant and method definitions with
/// pattern/body arms and existential return types.
///
/// # Example
///
/// Lift an enum definition into a trait with struct variants.
///
/// ```ignore
/// type_enum! {
///     pub enum Either<A, E> {
///         Right(A),
///         Left(E),
///     }
/// }
/// ```
///
/// Or with indexed types. It is a feature similar to GADTs in other languages,
/// where each variant can refine the overall type with specific type arguments.
///
/// ```ignore
/// type_enum! {
///    enum Expr<T> {
///       LitInt(i32) : Expr<i32>,
///       LitBool(bool) : Expr<bool>,
///       Add(Box<Expr<i32>>, Box<Expr<i32>>) : Expr<i32>,
///       Or(Box<Expr<bool>>, Box<Expr<bool>>) : Expr<bool>,
///    }
/// }
/// ```
///
/// Or with functions using existential return types
///
/// ```ignore
/// type_enum! {
///    enum Expr<T> { ... }
///
///    fn eval(&self) -> T {
///       LitInt(i) => *i,
///       LitBool(b) => *b,
///       Add(lhs, rhs) => lhs.eval() + rhs.eval(),
///       Or(lhs, rhs) => lhs.eval() || rhs.eval(),
///    }
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

    let all_type_params_ordered = collect_ordered_type_params(generics);
    let all_type_params: HashSet<String> = all_type_params_ordered.iter().cloned().collect();

    let generics_with_static = add_static_bounds(generics);
    let (_impl_generics_static, _, where_clause_static) = generics_with_static.split_for_impl();

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

/// Pattern match on trait objects based on their concrete types.
/// It supports both reference (`&dyn Trait`) and boxed (`Box<dyn Trait>`)
/// trait objects.
///
/// Use `move` keyword to indicate ownership transfer when matching on `Box<dyn Trait>`.
///
/// # Example
///
/// ```ignore
/// type_enum! {
///     enum Tree<T: Display> {
///         Leaf(T),
///         Node(Box<Tree<T>>, Box<Tree<T>>),
///     }
/// }
///
/// let tree: Box<dyn Tree<i32>> = Box::new(...);
/// let tree_ref: &dyn Tree<i32> = &...;
/// let describe = match_t! {
///     move tree {
///         Leaf(value) => format!("Leaf: {}", value),
///         Node(left, right) => format!("Node with left and right"),
///     }
/// }
/// let describe_ref = match_t! {
///     tree_ref {
///         Leaf(value) => format!("Leaf: {}", value),
///         Node(left, right) => format!("Node with left and right"),
///     }
/// }
/// ```
#[proc_macro]
pub fn match_t(input: TokenStream) -> TokenStream {
    let input_parsed = match parse_match_t(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };

    let expr = &input_parsed.expr;
    let is_move = input_parsed.is_move;
    let type_hint = &input_parsed.type_hint;

    let hint_generics = type_hint
        .as_ref()
        .and_then(|hint| extract_generics_from_type_hint(hint));

    if is_move {
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
