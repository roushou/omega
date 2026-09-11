use super::named_fields;
use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

pub(super) struct InputExpansion;
impl InputExpansion {
    pub(super) fn expand(tokens: TokenStream, form: bool) -> TokenStream {
        let input = match syn::parse::<DeriveInput>(tokens) {
            Ok(input) => input,
            Err(error) => return error.to_compile_error().into(),
        };
        match Self::generate(&input, form) {
            Ok(output) => output.into(),
            Err(error) => error.to_compile_error().into(),
        }
    }
    fn generate(input: &DeriveInput, form: bool) -> syn::Result<proc_macro2::TokenStream> {
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &input.generics,
                "an input declaration must be a concrete type",
            ));
        }
        let name = &input.ident;
        let fields = named_fields(input)?;
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        let mut controls = Vec::new();
        for field in &fields {
            let ident = field.ident.as_ref().expect("named field");
            let key = ident.to_string().trim_start_matches("r#").to_string();
            let ty = &field.ty;
            reads.push(quote! { #ident: values.get::<#ty>(#key).ok_or_else(|| ::omega::internal::Error::invalid(concat!("missing or invalid field: ", #key)))? });
            writes.push(quote! { values.set(#key, self.#ident); });
            if form {
                let mut label = key.replace('_', " ");
                let mut placeholder = String::new();
                let mut help = String::new();
                let mut secret = false;
                for attr in &field.attrs {
                    if attr.path().is_ident("omega") {
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("secret") {
                                secret = true;
                                return Ok(());
                            }
                            let value = meta.value()?.parse::<syn::LitStr>()?.value();
                            if meta.path.is_ident("label") {
                                label = value;
                            } else if meta.path.is_ident("placeholder") {
                                placeholder = value;
                            } else if meta.path.is_ident("help") {
                                help = value;
                            } else {
                                return Err(
                                    meta.error("expected label, placeholder, help or secret")
                                );
                            }
                            Ok(())
                        })?;
                    }
                }
                let secret = if secret { quote!(.secret()) } else { quote!() };
                controls.push(quote! {
                    {
                        // Text controls submit strings; aliases of String are supported.
                        let _: fn(String) -> #ty = |value| value;
                        (#key, ::omega::internal::Field::new(#label).placeholder(#placeholder).help(#help)#secret)
                    }
                });
            }
        }
        let count = fields.len();
        let form_impl = if form {
            quote! {
                impl ::omega::internal::FormInput for #name {
                    fn fields() -> Vec<(&'static str, ::omega::internal::Field)> { vec![#(#controls),*] }
                }
            }
        } else {
            quote!()
        };
        Ok(quote! {
            impl ::omega::internal::Input for #name {
                fn decode(args: ::omega::internal::Args) -> Result<Self, ::omega::internal::Error> {
                    if args.len() != 1 { return Err(::omega::internal::Error::invalid("expected one input map")); }
                    let values = args.get::<::omega::internal::Values>(0)
                        .ok_or_else(|| ::omega::internal::Error::invalid("expected an input map"))?;
                    let input = Self { #(#reads),* };
                    if values.as_map().len() != #count {
                        return Err(::omega::internal::Error::invalid("unexpected input fields"));
                    }
                    Ok(input)
                }
                fn encode(self) -> Vec<::omega::internal::Value> {
                    let mut values = ::omega::internal::Values::new();
                    #(#writes)*
                    vec![::omega::internal::IntoValue::into_value(values)]
                }
            }
            #form_impl
        })
    }
}
