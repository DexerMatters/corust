use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use syn::parse_macro_input;

mod codegen;
mod match_macro;
mod parser;
mod types;

use codegen::generate_gadt_code;
use types::GadtEnum;

/// The `g!` macro for GADT-like enums and type-based pattern matching.
///
/// Enum definition syntax:
/// ```ignore
/// g! {
///     pub enum Either<L, R, U> {
///         Left: L -> Either<L, R, U>,
///         Right: R -> Either<L, R, U>,
///         Both: L -> R -> Either<L, R, U>,
///         Exist<T>: T -> Either<L, R, U>,
///     }
/// }
/// ```
///
/// Pattern matching syntax:
/// ```ignore
/// // Reference matching
/// g!(match expr {
///     Pattern1(fields) => expr1,
///     Pattern2(fields) => expr2,
/// })
///
/// // Move matching (for Box<dyn Trait>)
/// g!(match move expr {
///     Pattern1(fields) => expr1,
///     Pattern2(fields) => expr2,
/// })
/// ```
#[proc_macro]
pub fn g(input: TokenStream) -> TokenStream {
    let input_clone = input.clone();
    let tokens = TokenStream2::from(input_clone);
    let mut iter = tokens.into_iter().peekable();

    // Check if it's a match expression or enum definition
    if let Some(TokenTree::Ident(ident)) = iter.peek() {
        if ident.to_string() == "match" {
            // It's a match expression
            return match_macro::match_impl(input);
        }
    }

    // Otherwise, it's an enum definition
    let gadt = parse_macro_input!(input as GadtEnum);
    let code = generate_gadt_code(&gadt);
    TokenStream::from(code)
}
