use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_attribute]
pub fn type_enum(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let enum_name = &input.ident;
    let vis = &input.vis;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Extract enum variants
    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => {
            return syn::Error::new_spanned(&input.ident, "type_enum can only be used on enums")
                .to_compile_error()
                .into();
        }
    };

    // Generate structs and impls for each variant
    let structs_and_impls = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        let variant_vis = vis; // Use same visibility as enum

        match &variant.fields {
            Fields::Named(fields) => {
                quote! {
                    #variant_vis struct #variant_name #fields
                    impl #impl_generics #enum_name #ty_generics for #variant_name #where_clause {}
                }
            }
            Fields::Unnamed(fields) => {
                quote! {
                    #variant_vis struct #variant_name #fields;
                    impl #impl_generics #enum_name #ty_generics for #variant_name #where_clause {}
                }
            }
            Fields::Unit => {
                quote! {
                    #variant_vis struct #variant_name;
                    impl #impl_generics #enum_name #ty_generics for #variant_name #where_clause {}
                }
            }
        }
    });

    // Generate the trait
    let expanded = quote! {
        #vis trait #enum_name #generics : std::any::Any #where_clause {}

        #(#structs_and_impls)*
    };

    TokenStream::from(expanded)
}

// Simpler approach: parse as token streams
struct MatchArm {
    pattern: TokenStream2,
    body: TokenStream2,
}

struct MatchTInput {
    is_move: bool,
    expr: TokenStream2,
    arms: Vec<MatchArm>,
}

fn parse_match_t(input: TokenStream) -> syn::Result<MatchTInput> {
    use proc_macro2::{Delimiter, TokenTree};

    let tokens = TokenStream2::from(input);
    let mut iter = tokens.into_iter().peekable();

    // Check for optional 'move' keyword
    let is_move = if let Some(TokenTree::Ident(ident)) = iter.peek() {
        if ident.to_string() == "move" {
            iter.next(); // consume 'move'
            true
        } else {
            false
        }
    } else {
        false
    };

    // Parse the expression (everything before the first brace)
    let mut expr_tokens = Vec::new();
    while let Some(token) = iter.peek() {
        if matches!(token, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace) {
            break;
        }
        expr_tokens.push(iter.next().unwrap());
    }
    let expr: TokenStream2 = expr_tokens.into_iter().collect();

    // Parse the brace group containing arms
    let arms_group = match iter.next() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => g,
        _ => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "Expected braced block with match arms",
            ));
        }
    };

    // Parse arms from the group
    let arms_tokens = arms_group.stream();
    let mut arms = Vec::new();
    let mut current_pattern = Vec::new();
    let mut current_body = Vec::new();
    let mut in_body = false;

    for token in arms_tokens {
        match &token {
            TokenTree::Punct(p) if p.as_char() == '=' && !in_body => {
                // This might be part of =>
                current_pattern.push(token.clone());
            }
            TokenTree::Punct(p) if p.as_char() == '>' && !current_pattern.is_empty() => {
                // Check if previous was =
                if let Some(TokenTree::Punct(prev)) = current_pattern.last() {
                    if prev.as_char() == '=' {
                        // Remove the = from pattern
                        current_pattern.pop();
                        in_body = true;
                        continue;
                    }
                }
                current_pattern.push(token);
            }
            TokenTree::Punct(p) if p.as_char() == ',' && in_body => {
                // End of arm
                arms.push(MatchArm {
                    pattern: current_pattern.clone().into_iter().collect(),
                    body: current_body.clone().into_iter().collect(),
                });
                current_pattern.clear();
                current_body.clear();
                in_body = false;
            }
            _ => {
                if in_body {
                    current_body.push(token);
                } else {
                    current_pattern.push(token);
                }
            }
        }
    }

    // Don't forget the last arm
    if !current_pattern.is_empty() || !current_body.is_empty() {
        arms.push(MatchArm {
            pattern: current_pattern.into_iter().collect(),
            body: current_body.into_iter().collect(),
        });
    }

    Ok(MatchTInput {
        is_move,
        expr,
        arms,
    })
}

/// A macro for type-based pattern matching on trait objects
///
/// Syntax:
/// - `match_t!(expr { Pattern1(fields) => expr1, ... })` for `&dyn Trait` (uses references)
/// - `match_t!(move expr { Pattern1(fields) => expr1, ... })` for `Box<dyn Trait>` (moves values)
#[proc_macro]
pub fn match_t(input: TokenStream) -> TokenStream {
    let input_parsed = match parse_match_t(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };

    let expr = &input_parsed.expr;
    let is_move = input_parsed.is_move;

    if is_move {
        // Move semantics for Box<dyn Trait>
        let type_checks = input_parsed.arms.iter().enumerate().map(|(idx, arm)| {
            let pattern = &arm.pattern;

            let type_name: TokenStream2 = pattern
                .clone()
                .into_iter()
                .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
                .collect();

            quote! {
                if (&*__expr as &dyn std::any::Any).is::<#type_name>() {
                    __matched_idx = Some(#idx);
                }
            }
        });

        let match_arms = input_parsed.arms.iter().enumerate().map(|(idx, arm)| {
            let pattern = &arm.pattern;
            let body = &arm.body;

            let type_name: TokenStream2 = pattern
                .clone()
                .into_iter()
                .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
                .collect();

            quote! {
                #idx => {
                    let __any_box: Box<dyn std::any::Any> = __expr;
                    if let Ok(__concrete_box) = __any_box.downcast::<#type_name>() {
                        match *__concrete_box {
                            #pattern => #body,
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

            let type_name: TokenStream2 = pattern
                .clone()
                .into_iter()
                .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
                .collect();

            quote! {
                if let Some(__value_ref) = (&*__expr as &dyn std::any::Any).downcast_ref::<#type_name>() {
                    if let #pattern = __value_ref {
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

        let type_name: TokenStream2 = pattern
            .clone()
            .into_iter()
            .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
            .collect();

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

        let type_name: TokenStream2 = pattern
            .clone()
            .into_iter()
            .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
            .collect();

        quote! {
            #idx => {
                let __any_box: Box<dyn std::any::Any> = __expr;
                if let Ok(__concrete_box) = __any_box.downcast::<#type_name>() {
                    let __value = *__concrete_box;
                    if let #pattern = __value {
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
