//! Procedural derive macro for `LayeredMerge`.
//!
//! Generates a `LayeredMerge::merge(self, other) -> Self` impl that
//! walks each field according to its `#[layer(merge = "...")]` annotation.
//! Supported strategies: `replace` (default), `append`, `deep_merge`,
//! `culture_final`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Meta};

/// `#[derive(Layered)]` — generates `LayeredMerge` impl for the struct.
#[proc_macro_derive(Layered, attributes(layer))]
pub fn derive_layered(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let Data::Struct(data) = &ast.data else {
        return syn::Error::new_spanned(&ast, "Layered only supports structs")
            .to_compile_error()
            .into();
    };
    let Fields::Named(fields) = &data.fields else {
        return syn::Error::new_spanned(&ast, "Layered requires named fields")
            .to_compile_error()
            .into();
    };

    let merges: Vec<TokenStream2> = fields
        .named
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let strategy = extract_strategy(f).unwrap_or_else(|| "replace".to_string());
            match strategy.as_str() {
                "append" => quote! {
                    #ident: {
                        let mut v = self.#ident;
                        v.extend(other.#ident);
                        v
                    }
                },
                "deep_merge" => quote! {
                    #ident: ::sidequest_genre::resolver::LayeredMerge::merge(self.#ident, other.#ident)
                },
                _ => quote! {
                    #ident: other.#ident
                },
            }
        })
        .collect();

    let expanded = quote! {
        impl ::sidequest_genre::resolver::LayeredMerge for #name {
            fn merge(self, other: Self) -> Self {
                Self {
                    #( #merges ),*
                }
            }
        }
    };
    expanded.into()
}

fn extract_strategy(f: &syn::Field) -> Option<String> {
    for attr in &f.attrs {
        if !attr.path().is_ident("layer") {
            continue;
        }
        if let Meta::List(list) = &attr.meta {
            let mut found = None;
            let _ = list.parse_nested_meta(|meta| {
                if meta.path.is_ident("merge") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    found = Some(value.value());
                }
                Ok(())
            });
            if found.is_some() {
                return found;
            }
        }
    }
    None
}
