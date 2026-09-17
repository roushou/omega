//! Derive dependency declarations, typed identities, and construction from plugin fields.

mod command;
mod input;
mod surface;
use command::CommandExpansion;
use input::InputExpansion;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Ident, Type, parse_macro_input};

/// A widget: draws, and may hold only state.
#[proc_macro_derive(Surface, attributes(omega))]
pub fn surface(input: TokenStream) -> TokenStream {
    surface::SurfaceExpansion::expand(input)
}

/// A command: does something on request, and may hold anything.
#[proc_macro_derive(Command, attributes(omega))]
pub fn command(input: TokenStream) -> TokenStream {
    CommandExpansion::expand(input)
}

/// A reaction: runs when something happened, and may hold anything.
#[proc_macro_derive(Reaction, attributes(omega))]
pub fn reaction(input: TokenStream) -> TokenStream {
    wire(input, Marker::Wiring)
}

/// Derive bidirectional settings serialization.
/// Requires `Default`; missing fields use the type's declared defaults.
#[proc_macro_derive(Config, attributes(omega))]
pub fn config(input: TokenStream) -> TokenStream {
    fields_impl(input)
}

/// A struct that is a map of values on the wire.
fn fields_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match named_fields(&input) {
        Ok(fields) => fields,
        Err(error) => return error.to_compile_error().into(),
    };

    let named: Vec<(Ident, String, Type)> = fields
        .iter()
        .map(|field| {
            let ident = field.ident.clone().expect("named fields have names");
            // `low_threshold` in Rust is `low-threshold` in a document: each
            // language spells a compound name the way it spells every other.
            let key = ident.to_string().replace('_', "-");
            (ident, key, field.ty.clone())
        })
        .collect();

    // Reading is total: a field the map does not carry takes its default, so
    // adding one never breaks a writer that predates it.
    let reads = named.iter().map(|(ident, key, _)| {
        quote! {
            #ident: values.get(#key).unwrap_or(defaults.#ident)
        }
    });

    let writes = named.iter().map(|(ident, key, _)| {
        quote! {
            values.set(#key, ::core::clone::Clone::clone(&self.#ident));
        }
    });

    quote! {
        impl ::omega::internal::Fields for #name {
            fn read(values: &::omega::internal::Values) -> Self {
                // Missing fields use their declared type's default.
                let defaults = <Self as ::core::default::Default>::default();
                Self { #(#reads,)* }
            }

            fn write(&self) -> ::omega::internal::Values {
                let mut values = ::omega::internal::Values::new();
                #(#writes)*
                values
            }
        }

        // Support nested maps and lists by implementing value conversion.
        impl ::omega::internal::IntoValue for #name {
            fn into_value(self) -> ::omega::internal::Value {
                ::omega::internal::IntoValue::into_value(
                    <Self as ::omega::internal::Fields>::write(&self),
                )
            }
        }

        impl ::omega::internal::FromValue for #name {
            fn from_value(value: &::omega::internal::Value) -> ::core::option::Option<Self> {
                let values: ::omega::internal::Values =
                    ::omega::internal::FromValue::from_value(value)?;
                ::core::option::Option::Some(<Self as ::omega::internal::Fields>::read(&values))
            }
        }
    }
    .into()
}

/// Derive record ownership from the defining package and key from the type name.
#[proc_macro_derive(PluginState, attributes(omega))]
pub fn plugin_state(input: TokenStream) -> TokenStream {
    let fields = fields_impl(input.clone());
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let key = kebab(&name.to_string());

    let identity = quote! {
        impl ::omega::internal::PluginState for #name {
            const PLUGIN: &'static str = env!("CARGO_PKG_NAME");
            const KEY: &'static str = #key;
        }
    };

    let mut expanded = proc_macro2::TokenStream::from(fields);
    expanded.extend(identity);
    expanded.into()
}

/// Convert type names to kebab-case record keys.
fn kebab(name: &str) -> String {
    let mut kebab = String::with_capacity(name.len() + 4);
    for (index, character) in name.char_indices() {
        if character.is_uppercase() {
            if index > 0 {
                kebab.push('-');
            }
            kebab.extend(character.to_lowercase());
        } else {
            kebab.push(character);
        }
    }
    kebab
}

/// Which marker the fields of this kind of plugin must satisfy.
#[derive(Clone, Copy)]
enum Marker {
    /// Anything a plugin can hold.
    Wiring,
    /// Read-only dependencies for render declarations.
    Reads,
}

impl Marker {
    fn path(self) -> proc_macro2::TokenStream {
        match self {
            Self::Wiring => quote!(::omega::internal::Wiring),
            Self::Reads => quote!(::omega::internal::Reads),
        }
    }
}

struct Field {
    ident: Ident,
    ty: Type,
    /// Settings supplied to the instance at construction.
    is_config: bool,
}

fn wire(input: TokenStream, marker: Marker) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match wired_fields(&input) {
        Ok(fields) => fields,
        Err(error) => return error.to_compile_error().into(),
    };

    let handles: Vec<&Field> = fields.iter().filter(|field| !field.is_config).collect();
    let bound = marker.path();

    // Emit dependency-bound errors at the author's field type.
    let assertions = handles.iter().enumerate().map(|(index, field)| {
        let assert = format_ident!("_omega_assert_{index}");
        let ty = &field.ty;
        quote! {
            fn #assert<T: #bound>() {}
            #assert::<#ty>();
        }
    });

    let topics = handles.iter().map(|field| {
        let ty = &field.ty;
        quote! {
            topics.extend_from_slice(<#ty as ::omega::internal::Wiring>::TOPICS);
        }
    });

    let required = handles.iter().map(|field| {
        let ty = &field.ty;
        quote! { topics.extend(<#ty as ::omega::internal::Wiring>::required_topics()); }
    });

    let capabilities = handles.iter().map(|field| {
        let ty = &field.ty;
        quote! {
            capabilities.extend_from_slice(<#ty as ::omega::internal::Wiring>::CAPABILITIES);
        }
    });

    let keyspaces = handles.iter().map(|field| {
        let ty = &field.ty;
        quote! {
            keyspaces.extend(<#ty as ::omega::internal::Wiring>::keyspaces());
        }
    });

    let build = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        if field.is_config {
            quote! { #ident: <#ty as ::omega::internal::Fields>::read(settings) }
        } else {
            quote! {
                #ident: <#ty as ::omega::internal::Wiring>::build(context)
            }
        }
    });

    quote! {
        impl ::omega::internal::Wired for #name {
            fn topics() -> ::std::vec::Vec<::omega::internal::SystemTopic> {
                let mut topics = ::std::vec::Vec::new();
                #(#topics)*
                topics
            }

            fn required_topics() -> ::std::vec::Vec<::omega::internal::SystemTopic> { let mut topics = ::std::vec::Vec::new(); #(#required)* topics }

            fn capabilities() -> ::std::vec::Vec<::omega::internal::Capability> {
                let mut capabilities = ::std::vec::Vec::new();
                #(#capabilities)*
                capabilities
            }

            fn keyspaces() -> ::std::vec::Vec<::std::string::String> {
                let mut keyspaces = ::std::vec::Vec::new();
                #(#keyspaces)*
                keyspaces
            }

            fn build(
                context: &::omega::internal::Context,
                settings: &::omega::internal::Values,
            ) -> Self {
                const _: fn() = || { #(#assertions)* };
                let _ = settings;
                let _ = context;
                Self { #(#build,)* }
            }
        }
    }
    .into()
}

fn wired_fields(input: &DeriveInput) -> syn::Result<Vec<Field>> {
    named_fields(input)?
        .into_iter()
        .map(|field| {
            Ok(Field {
                is_config: is_config(&field)?,
                ident: field.ident.clone().expect("named fields have names"),
                ty: field.ty.clone(),
            })
        })
        .collect()
}

fn named_fields(input: &DeriveInput) -> syn::Result<Vec<syn::Field>> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "a plugin is a struct of what it needs; an enum has no fields to declare from",
        ));
    };

    match &data.fields {
        Fields::Named(named) => Ok(named.named.iter().cloned().collect()),
        // Unit structs have no dependency fields.
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(unnamed) => Err(syn::Error::new_spanned(
            unnamed,
            "a plugin's fields are named: the name is how the settings and the code agree",
        )),
    }
}

/// Whether the field holds instance settings.
fn is_config(field: &syn::Field) -> syn::Result<bool> {
    let mut config = false;
    for attribute in &field.attrs {
        if !attribute.path().is_ident("omega") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("config") {
                config = true;
                Ok(())
            } else {
                Err(meta.error("unknown omega attribute; the only one is `config`"))
            }
        })?;
    }
    Ok(config)
}

/// A strict command input map. Missing, mistyped and unknown fields are refused.
#[proc_macro_derive(Input)]
pub fn input(tokens: TokenStream) -> TokenStream {
    InputExpansion::expand(tokens, false)
}

/// A typed text form. Field attributes are `label`, `placeholder`, `help` and `secret`.
#[proc_macro_derive(Form, attributes(omega))]
pub fn form(tokens: TokenStream) -> TokenStream {
    InputExpansion::expand(tokens, true)
}

/// Effect handles supplied exclusively to stateful surface behavior.
#[proc_macro_derive(Effects, attributes(omega))]
pub fn effects(input: TokenStream) -> TokenStream {
    wire(input, Marker::Wiring)
}
