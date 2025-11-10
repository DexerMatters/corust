use proc_macro::TokenStream;
use proc_macro2::{Delimiter, TokenStream as TokenStream2, TokenTree};
use quote::quote;

struct MatchArm {
    pattern: TokenStream2,
    body: TokenStream2,
}

struct MatchInput {
    is_move: bool,
    expr: TokenStream2,
    arms: Vec<MatchArm>,
}

/// Parse match expression from g!(match ...) or g!(match move ...)
pub fn match_impl(input: TokenStream) -> TokenStream {
    let input_parsed = match parse_match_expr(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };

    let expr = &input_parsed.expr;
    let is_move = input_parsed.is_move;

    if is_move {
        generate_move_match(expr, &input_parsed.arms)
    } else {
        generate_ref_match(expr, &input_parsed.arms)
    }
}

fn parse_match_expr(input: TokenStream) -> syn::Result<MatchInput> {
    let tokens = TokenStream2::from(input);
    let mut iter = tokens.into_iter().peekable();

    // Parse "match" keyword
    match iter.next() {
        Some(TokenTree::Ident(ident)) if ident.to_string() == "match" => {}
        _ => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "Expected 'match' keyword",
            ));
        }
    }

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
    let arms = parse_match_arms(arms_group.stream())?;

    Ok(MatchInput {
        is_move,
        expr,
        arms,
    })
}

fn parse_match_arms(arms_tokens: TokenStream2) -> syn::Result<Vec<MatchArm>> {
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

    Ok(arms)
}

fn generate_move_match(expr: &TokenStream2, arms: &[MatchArm]) -> TokenStream {
    // Move semantics for Box<dyn Trait>
    let type_checks = arms.iter().enumerate().map(|(idx, arm)| {
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

    let match_arms = arms.iter().enumerate().map(|(idx, arm)| {
        let pattern = &arm.pattern;
        let body = &arm.body;

        let type_name: TokenStream2 = pattern
            .clone()
            .into_iter()
            .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
            .collect();

        // Transform pattern to add `..` if it's a tuple/struct pattern
        let transformed_pattern = transform_pattern_add_rest(pattern.clone());

        quote! {
            #idx => {
                let __any_box: Box<dyn std::any::Any> = __expr;
                if let Ok(__concrete_box) = __any_box.downcast::<#type_name>() {
                    match *__concrete_box {
                        #transformed_pattern => #body,
                        _ => panic!("Pattern match failed in g!(match move)!")
                    }
                } else {
                    panic!("Downcast failed in g!(match move)!");
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
                        _ => panic!("Invalid match index in g!(match move)!")
                    }
                }
                None => panic!("No matching type found in g!(match move)!")
            }
        }
    };

    TokenStream::from(expanded)
}

fn generate_ref_match(expr: &TokenStream2, arms: &[MatchArm]) -> TokenStream {
    // Reference semantics for &dyn Trait
    // Use the __as_variant methods to extract values without downcasting
    let match_arms = arms.iter().map(|arm| {
        let pattern = &arm.pattern;
        let body = &arm.body;

        let variant_name: TokenStream2 = pattern
            .clone()
            .into_iter()
            .take_while(|t| !matches!(t, TokenTree::Group(_) | TokenTree::Punct(_)))
            .collect();

        // Convert variant name to lowercase for method name
        let variant_name_str = variant_name.to_string().to_lowercase();
        let method_name = proc_macro2::Ident::new(
            &format!("__as_{}", variant_name_str),
            proc_macro2::Span::call_site(),
        );

        // Extract the pattern from inside the parentheses
        // For patterns like Left(_) or Exist(v), extract the inner part
        let (inner_pattern, type_hint) = extract_inner_pattern_and_type(pattern.clone());
        
        // Check if the pattern is a simple binding (identifier) or wildcard
        let inner_str = inner_pattern.to_string();
        let is_simple_binding = !inner_str.contains(',') && !inner_str.contains('(');

        if is_simple_binding && inner_str.trim() != "_" {
            // It's a simple binding like `v` - bind directly without pattern matching
            // This handles the case where __variant_data might be &dyn Any
            
            // Check if the body is just dereferencing the binding (e.g., *v)
            // If so, just return the reference directly without the dereference
            let body_str = body.to_string();
            let binding_name = inner_str.trim();
            let deref_pattern = format!("* {}", binding_name);
            
            // Check if body is just the binding (e.g., v)
            if body_str.trim() == binding_name {
                // Body is just the binding, need to downcast &dyn Any to concrete type
                // If there's a type hint, use it directly. Otherwise, use type inference.
                if let Some(target_type) = type_hint {
                    // User provided explicit type: v as &u8
                    quote! {
                        if let Some(__variant_data) = __expr.#method_name() {
                            let #inner_pattern: #target_type = __variant_data.downcast_ref().expect("Type mismatch in pattern match");
                            return Some(#body);
                        }
                    }
                } else {
                    // No type hint, use inference from return type
                    quote! {
                        if let Some(__variant_data) = __expr.#method_name() {
                            // Use a helper that downcasts based on return type inference
                            fn __downcast_ref<'a, T: 'static>(any_ref: &'a dyn std::any::Any) -> &'a T {
                                any_ref.downcast_ref::<T>().expect("Type mismatch in existential pattern match")
                            }
                            return Some(__downcast_ref(__variant_data));
                        }
                    }
                }
            } else if body_str.trim() == deref_pattern.trim() || 
                      body_str.trim() == format!("*{}", binding_name) {
                // Body is *binding, we need to downcast and dereference
                if let Some(target_type) = type_hint {
                    // User provided explicit type
                    quote! {
                        if let Some(__variant_data) = __expr.#method_name() {
                            let #inner_pattern: #target_type = __variant_data.downcast_ref().expect("Type mismatch in pattern match");
                            return Some(*#inner_pattern);
                        }
                    }
                } else {
                    quote! {
                        if let Some(__variant_data) = __expr.#method_name() {
                            // Downcast and dereference
                            fn __extract<T: 'static + Copy>(any_ref: &dyn std::any::Any) -> T {
                                *any_ref.downcast_ref::<T>().expect("Type mismatch in pattern match")
                            }
                            return Some(__extract(__variant_data));
                        }
                    }
                }
            } else {
                // Body uses the binding in some other way (e.g., v + 1)
                if let Some(target_type) = type_hint {
                    // User provided explicit type - bind with that type
                    quote! {
                        if let Some(__variant_data) = __expr.#method_name() {
                            let #inner_pattern: #target_type = __variant_data.downcast_ref().expect("Type mismatch in pattern match");
                            return Some(#body);
                        }
                    }
                } else {
                    // No type hint - bind to &dyn Any (will likely fail if used)
                    quote! {
                        if let Some(__variant_data) = __expr.#method_name() {
                            let #inner_pattern = __variant_data;
                            return Some(#body);
                        }
                    }
                }
            }
        } else {
            // It's a wildcard or complex pattern - use pattern matching
            quote! {
                if let Some(__variant_data) = __expr.#method_name() {
                    if let #inner_pattern = __variant_data {
                        return Some(#body);
                    }
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
            })().expect("No matching type found in g!(match)!")
        }
    };

    TokenStream::from(expanded)
}

/// Extract the inner pattern from a variant pattern and any type ascription
/// E.g., Left(v) => (v, None), Exist(v as &u8) => (v, Some(&u8))
fn extract_inner_pattern_and_type(pattern: TokenStream2) -> (TokenStream2, Option<TokenStream2>) {
    let tokens: Vec<TokenTree> = pattern.into_iter().collect();
    
    // Find the group (parentheses)
    for token in tokens {
        if let TokenTree::Group(group) = token {
            if group.delimiter() == Delimiter::Parenthesis {
                let inner_tokens: Vec<TokenTree> = group.stream().into_iter().collect();
                
                // Check if there's a type ascription (pattern: "binding as Type")
                // Look for 'as' keyword
                let mut as_pos = None;
                for (i, token) in inner_tokens.iter().enumerate() {
                    if let TokenTree::Ident(ident) = token {
                        if ident.to_string() == "as" {
                            as_pos = Some(i);
                            break;
                        }
                    }
                }
                
                if let Some(pos) = as_pos {
                    // Split into binding and type
                    let binding: TokenStream2 = inner_tokens[..pos].iter().cloned().collect();
                    let type_tokens: TokenStream2 = inner_tokens[pos + 1..].iter().cloned().collect();
                    return (binding, Some(type_tokens));
                } else {
                    return (group.stream(), None);
                }
            }
        }
    }
    
    // If no group found, return empty (for unit variants)
    (TokenStream2::new(), None)
}

/// Transform a pattern to add `..` before the closing delimiter to ignore PhantomData
fn transform_pattern_add_rest(pattern: TokenStream2) -> TokenStream2 {
    let mut tokens: Vec<TokenTree> = pattern.into_iter().collect();

    // Find the last group (tuple or struct pattern)
    for i in (0..tokens.len()).rev() {
        if let TokenTree::Group(group) = &tokens[i] {
            let mut inner_tokens: Vec<TokenTree> = group.stream().into_iter().collect();

            // Add `..` before the closing parenthesis/brace if not already present
            let has_rest = inner_tokens.iter().any(|t| {
                matches!(t, TokenTree::Punct(p) if p.as_char() == '.' && p.spacing() == proc_macro2::Spacing::Joint)
            });

            if !has_rest && !inner_tokens.is_empty() {
                // Add comma if the last token isn't already a comma
                if !matches!(inner_tokens.last(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
                    inner_tokens.push(TokenTree::Punct(proc_macro2::Punct::new(
                        ',',
                        proc_macro2::Spacing::Alone,
                    )));
                }
                // Add ..
                inner_tokens.push(TokenTree::Punct(proc_macro2::Punct::new(
                    '.',
                    proc_macro2::Spacing::Joint,
                )));
                inner_tokens.push(TokenTree::Punct(proc_macro2::Punct::new(
                    '.',
                    proc_macro2::Spacing::Alone,
                )));
            }

            let new_stream: TokenStream2 = inner_tokens.into_iter().collect();
            tokens[i] = TokenTree::Group(proc_macro2::Group::new(group.delimiter(), new_stream));
            break;
        }
    }

    tokens.into_iter().collect()
}
