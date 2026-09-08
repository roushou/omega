//! Where a plugin's manifest comes from.
//!
//! A plugin declares what it needs by holding it. These derives read the
//! fields and produce the two things the runtime wants from a type: what it
//! costs — the topics it reads and the capabilities those and its effects
//! require — and how to build one out of a live connection.
//!
//! Nothing here invents a declaration. Every topic and capability comes from
//! a field's own type, which is why the manifest cannot drift from the code:
//! it is a projection of it.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Ident, Type, parse_macro_input};

/// A widget: draws, and may hold only state.
#[proc_macro_derive(Widget, attributes(omega))]
pub fn widget(input: TokenStream) -> TokenStream {
    wire(input, Marker::Reads)
}

/// A command: does something on request, and may hold anything.
#[proc_macro_derive(Command, attributes(omega))]
pub fn command(input: TokenStream) -> TokenStream {
    wire(input, Marker::Wiring)
}

/// A reaction: runs when something happened, and may hold anything.
#[proc_macro_derive(Reaction, attributes(omega))]
pub fn reaction(input: TokenStream) -> TokenStream {
    wire(input, Marker::Wiring)
}

/// Settings a document configures an instance with.
///
/// Requires `Default`: it is what a setting the document left out falls back
/// to, and having it on the type means the fallback is stated once, next to
/// the field, rather than at every place that reads it.
///
/// Generates both directions. The config plane writes these fields and the
/// plugin reads them, and a boundary where each side spells the names itself
/// is a boundary where they can disagree.
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
        impl ::omega::Fields for #name {
            fn read(values: &::omega::Values) -> Self {
                // What the writer left out is what this type says it is.
                let defaults = <Self as ::core::default::Default>::default();
                Self { #(#reads,)* }
            }

            fn write(&self) -> ::omega::Values {
                let mut values = ::omega::Values::new();
                #(#writes)*
                values
            }
        }

        // A struct that is a map is also a value, so one can be a field of
        // another or an element of a list. Without this a type could be
        // written to a keyspace only at the top level, and `Vec<Self>` would
        // not compile — which is most of what a unit has to publish.
        impl ::omega::internal::IntoValue for #name {
            fn into_value(self) -> ::omega::internal::Value {
                ::omega::internal::IntoValue::into_value(
                    <Self as ::omega::Fields>::write(&self),
                )
            }
        }

        impl ::omega::internal::FromValue for #name {
            fn from_value(value: &::omega::internal::Value) -> ::core::option::Option<Self> {
                let values: ::omega::Values =
                    ::omega::internal::FromValue::from_value(value)?;
                ::core::option::Option::Some(<Self as ::omega::Fields>::read(&values))
            }
        }
    }
    .into()
}

/// State a plugin owns, addressed by where it is defined.
///
/// The unit is the crate this type is written in and the key is the type's
/// own name, so `Watch<lamp::Power>` resolves to `unit.lamp.power` with no
/// string anywhere for a rename to leave behind.
#[proc_macro_derive(Topic, attributes(omega))]
pub fn topic(input: TokenStream) -> TokenStream {
    let fields = fields_impl(input.clone());
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let key = kebab(&name.to_string());

    let identity = quote! {
        impl ::omega::Topic for #name {
            const UNIT: &'static str = env!("CARGO_PKG_NAME");
            const KEY: &'static str = #key;
        }
    };

    let mut expanded = proc_macro2::TokenStream::from(fields);
    expanded.extend(identity);
    expanded.into()
}

/// `LowPower` is `low-power`: a topic key reads as an address, not as a Rust
/// type.
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
    /// Only what it can read — a widget renders, and rendering may not act.
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
    /// The settings the document gave this instance, rather than a handle.
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

    // The assertion that carries the error message. A field the plugin may
    // not hold fails here, naming the type and saying where it belongs,
    // rather than deeper in generated code that mentions none of the
    // author's own names.
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
        match field.is_config {
            true => quote! { #ident: <#ty as ::omega::Fields>::read(settings) },
            false => quote! {
                #ident: <#ty as ::omega::internal::Wiring>::build(context)
            },
        }
    });

    quote! {
        impl ::omega::Wired for #name {
            fn topics() -> ::std::vec::Vec<::omega::internal::SystemTopic> {
                let mut topics = ::std::vec::Vec::new();
                #(#topics)*
                topics
            }

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
        // A unit struct declares nothing, which is a fine thing to be.
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(unnamed) => Err(syn::Error::new_spanned(
            unnamed,
            "a plugin's fields are named: the name is how the settings and the code agree",
        )),
    }
}

/// Whether this field is the instance's settings rather than a handle.
fn is_config(field: &syn::Field) -> syn::Result<bool> {
    let mut config = false;
    for attribute in &field.attrs {
        if !attribute.path().is_ident("omega") {
            continue;
        }
        attribute.parse_nested_meta(|meta| match meta.path.is_ident("config") {
            true => {
                config = true;
                Ok(())
            }
            false => Err(meta.error("unknown omega attribute; the only one is `config`")),
        })?;
    }
    Ok(config)
}
