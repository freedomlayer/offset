#![crate_type = "lib"]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

extern crate proc_macro;

// use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Data, DeriveInput, Fields};
// use syn::spanned::Spanned;
// use syn::Fields;

/// Automatically derive bidirectional From traits (Between current struct and other specified
/// struct).
#[proc_macro_attribute]
pub fn mutual_from(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    // See: https://github.com/dtolnay/syn/issues/86
    // for information about arguments.

    // This is the name of the other struct:
    let remote_name = args.to_string();

    // let item: syn::Item = syn::parse(input).expect("failed to parse input into `syn::Item`");
    let input = parse_macro_input!(input as DeriveInput);

    // Name of local struct:
    let local_name = input.ident.to_string();

    let conversion = match input.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                // Example:
                // struct Point {
                //     x: u32,
                //     y: u32,
                // }
                let recurse1 = fields.named.iter().map(|f| {
                    let fname = &f.ident;
                    quote_spanned! { f.span() =>
                        #fname: input.#fname
                    }
                });
                // TODO: Is there a more elegant way to do this except cloning?
                let recurse2 = recurse1.clone();
                quote! {
                    impl From<#local_name> for #remote_name {
                        fn from(input: #local_name) -> Self {
                            #remote_name {
                                #(#recurse1, )*
                            }
                        }
                    }
                    impl From<#remote_name> for #local_name {
                        fn from(input: #remote_name) -> Self {
                            #local_name {
                                #(#recurse2, )*
                            }
                        }
                    }
                }
            }
            Fields::Unnamed(ref fields) => {
                // Example:
                // struct Pair(i32, f32);

                let recurse1 = fields.unnamed.iter().enumerate().map(|(i, f)| {
                    // TODO: Should we use Index::from(i) here?
                    // What happens if we don't?
                    quote_spanned! { f.span() =>
                        input.#i
                    }
                });
                // TODO: Is there a more elegant way to do this except cloning?
                let recurse2 = recurse1.clone();
                quote! {
                    impl From<#local_name> for #remote_name {
                        fn from(input: #local_name) -> Self {
                            #remote_name(#(#recurse1,)*)
                        }
                    }
                    impl From<#remote_name> for #local_name {
                        fn from(input: #remote_name) -> Self {
                            #local_name(#(#recurse2,)*)
                        }
                    }
                }
            }
            Fields::Unit => {
                // Example:
                // struct MyStruct;
                quote! {
                    impl From<#local_name> for #remote_name {
                        fn from(input: #local_name) -> Self {
                            #remote_name
                        }
                    }
                    impl From<#remote_name> for #local_name {
                        fn from(input: #remote_name) -> Self {
                            #local_name
                        }
                    }
                }
            }
        },
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    };

    let expanded = quote! {
        // Original structure
        #input
        // Generated mutual From conversion code:
        #conversion
    };

    proc_macro::TokenStream::from(expanded)
}
