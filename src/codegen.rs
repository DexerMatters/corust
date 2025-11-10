use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Ident;

use crate::types::{GadtEnum, VariantDef};

pub fn generate_gadt_code(gadt: &GadtEnum) -> TokenStream2 {
    let trait_def = generate_trait(gadt);
    let struct_defs = generate_structs(gadt);
    let match_macro = generate_match_macro(gadt);

    quote! {
        #trait_def
        #(#struct_defs)*
        #match_macro
    }
}

fn generate_match_macro(gadt: &GadtEnum) -> TokenStream2 {
    // Generate a declarative macro for matching this specific enum
    let enum_name = &gadt.name;
    let enum_name_lower = Ident::new(
        &enum_name.to_string().to_lowercase(),
        proc_macro2::Span::call_site(),
    );
    let macro_name = Ident::new(
        &format!("{}_match", enum_name_lower),
        proc_macro2::Span::call_site(),
    );

    let variant_names: Vec<_> = gadt.variants.iter().map(|v| &v.name).collect();

    // Get type parameter names
    let type_params: Vec<_> = gadt
        .enum_generics
        .type_params()
        .map(|tp| &tp.ident)
        .collect();
    let type_param_list = if !type_params.is_empty() {
        quote! { <#(#type_params),*> }
    } else {
        quote! {}
    };

    // Generate downcast attempts for each variant
    let variant_arms = variant_names.iter().map(|variant_name| {
        quote! {
            if let Some(__val) = (__expr as &dyn std::any::Any).downcast_ref::<#variant_name #type_param_list>() {
                // Now match the user's patterns against __val
                // But we still can't do this generically...
            }
        }
    });

    // This approach still doesn't solve the problem
    quote! {
        // TODO: generate enum-specific match macro
    }
}

fn generate_trait(gadt: &GadtEnum) -> TokenStream2 {
    let vis = &gadt.vis;
    let name = &gadt.name;
    let generics = &gadt.enum_generics;
    let (_impl_generics, _ty_generics, where_clause) = generics.split_for_impl();

    // Add 'static bounds for all type parameters
    let type_params: Vec<_> = generics.type_params().collect();
    let static_bounds = type_params.iter().map(|tp| {
        let ident = &tp.ident;
        quote! { #ident: 'static }
    });

    let trait_where_clause = if !type_params.is_empty() {
        if let Some(where_clause) = where_clause {
            let existing = &where_clause.predicates;
            quote! { where #existing, #(#static_bounds),* }
        } else {
            quote! { where #(#static_bounds),* }
        }
    } else {
        quote! { #where_clause }
    };

    // Generate as_variant methods for each variant
    // These allow pattern matching without downcasting
    // For variants with variant-specific generics, return &dyn Any since we can't have generic trait methods
    let variant_methods = gadt.variants.iter().map(|variant| {
        let variant_name = &variant.name;
        let variant_name_lower = Ident::new(
            &variant_name.to_string().to_lowercase(),
            proc_macro2::Span::call_site(),
        );
        let method_name = Ident::new(
            &format!("__as_{}", variant_name_lower),
            proc_macro2::Span::call_site(),
        );

        // Return a tuple of references to the variant's fields
        // If there are variant-specific generics, we must return &dyn Any for each field
        let param_count = variant.param_types.len();
        let has_variant_generics = !variant.variant_generics.is_empty();
        
        if param_count == 0 {
            quote! {
                #[doc(hidden)]
                fn #method_name(&self) -> Option<()> { None }
            }
        } else if param_count == 1 && !has_variant_generics {
            let param_type = &variant.param_types[0];
            quote! {
                #[doc(hidden)]
                fn #method_name(&self) -> Option<&#param_type> { None }
            }
        } else if param_count == 1 && has_variant_generics {
            // Return &dyn Any for variant-generic fields
            quote! {
                #[doc(hidden)]
                fn #method_name(&self) -> Option<&dyn std::any::Any> { None }
            }
        } else if !has_variant_generics {
            let param_types = &variant.param_types;
            quote! {
                #[doc(hidden)]
                fn #method_name(&self) -> Option<(#(&#param_types),*)> { None }
            }
        } else {
            // Multiple fields with variant generics - return tuple of &dyn Any
            let any_refs = (0..param_count).map(|_| quote! { &dyn std::any::Any });
            quote! {
                #[doc(hidden)]
                fn #method_name(&self) -> Option<(#(#any_refs),*)> { None }
            }
        }
    });

    quote! {
        #vis trait #name #generics : std::any::Any #trait_where_clause {
            #(#variant_methods)*
        }
    }
}

fn generate_structs(gadt: &GadtEnum) -> Vec<TokenStream2> {
    gadt.variants
        .iter()
        .map(|variant| generate_variant_struct(gadt, variant))
        .collect()
}

fn generate_variant_struct(gadt: &GadtEnum, variant: &VariantDef) -> TokenStream2 {
    let vis = &gadt.vis;
    let variant_name = &variant.name;
    let trait_name = &gadt.name;

    // Combine enum generics + variant-specific generics
    let all_type_params = combine_generics(&gadt.enum_generics, &variant.variant_generics);
    let enum_type_params: Vec<_> = gadt
        .enum_generics
        .type_params()
        .map(|tp| &tp.ident)
        .collect();

    // Build struct generics
    let struct_generics = if !all_type_params.is_empty() {
        quote! { < #(#all_type_params),* > }
    } else {
        quote! {}
    };

    // Build impl generics with bounds
    let impl_generics = if !all_type_params.is_empty() {
        quote! { < #(#all_type_params: 'static),* > }
    } else {
        quote! {}
    };

    // Build trait type generics (only enum-level generics)
    let trait_ty_generics = if !enum_type_params.is_empty() {
        quote! { < #(#enum_type_params),* > }
    } else {
        quote! {}
    };

    // Generate struct fields
    let param_count = variant.param_types.len();
    let param_types = &variant.param_types;

    // Generate accessor method implementation for this variant
    let variant_name_lower = Ident::new(
        &variant_name.to_string().to_lowercase(),
        proc_macro2::Span::call_site(),
    );
    let method_name = Ident::new(
        &format!("__as_{}", variant_name_lower),
        proc_macro2::Span::call_site(),
    );

    let has_variant_generics = !variant.variant_generics.is_empty();

    let accessor_impl = if param_count == 0 {
        quote! {
            fn #method_name(&self) -> Option<()> { Some(()) }
        }
    } else if param_count == 1 && !has_variant_generics {
        let param_type = &param_types[0];
        quote! {
            fn #method_name(&self) -> Option<&#param_type> { Some(&self.0) }
        }
    } else if param_count == 1 && has_variant_generics {
        // Return &dyn Any for variant-generic fields
        quote! {
            fn #method_name(&self) -> Option<&dyn std::any::Any> { Some(&self.0) }
        }
    } else if !has_variant_generics {
        let field_refs: Vec<_> = (0..param_count)
            .map(|i| {
                let idx = syn::Index::from(i);
                quote! { &self.#idx }
            })
            .collect();
        quote! {
            fn #method_name(&self) -> Option<(#(&#param_types),*)> {
                Some((#(#field_refs),*))
            }
        }
    } else {
        // Multiple fields with variant generics - return tuple of &dyn Any
        let field_refs: Vec<_> = (0..param_count)
            .map(|i| {
                let idx = syn::Index::from(i);
                quote! { &self.#idx as &dyn std::any::Any }
            })
            .collect();
        let any_count = param_count;
        let any_types = (0..any_count).map(|_| quote! { &dyn std::any::Any });
        quote! {
            fn #method_name(&self) -> Option<(#(#any_types),*)> {
                Some((#(#field_refs),*))
            }
        }
    };

    if param_count == 0 {
        // Unit-like variant with PhantomData
        quote! {
            #vis struct #variant_name #struct_generics {
                _phantom: std::marker::PhantomData<(#(#all_type_params),*)>
            }

            impl #impl_generics #variant_name #struct_generics {
                #vis fn new() -> Self {
                    #variant_name { _phantom: std::marker::PhantomData }
                }
            }

            impl #impl_generics #trait_name #trait_ty_generics for #variant_name #struct_generics {
                #accessor_impl
            }
        }
    } else {
        // Use tuple struct with PhantomData
        let field_names: Vec<_> = (0..param_count)
            .map(|i| Ident::new(&format!("f{}", i), proc_macro2::Span::call_site()))
            .collect();
        let field_names_2 = field_names.clone();

        quote! {
            #vis struct #variant_name #struct_generics (
                #(#vis #param_types,)*
                #[doc(hidden)]
                core::marker::PhantomData<(#(#all_type_params),*)>
            );

            impl #impl_generics #variant_name #struct_generics {
                #vis fn new(#(#field_names: #param_types),*) -> Self {
                    #variant_name(#(#field_names_2,)* core::marker::PhantomData)
                }
            }

            impl #impl_generics #trait_name #trait_ty_generics for #variant_name #struct_generics {
                #accessor_impl
            }
        }
    }
}

fn combine_generics(enum_generics: &syn::Generics, variant_generics: &[Ident]) -> Vec<Ident> {
    let mut result: Vec<Ident> = enum_generics
        .type_params()
        .map(|tp| tp.ident.clone())
        .collect();

    result.extend(variant_generics.iter().cloned());
    result
}
