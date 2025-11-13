//! Custom enum parser for tagless final style syntax

use proc_macro2::{Ident, TokenStream as TokenStream2, TokenTree};
use syn::{
    parse::{Parse, ParseStream},
    Attribute, Fields, Generics, Token, Visibility,
};

/// Parsed variant with optional trait type constraint
pub struct ParsedVariant {
    pub attrs: Vec<Attribute>,
    pub ident: Ident,
    pub generics: Generics,
    pub fields: Fields,
    pub trait_type: Option<TokenStream2>,
}

/// A single method arm (pattern => body)
pub struct MethodArm {
    pub pattern: TokenStream2,
    pub body: TokenStream2,
}

/// Parsed method with signature and pattern/body arms
pub struct ParsedMethod {
    pub sig: TokenStream2,
    pub arms: Vec<MethodArm>,
}

pub struct ParsedEnum {
    #[allow(dead_code)]
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub ident: Ident,
    pub generics: Generics,
    pub variants: Vec<ParsedVariant>,
    pub methods: Vec<ParsedMethod>,
}

impl Parse for ParsedEnum {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;

        // Accept either 'enum' or 'trait' keyword
        let lookahead = input.lookahead1();
        if lookahead.peek(Token![enum]) {
            input.parse::<Token![enum]>()?;
        } else if lookahead.peek(Token![trait]) {
            input.parse::<Token![trait]>()?;
        } else {
            return Err(lookahead.error());
        }

        let ident = input.parse()?;
        let generics = input.parse()?;

        let content;
        syn::braced!(content in input);

        let mut variants = Vec::new();

        while !content.is_empty() {
            let variant_attrs = content.call(Attribute::parse_outer)?;
            let variant_ident: Ident = content.parse()?;

            // Parse variant-level generics (e.g., A<T>, B<U: Trait>)
            let variant_generics: Generics = content.parse()?;

            // Parse fields
            let fields = if content.peek(syn::token::Brace) {
                Fields::Named(content.parse()?)
            } else if content.peek(syn::token::Paren) {
                Fields::Unnamed(content.parse()?)
            } else {
                Fields::Unit
            };

            // Check for trait type constraint (: Type)
            let trait_type = if content.peek(Token![:]) {
                content.parse::<Token![:]>()?;

                // Parse everything until comma or end, respecting angle brackets
                let mut type_tokens = Vec::new();
                let mut angle_depth: i32 = 0;
                while !content.is_empty() {
                    // Check if we're at a comma at depth 0
                    if angle_depth == 0 && content.peek(Token![,]) {
                        break;
                    }

                    let token = content.parse::<TokenTree>()?;

                    // Track angle bracket depth
                    if let TokenTree::Punct(ref punct) = token {
                        match punct.as_char() {
                            '<' => angle_depth += 1,
                            '>' => angle_depth = angle_depth.saturating_sub(1),
                            _ => {}
                        }
                    }

                    type_tokens.push(token);
                }

                Some(type_tokens.into_iter().collect())
            } else {
                None
            };

            variants.push(ParsedVariant {
                attrs: variant_attrs,
                ident: variant_ident,
                generics: variant_generics,
                fields,
                trait_type,
            });

            // Optional trailing comma
            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }

        // Now parse method definitions (if present) from remaining input
        let mut methods = Vec::new();
        while !input.is_empty() {
            methods.push(parse_method(input)?);
        }

        Ok(ParsedEnum {
            attrs,
            vis,
            ident,
            generics,
            variants,
            methods,
        })
    }
}

fn parse_method(input: ParseStream) -> syn::Result<ParsedMethod> {
    // Parse the method signature: fn name(...) -> ReturnType
    let mut sig_tokens = Vec::new();

    // Collect tokens until we hit the opening brace
    while !input.is_empty() && !input.peek(syn::token::Brace) {
        sig_tokens.push(input.parse::<TokenTree>()?);
    }

    let sig: TokenStream2 = sig_tokens.into_iter().collect();

    // Parse the method body (pattern => body pairs)
    let content;
    syn::braced!(content in input);

    let mut arms = Vec::new();

    while !content.is_empty() {
        // Parse pattern: everything until =>
        // Need to skip over <...> angle bracket pairs
        let mut pattern_tokens = Vec::new();
        let mut angle_depth: i32 = 0;

        while !content.is_empty() {
            // Peek at the next token to check for =>
            if content.peek(Token![=>]) && angle_depth == 0 {
                break;
            }

            let tt = content.parse::<TokenTree>()?;

            // Track angle bracket depth for generic type parameters in patterns
            match &tt {
                TokenTree::Punct(p) if p.as_char() == '<' => angle_depth += 1,
                TokenTree::Punct(p) if p.as_char() == '>' => angle_depth = (angle_depth - 1).max(0),
                _ => {}
            }

            pattern_tokens.push(tt);
        }

        if content.is_empty() {
            break;
        }

        content.parse::<Token![=>]>()?;

        // Parse body: everything until comma (at the same nesting level)
        let mut body_tokens = Vec::new();
        while !content.is_empty() {
            if content.peek(Token![,]) {
                break;
            }

            let tt = content.parse::<TokenTree>()?;
            body_tokens.push(tt);
        }

        // Consume trailing comma if present
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }

        let pattern: TokenStream2 = pattern_tokens.into_iter().collect();
        let body: TokenStream2 = body_tokens.into_iter().collect();

        arms.push(MethodArm { pattern, body });
    }

    Ok(ParsedMethod { sig, arms })
}
