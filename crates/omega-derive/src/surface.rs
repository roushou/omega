use super::{Marker, kebab, wire};
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub(super) struct SurfaceExpansion;
impl SurfaceExpansion {
    pub(super) fn expand(tokens: TokenStream) -> TokenStream {
        let input = match syn::parse::<DeriveInput>(tokens.clone()) {
            Ok(input) => input,
            Err(error) => return error.to_compile_error().into(),
        };
        if !input.generics.params.is_empty() {
            return syn::Error::new_spanned(
                &input.generics,
                "a widget declaration must be a concrete type",
            )
            .to_compile_error()
            .into();
        }
        let name = &input.ident;
        let visibility = &input.vis;
        let mut surface = kebab(&name.to_string());
        for attribute in &input.attrs {
            if attribute.path().is_ident("omega")
                && let Err(error) = attribute.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        surface = meta.value()?.parse::<syn::LitStr>()?.value();
                        Ok(())
                    } else {
                        Err(meta.error("expected `name = \"surface-name\"`"))
                    }
                })
            {
                return error.to_compile_error().into();
            }
        }
        let reference = match &input.data {
            Data::Struct(data) if matches!(data.fields, Fields::Unit) => quote! {
                impl From<#name> for ::omega::internal::SurfaceRef<#name> {
                    fn from(_: #name) -> Self { Self::INSTANCE }
                }
            },
            _ => quote! {
                #[allow(non_upper_case_globals, dead_code)]
                #visibility const #name: ::omega::internal::SurfaceRef<#name> = ::omega::internal::SurfaceRef::INSTANCE;
            },
        };
        let mut expanded = proc_macro2::TokenStream::from(wire(tokens, Marker::Reads));
        expanded.extend(quote! {
            impl ::omega::internal::SurfaceIdentity for #name {
                const PLUGIN: &'static str = env!("CARGO_PKG_NAME");
                const SURFACE: &'static str = #surface;
            }
            #reference
        });
        expanded.into()
    }
}
