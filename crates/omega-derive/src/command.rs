use super::{Marker, kebab, wire};
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub(super) struct CommandExpansion;
impl CommandExpansion {
    pub(super) fn expand(tokens: TokenStream) -> TokenStream {
        let input = match syn::parse::<DeriveInput>(tokens.clone()) {
            Ok(input) => input,
            Err(error) => return error.to_compile_error().into(),
        };
        if !input.generics.params.is_empty() {
            return syn::Error::new_spanned(
                &input.generics,
                "a command declaration must be a concrete type",
            )
            .to_compile_error()
            .into();
        }
        let name = &input.ident;
        let visibility = &input.vis;
        let mut command_name = kebab(&name.to_string());
        for attr in &input.attrs {
            if attr.path().is_ident("omega")
                && let Err(error) = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        command_name = meta.value()?.parse::<syn::LitStr>()?.value();
                        Ok(())
                    } else {
                        Err(meta.error("expected `name = \"command-name\"`"))
                    }
                })
            {
                return error.to_compile_error().into();
            }
        }
        let reference = match &input.data {
            Data::Struct(data) if matches!(data.fields, Fields::Unit) => quote! {
                impl From<#name> for ::omega::internal::CommandRef<#name> {
                    fn from(_: #name) -> Self { Self::INSTANCE }
                }
                impl #name {
                    /// Bind this command's input for a button.
                    pub fn with(self, input: <Self as ::omega::internal::Command>::Input) -> ::omega::internal::Bind<()> {
                        ::omega::internal::CommandRef::<Self>::INSTANCE.with(input)
                    }
                }
                impl From<#name> for ::omega::internal::Bind<<#name as ::omega::internal::Command>::Input> {
                    fn from(_: #name) -> Self { ::omega::internal::CommandRef::<#name>::INSTANCE.into() }
                }
            },
            _ => quote! {
                #[allow(non_upper_case_globals, dead_code)]
                #visibility const #name: ::omega::internal::CommandRef<#name> = ::omega::internal::CommandRef::INSTANCE;
            },
        };
        let mut wired = proc_macro2::TokenStream::from(wire(tokens, Marker::Wiring));
        wired.extend(quote! {
            impl ::omega::internal::CommandName for #name { const NAME: &'static str = #command_name; }
            #reference
        });
        wired.into()
    }
}
