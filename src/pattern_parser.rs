//! Pattern matching parser utilities

use proc_macro2::TokenStream as TokenStream2;
use syn;

pub struct MatchArm {
    pub pattern: TokenStream2,
    pub body: TokenStream2,
}

pub struct MatchTInput {
    pub is_move: bool,
    pub expr: TokenStream2,
    pub type_hint: Option<TokenStream2>,
    pub arms: Vec<MatchArm>,
}

pub fn parse_match_t(input: proc_macro::TokenStream) -> syn::Result<MatchTInput> {
    use proc_macro2::{Delimiter, TokenTree};

    let tokens = TokenStream2::from(input);
    let mut iter = tokens.into_iter().peekable();

    // Check for optional 'move' keyword
    let is_move = matches!(
        iter.peek(),
        Some(TokenTree::Ident(ident)) if ident.to_string() == "move"
    );
    if is_move {
        iter.next();
    }

    // Parse the expression (everything before 'as' or the first brace)
    let (expr, type_hint) = parse_expression_and_type_hint(&mut iter)?;

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

    let arms = parse_match_arms(arms_group.stream())?;

    Ok(MatchTInput {
        is_move,
        expr,
        type_hint,
        arms,
    })
}

/// Parse expression and optional type hint (e.g., `expr as Type`)
fn parse_expression_and_type_hint(
    iter: &mut std::iter::Peekable<impl Iterator<Item = proc_macro2::TokenTree>>,
) -> syn::Result<(TokenStream2, Option<TokenStream2>)> {
    use proc_macro2::{Delimiter, TokenTree};

    let mut expr_tokens = Vec::new();
    let mut type_hint = None;

    while let Some(token) = iter.peek() {
        if matches!(token, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace) {
            break;
        }

        // Check for 'as' keyword for type hint
        if let TokenTree::Ident(ident) = token {
            if ident.to_string() == "as" {
                iter.next(); // consume 'as'

                // Parse type hint (everything until the brace)
                let mut type_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if matches!(t, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace) {
                        break;
                    }
                    type_tokens.push(iter.next().unwrap());
                }
                type_hint = Some(type_tokens.into_iter().collect());
                break;
            }
        }

        expr_tokens.push(iter.next().unwrap());
    }

    Ok((expr_tokens.into_iter().collect(), type_hint))
}

/// Parse match arms from token stream
fn parse_match_arms(tokens: TokenStream2) -> syn::Result<Vec<MatchArm>> {
    use proc_macro2::TokenTree;

    let mut arms = Vec::new();
    let mut current_pattern = Vec::new();
    let mut current_body = Vec::new();
    let mut in_body = false;

    for token in tokens {
        match &token {
            TokenTree::Punct(p) if p.as_char() == '=' && !in_body => {
                current_pattern.push(token.clone());
            }
            TokenTree::Punct(p) if p.as_char() == '>' && !current_pattern.is_empty() => {
                if let Some(TokenTree::Punct(prev)) = current_pattern.last() {
                    if prev.as_char() == '=' {
                        current_pattern.pop();
                        in_body = true;
                        continue;
                    }
                }
                current_pattern.push(token);
            }
            TokenTree::Punct(p) if p.as_char() == ',' && in_body => {
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

    // Add the last arm if present
    if !current_pattern.is_empty() || !current_body.is_empty() {
        arms.push(MatchArm {
            pattern: current_pattern.into_iter().collect(),
            body: current_body.into_iter().collect(),
        });
    }

    Ok(arms)
}

/// Extract the type name (e.g., "Circle<i32>") and the pattern (e.g., "{ radius }") from the pattern
/// Examples:
/// - `Circle(x)` -> (Circle, Circle(x))
/// - `Leaf<i32>(x)` -> (Leaf<i32>, Leaf(x))
/// - `Rectangle { width, height }` -> (Rectangle, Rectangle { width, height })
/// Returns: (type_name_for_downcast, pattern_without_generics)
pub fn extract_type_and_pattern(pattern: &TokenStream2) -> (TokenStream2, TokenStream2) {
    use proc_macro2::{Delimiter, TokenTree};

    let mut type_name_tokens = Vec::new();
    let mut angle_bracket_depth = 0;

    // First pass: extract type name with generics (everything before ( or { )
    for token in pattern.clone() {
        match &token {
            // Stop at tuple fields ( or struct fields {
            TokenTree::Group(g)
                if g.delimiter() == Delimiter::Parenthesis || g.delimiter() == Delimiter::Brace =>
            {
                break;
            }
            // Track angle bracket depth
            TokenTree::Punct(p) if p.as_char() == '<' => {
                angle_bracket_depth += 1;
                type_name_tokens.push(token);
            }
            TokenTree::Punct(p) if p.as_char() == '>' && angle_bracket_depth > 0 => {
                type_name_tokens.push(token);
                angle_bracket_depth -= 1;
            }
            // Stop at other punctuation if not in angle brackets
            TokenTree::Punct(_) if angle_bracket_depth == 0 => break,
            _ => {
                type_name_tokens.push(token);
            }
        }
    }

    // Second pass: build pattern without generics
    let mut pattern_without_generics = Vec::new();
    let mut skip_until_angle_close = false;
    let mut angle_depth = 0;

    for token in pattern.clone() {
        match &token {
            TokenTree::Punct(p) if p.as_char() == '<' && !skip_until_angle_close => {
                skip_until_angle_close = true;
                angle_depth = 1;
            }
            TokenTree::Punct(p) if p.as_char() == '<' && skip_until_angle_close => {
                angle_depth += 1;
            }
            TokenTree::Punct(p) if p.as_char() == '>' && skip_until_angle_close => {
                angle_depth -= 1;
                if angle_depth == 0 {
                    skip_until_angle_close = false;
                }
            }
            _ if !skip_until_angle_close => {
                pattern_without_generics.push(token);
            }
            _ => {} // Skip tokens inside angle brackets
        }
    }

    (
        type_name_tokens.into_iter().collect(),
        pattern_without_generics.into_iter().collect(),
    )
}

/// Extract generic type parameters from a type hint like `Tree<i32>` or `Box<dyn Tree<i32>>`
/// Returns the generic parameters as a TokenStream, e.g., `<i32>`
pub fn extract_generics_from_type_hint(type_hint: &TokenStream2) -> Option<TokenStream2> {
    use proc_macro2::TokenTree;

    let mut generics_tokens = Vec::new();
    let mut depth = 0;
    let mut collecting = false;

    for token in type_hint.clone() {
        match &token {
            TokenTree::Punct(p) if p.as_char() == '<' => {
                depth += 1;
                collecting = true;
                generics_tokens.push(token);
            }
            TokenTree::Punct(p) if p.as_char() == '>' && depth > 0 => {
                generics_tokens.push(token);
                depth -= 1;
                if depth == 0 {
                    return Some(generics_tokens.into_iter().collect());
                }
            }
            _ if collecting => {
                generics_tokens.push(token);
            }
            _ => {}
        }
    }

    if !generics_tokens.is_empty() {
        Some(generics_tokens.into_iter().collect())
    } else {
        None
    }
}
