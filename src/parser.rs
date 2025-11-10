use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use syn::{
    Generics, Ident, Result, Token, Visibility,
    parse::{Parse, ParseStream},
};

use crate::types::{GadtEnum, VariantDef};

impl Parse for GadtEnum {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse visibility
        let vis: Visibility = input.parse()?;

        // Parse "enum"
        input.parse::<Token![enum]>()?;

        // Parse enum name
        let name: Ident = input.parse()?;

        // Parse generics
        let enum_generics: Generics = input.parse()?;

        // Parse the braced content
        let content;
        syn::braced!(content in input);

        // Parse variants
        let mut variants = Vec::new();
        while !content.is_empty() {
            variants.push(parse_variant(&content)?);

            // Optional trailing comma
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(GadtEnum {
            vis,
            name,
            enum_generics,
            variants,
        })
    }
}

fn parse_variant(input: ParseStream) -> Result<VariantDef> {
    // Parse variant name
    let name: Ident = input.parse()?;

    // Parse optional variant-specific generics (e.g., <T>)
    let variant_generics = if input.peek(Token![<]) {
        input.parse::<Token![<]>()?;
        let mut generics = Vec::new();
        loop {
            generics.push(input.parse::<Ident>()?);
            if !input.peek(Token![,]) {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        input.parse::<Token![>]>()?;
        generics
    } else {
        Vec::new()
    };

    // Parse colon
    input.parse::<Token![:]>()?;

    // Parse parameter types and arrows until we find the final return type
    let mut param_types = Vec::new();
    let mut return_type = TokenStream2::new();

    // Collect tokens until we hit a comma or end
    let mut current_tokens = Vec::new();
    let mut depth = 0; // Track angle bracket depth

    while !input.is_empty() {
        // Check if we're at a comma outside of angle brackets
        if input.peek(Token![,]) && depth == 0 {
            break;
        }

        if input.peek(Token![->]) {
            // Save current tokens as a parameter type
            if !current_tokens.is_empty() {
                param_types.push(current_tokens.drain(..).collect());
            }
            input.parse::<Token![->]>()?;
        } else if input.peek(Token![<]) {
            depth += 1;
            current_tokens.push(input.parse::<TokenTree>()?);
        } else if input.peek(Token![>]) {
            depth -= 1;
            current_tokens.push(input.parse::<TokenTree>()?);
        } else if input.peek(Token![,]) {
            // Comma inside angle brackets
            current_tokens.push(input.parse::<TokenTree>()?);
        } else {
            current_tokens.push(input.parse::<TokenTree>()?);
        }
    }

    // The last collected tokens are the return type
    if !current_tokens.is_empty() {
        return_type = current_tokens.into_iter().collect();
    }

    Ok(VariantDef {
        name,
        variant_generics,
        param_types,
        return_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_parse_simple_variant() {
        let input: GadtEnum = parse_quote! {
            pub enum Either<L, R> {
                Left: L -> Either<L, R>,
                Right: R -> Either<L, R>
            }
        };

        assert_eq!(input.name.to_string(), "Either");
        assert_eq!(input.variants.len(), 2);
        assert_eq!(input.variants[0].name.to_string(), "Left");
        assert_eq!(input.variants[0].param_types.len(), 1);
    }
}
