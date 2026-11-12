//! Validated model → Rust source (quote + prettyplease, the formatter prost-build uses).

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

use super::Settings;
use super::model::{
    Command, Dec, Def, Domain, Event, Field, InlineEnum, Int, Meta, Scope, Ty, TypeBody, TypeDef,
    Variant,
};

/// Helpers for values carried as JSON strings, emitted once at the root.
const WIRE: &str = r#"
/// Values carried as JSON strings, used through `#[serde(with = ...)]` on generated fields:
/// 64-bit integers as canonical decimal strings (`0`, or an optional `-` for signed types and
/// a non-zero first digit; no `+`, leading zeros, `-0`, whitespace, fraction or exponent;
/// out-of-range values and JSON numbers are rejected), and `binary` as base64 (RFC 4648
/// section 4, standard alphabet, with canonical padding).
pub mod wire {
    /// A value carried as a JSON string.
    pub trait StringCarried: Sized {
        /// What the string must be, for error messages.
        const EXPECTING: &'static str;
        /// The wire string.
        fn to_wire(&self) -> ::std::string::String;
        /// Parses the wire string; `None` unless it is exactly the canonical form.
        fn from_wire(s: &str) -> ::core::option::Option<Self>;
    }

    fn canonical_decimal(s: &str, signed: bool) -> bool {
        let digits = match s.strip_prefix('-') {
            ::core::option::Option::Some(d) if signed => d,
            ::core::option::Option::Some(_) => return false,
            ::core::option::Option::None => s,
        };
        match digits.as_bytes() {
            [b'0'] => digits.len() == s.len(),
            [b'1'..=b'9', rest @ ..] => rest.iter().all(u8::is_ascii_digit),
            _ => false,
        }
    }

    impl StringCarried for i64 {
        const EXPECTING: &'static str = "a signed 64-bit integer as a canonical decimal string";
        fn to_wire(&self) -> ::std::string::String {
            ::std::string::ToString::to_string(self)
        }
        fn from_wire(s: &str) -> ::core::option::Option<Self> {
            if canonical_decimal(s, true) { s.parse().ok() } else { ::core::option::Option::None }
        }
    }

    impl StringCarried for u64 {
        const EXPECTING: &'static str = "an unsigned 64-bit integer as a canonical decimal string";
        fn to_wire(&self) -> ::std::string::String {
            ::std::string::ToString::to_string(self)
        }
        fn from_wire(s: &str) -> ::core::option::Option<Self> {
            if canonical_decimal(s, false) { s.parse().ok() } else { ::core::option::Option::None }
        }
    }

    impl StringCarried for ::std::vec::Vec<u8> {
        const EXPECTING: &'static str = "base64 (RFC 4648 section 4, with padding)";
        fn to_wire(&self) -> ::std::string::String {
            ::base64::Engine::encode(&::base64::engine::general_purpose::STANDARD, self)
        }
        fn from_wire(s: &str) -> ::core::option::Option<Self> {
            ::base64::Engine::decode(&::base64::engine::general_purpose::STANDARD, s).ok()
        }
    }

    struct Ser<'a, T>(&'a T);

    impl<T: StringCarried> ::serde::Serialize for Ser<'_, T> {
        fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            s.serialize_str(&self.0.to_wire())
        }
    }

    struct Seq<'a, T>(&'a [T]);

    impl<T: StringCarried> ::serde::Serialize for Seq<'_, T> {
        fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            s.collect_seq(self.0.iter().map(Ser))
        }
    }

    struct De<T>(T);

    impl<'de, T: StringCarried> ::serde::Deserialize<'de> for De<T> {
        fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<Self, D::Error> {
            struct V<T>(::core::marker::PhantomData<T>);
            impl<T: StringCarried> ::serde::de::Visitor<'_> for V<T> {
                type Value = De<T>;
                fn expecting(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    f.write_str(T::EXPECTING)
                }
                fn visit_str<E: ::serde::de::Error>(self, v: &str) -> ::core::result::Result<De<T>, E> {
                    T::from_wire(v)
                        .map(De)
                        .ok_or_else(|| E::invalid_value(::serde::de::Unexpected::Str(v), &self))
                }
            }
            d.deserialize_str(V(::core::marker::PhantomData))
        }
    }

    /// A field of type `T`.
    pub mod one {
        use super::{De, Ser, StringCarried};

        /// Serializes the wire string.
        pub fn serialize<T: StringCarried, S: ::serde::Serializer>(v: &T, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            ::serde::Serialize::serialize(&Ser(v), s)
        }

        /// Deserializes the wire string.
        pub fn deserialize<'de, T: StringCarried, D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<T, D::Error> {
            <De<T> as ::serde::Deserialize>::deserialize(d).map(|w| w.0)
        }
    }

    /// A field of type `Option<T>`.
    pub mod option {
        use super::{De, Ser, StringCarried};

        /// Serializes the wire string, or `null`.
        pub fn serialize<T: StringCarried, S: ::serde::Serializer>(v: &::core::option::Option<T>, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            match v {
                ::core::option::Option::Some(x) => s.serialize_some(&Ser(x)),
                ::core::option::Option::None => s.serialize_none(),
            }
        }

        /// Deserializes the wire string, or `null`.
        pub fn deserialize<'de, T: StringCarried, D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<::core::option::Option<T>, D::Error> {
            <::core::option::Option<De<T>> as ::serde::Deserialize>::deserialize(d).map(|o| o.map(|w| w.0))
        }
    }

    /// A field of type `Nullable<T>`.
    pub mod nullable {
        use super::{De, Ser, StringCarried};
        use super::super::Nullable;

        /// Serializes the wire string, or `null`.
        pub fn serialize<T: StringCarried, S: ::serde::Serializer>(v: &Nullable<T>, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            match v {
                Nullable::Value(x) => s.serialize_some(&Ser(x)),
                Nullable::Absent | Nullable::Null => s.serialize_none(),
            }
        }

        /// Deserializes the wire string, or `null`.
        pub fn deserialize<'de, T: StringCarried, D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<Nullable<T>, D::Error> {
            <::core::option::Option<De<T>> as ::serde::Deserialize>::deserialize(d)
                .map(|o| o.map_or(Nullable::Null, |w| Nullable::Value(w.0)))
        }
    }

    /// A field of type `Nullable<Vec<T>>`.
    pub mod nullable_vec {
        use super::{De, Seq, StringCarried};
        use super::super::Nullable;

        /// Serializes an array of wire strings, or `null`.
        pub fn serialize<T: StringCarried, S: ::serde::Serializer>(v: &Nullable<::std::vec::Vec<T>>, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            match v {
                Nullable::Value(x) => s.serialize_some(&Seq(x)),
                Nullable::Absent | Nullable::Null => s.serialize_none(),
            }
        }

        /// Deserializes an array of wire strings, or `null`.
        pub fn deserialize<'de, T: StringCarried, D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<Nullable<::std::vec::Vec<T>>, D::Error> {
            <::core::option::Option<::std::vec::Vec<De<T>>> as ::serde::Deserialize>::deserialize(d)
                .map(|o| o.map_or(Nullable::Null, |v| Nullable::Value(v.into_iter().map(|w| w.0).collect())))
        }
    }

    /// A field of type `Vec<T>`.
    pub mod vec {
        use super::{De, Seq, StringCarried};

        /// Serializes an array of wire strings.
        pub fn serialize<T: StringCarried, S: ::serde::Serializer>(v: &[T], s: S) -> ::core::result::Result<S::Ok, S::Error> {
            ::serde::Serialize::serialize(&Seq(v), s)
        }

        /// Deserializes an array of wire strings.
        pub fn deserialize<'de, T: StringCarried, D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<::std::vec::Vec<T>, D::Error> {
            <::std::vec::Vec<De<T>> as ::serde::Deserialize>::deserialize(d).map(|v| v.into_iter().map(|w| w.0).collect())
        }
    }

    /// A field of type `Option<Vec<T>>`.
    pub mod option_vec {
        use super::{De, Seq, StringCarried};

        /// Serializes an array of wire strings, or `null`.
        pub fn serialize<T: StringCarried, S: ::serde::Serializer>(v: &::core::option::Option<::std::vec::Vec<T>>, s: S) -> ::core::result::Result<S::Ok, S::Error> {
            match v {
                ::core::option::Option::Some(x) => s.serialize_some(&Seq(x)),
                ::core::option::Option::None => s.serialize_none(),
            }
        }

        /// Deserializes an array of wire strings, or `null`.
        pub fn deserialize<'de, T: StringCarried, D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<::core::option::Option<::std::vec::Vec<T>>, D::Error> {
            <::core::option::Option<::std::vec::Vec<De<T>>> as ::serde::Deserialize>::deserialize(d)
                .map(|o| o.map(|v| v.into_iter().map(|w| w.0).collect()))
        }
    }
}
"#;

fn doc_lines(lines: &[String]) -> TokenStream {
    let attrs = lines.iter().map(|l| {
        let text = if l.is_empty() {
            String::new()
        } else {
            format!(" {l}")
        };
        quote!(#[doc = #text])
    });
    quote!(#(#attrs)*)
}

/// `head` (if any), a blank line, then the description and the experimental/deprecated notes.
fn docs(head: Option<String>, meta: &Meta) -> TokenStream {
    let mut lines = Vec::new();
    if let Some(h) = head {
        lines.push(h);
    }
    let mut body = Vec::new();
    if let Some(d) = &meta.description {
        body.extend(d.lines().map(|l| l.trim_end().to_string()));
    }
    if meta.experimental {
        body.push("Experimental.".into());
    }
    if meta.deprecated {
        body.push("Deprecated.".into());
    }
    if !lines.is_empty() && !body.is_empty() {
        lines.push(String::new());
    }
    lines.extend(body);
    doc_lines(&lines)
}

fn ident(name: &str) -> proc_macro2::Ident {
    format_ident!("{}", name)
}

fn field_ident(f: &Field) -> proc_macro2::Ident {
    if f.raw {
        proc_macro2::Ident::new_raw(&f.ident, Span::call_site())
    } else {
        ident(&f.ident)
    }
}

/// Values the `wire` helpers carry as strings.
fn string_carried(ty: &Ty) -> bool {
    matches!(ty, Ty::Decimal(_) | Ty::Binary)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Presence {
    Required,
    Optional,
    Nullable,
}

/// The `wire` helper for a field, if it needs one.
fn helper(ty: &Ty, presence: Presence) -> Option<&'static str> {
    let array = matches!(ty, Ty::Array(inner) if string_carried(inner));
    match (string_carried(ty), array, presence) {
        (true, _, Presence::Required) => Some("super::wire::one"),
        (true, _, Presence::Optional) => Some("super::wire::option"),
        (true, _, Presence::Nullable) => Some("super::wire::nullable"),
        (_, true, Presence::Required) => Some("super::wire::vec"),
        (_, true, Presence::Optional) => Some("super::wire::option_vec"),
        (_, true, Presence::Nullable) => Some("super::wire::nullable_vec"),
        _ => None,
    }
}

fn rust_ty(ty: &Ty) -> TokenStream {
    match ty {
        Ty::String => quote!(::std::string::String),
        Ty::Enum(name) => {
            let n = ident(name);
            quote!(#n)
        }
        Ty::Int(Int::I32) => quote!(i32),
        Ty::Int(Int::U32) => quote!(u32),
        Ty::Int(Int::I64) | Ty::Decimal(Dec::I64) => quote!(i64),
        Ty::Decimal(Dec::U64) => quote!(u64),
        Ty::Number => quote!(f64),
        Ty::Boolean => quote!(bool),
        Ty::Binary => quote!(::std::vec::Vec<u8>),
        Ty::Object => quote!(::serde_json::Map<::std::string::String, ::serde_json::Value>),
        Ty::Array(inner) => {
            let i = rust_ty(inner);
            quote!(::std::vec::Vec<#i>)
        }
        Ty::Ref {
            module,
            name,
            boxed,
            ..
        } => {
            let n = ident(name);
            let path = match module {
                Some(m) => {
                    let m = ident(m);
                    quote!(super::#m::#n)
                }
                None => quote!(#n),
            };
            if *boxed {
                quote!(::std::boxed::Box<#path>)
            } else {
                path
            }
        }
    }
}

fn field(f: &Field, public: bool) -> TokenStream {
    let doc = docs(None, &f.meta);
    let name = field_ident(f);
    let inner = rust_ty(&f.ty);
    let presence = match (f.optional, f.nullable) {
        (true, true) => Presence::Nullable,
        (true, false) => Presence::Optional,
        _ => Presence::Required,
    };
    let ty = match presence {
        Presence::Required => inner,
        Presence::Optional => quote!(::core::option::Option<#inner>),
        Presence::Nullable => quote!(super::Nullable<#inner>),
    };
    let mut parts = Vec::new();
    if f.rename {
        let wire = &f.wire;
        parts.push(quote!(rename = #wire));
    }
    match presence {
        Presence::Required => {}
        Presence::Optional => parts.push(quote!(
            default,
            skip_serializing_if = "::core::option::Option::is_none"
        )),
        Presence::Nullable => parts.push(quote!(
            default,
            skip_serializing_if = "super::Nullable::is_absent"
        )),
    }
    if let Some(h) = helper(&f.ty, presence) {
        parts.push(quote!(with = #h));
    }
    let serde = if parts.is_empty() {
        quote!()
    } else {
        quote!(#[serde(#(#parts),*)])
    };
    let vis = if public { quote!(pub) } else { quote!() };
    quote! {
        #doc
        #serde
        #vis #name: #ty,
    }
}

fn container_attr(s: &Settings) -> TokenStream {
    if s.deny_unknown_fields {
        quote!(#[serde(rename_all = "camelCase", deny_unknown_fields)])
    } else {
        quote!(#[serde(rename_all = "camelCase")])
    }
}

fn structure(name: &str, doc: TokenStream, fields: &[Field], s: &Settings) -> TokenStream {
    let n = ident(name);
    let attr = container_attr(s);
    let fields = fields.iter().map(|f| field(f, true));
    quote! {
        #doc
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        #attr
        pub struct #n {
            #(#fields)*
        }
    }
}

fn enumeration(name: &str, doc: TokenStream, variants: &[Variant]) -> TokenStream {
    let n = ident(name);
    let vs = variants.iter().map(|v| {
        let i = ident(&v.ident);
        let w = &v.wire;
        quote!(#[serde(rename = #w)] #i,)
    });
    let arms = variants.iter().map(|v| {
        let i = ident(&v.ident);
        let w = &v.wire;
        quote!(Self::#i => #w,)
    });
    quote! {
        #doc
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
        pub enum #n {
            #(#vs)*
        }

        impl #n {
            /// The wire value.
            pub const fn as_str(self) -> &'static str {
                match self {
                    #(#arms)*
                }
            }
        }
    }
}

fn type_def(t: &TypeDef, s: &Settings) -> TokenStream {
    let doc = docs(None, &t.meta);
    match &t.body {
        TypeBody::Struct(fields) => structure(&t.id, doc, fields, s),
        TypeBody::Enum(variants) => enumeration(&t.id, doc, variants),
        TypeBody::Newtype(ty) => {
            let n = ident(&t.id);
            let inner = rust_ty(ty);
            let eq = matches!(
                ty,
                Ty::String | Ty::Enum(_) | Ty::Int(_) | Ty::Decimal(_) | Ty::Boolean | Ty::Binary
            );
            let derive = if eq {
                quote!(#[derive(Debug, Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)])
            } else {
                quote!(#[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)])
            };
            let with = helper(ty, Presence::Required).map(|h| quote!(#[serde(with = #h)]));
            quote! {
                #doc
                #derive
                #[serde(transparent)]
                pub struct #n(#with pub #inner);
            }
        }
        TypeBody::Union { tag, variants } => {
            let n = ident(&t.id);
            let deny = s.deny_unknown_fields.then(|| quote!(, deny_unknown_fields));
            let vs = variants.iter().map(|v| {
                let vdoc = docs(
                    None,
                    &Meta {
                        description: v.description.clone(),
                        ..Meta::default()
                    },
                );
                let i = ident(&v.ident);
                let w = &v.wire;
                let fields = v.fields.iter().map(|f| field(f, false));
                quote! {
                    #vdoc
                    #[serde(rename = #w, rename_all = "camelCase")]
                    #i {
                        #(#fields)*
                    },
                }
            });
            quote! {
                #doc
                #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
                #[serde(tag = #tag #deny)]
                pub enum #n {
                    #(#vs)*
                }
            }
        }
    }
}

fn inline_enum(e: &InlineEnum) -> TokenStream {
    let doc = doc_lines(&[format!("Values of `{}`.", e.owner)]);
    enumeration(&e.name, doc, &e.variants)
}

fn scope(s: Scope) -> TokenStream {
    match s {
        Scope::Connection => quote!(super::Scope::Connection),
        Scope::Session => quote!(super::Scope::Session),
        Scope::Both => quote!(super::Scope::Both),
    }
}

/// The `targetTypes` enum as a path from a domain module.
fn target_type_path((module, id): &(String, String)) -> TokenStream {
    let m = ident(module);
    let t = ident(id);
    quote!(super::#m::#t)
}

fn command(
    domain: &str,
    c: &Command,
    target_type: Option<&(String, String)>,
    s: &Settings,
) -> TokenStream {
    let method = format!("{domain}.{}", c.name);
    let pdoc = docs(Some(format!("Parameters of `{method}`.")), &c.meta);
    let rdoc = doc_lines(&[format!("Result of `{method}`.")]);
    let params = structure(&c.params_name, pdoc, &c.params, s);
    let returns = structure(&c.returns_name, rdoc, &c.returns, s);
    let p = ident(&c.params_name);
    let r = ident(&c.returns_name);
    let sc = scope(c.scope);
    // `target_type` is `Some` whenever a command has `targetTypes` (checked in the model).
    let tt = target_type.map(|tt| {
        let path = target_type_path(tt);
        let value = match &c.target_types {
            Some(vs) => {
                let items = vs.iter().map(|v| {
                    let i = ident(&v.ident);
                    quote!(#path::#i)
                });
                quote!(::core::option::Option::Some(&[#(#items),*]))
            }
            None => quote!(::core::option::Option::None),
        };
        quote!(const TARGET_TYPES: ::core::option::Option<&'static [#path]> = #value;)
    });
    quote! {
        #params
        #returns
        impl super::Command for #p {
            type Returns = #r;
            const METHOD: &'static str = #method;
            const SCOPE: super::Scope = #sc;
            #tt
        }
    }
}

fn event(domain: &str, e: &Event, s: &Settings) -> TokenStream {
    let method = format!("{domain}.{}", e.name);
    let doc = docs(
        Some(format!("Parameters of the event `{method}`.")),
        &e.meta,
    );
    let st = structure(&e.struct_name, doc, &e.params, s);
    let n = ident(&e.struct_name);
    let sc = scope(e.scope);
    quote! {
        #st
        impl super::Event for #n {
            const METHOD: &'static str = #method;
            const SCOPE: super::Scope = #sc;
        }
    }
}

fn module(
    d: &Domain,
    head: String,
    extra: &[String],
    target_type: Option<&(String, String)>,
    s: &Settings,
) -> TokenStream {
    let mut doc_meta = d.meta.clone();
    if !extra.is_empty() {
        let mut desc = doc_meta.description.unwrap_or_default();
        for e in extra {
            if !desc.is_empty() {
                desc.push_str("\n\n");
            }
            desc.push_str(e);
        }
        doc_meta.description = Some(desc);
    }
    let doc = docs(Some(head), &doc_meta);
    let m = ident(&d.module);
    let types = d.types.iter().map(|t| type_def(t, s));
    let commands = d
        .commands
        .iter()
        .map(|c| command(&d.name, c, target_type, s));
    let events = d.events.iter().map(|e| event(&d.name, e, s));
    let enums = d.enums.iter().map(inline_enum);
    quote! {
        #doc
        pub mod #m {
            #(#types)*
            #(#commands)*
            #(#events)*
            #(#enums)*
        }
    }
}

fn errors(def: &Def) -> TokenStream {
    let vs = def.errors.iter().map(|e| {
        let doc = docs(
            None,
            &Meta {
                description: e.description.clone(),
                ..Meta::default()
            },
        );
        let i = ident(&e.variant);
        let w = &e.wire;
        quote!(#doc #[serde(rename = #w)] #i,)
    });
    let arms = def.errors.iter().map(|e| {
        let i = ident(&e.variant);
        let w = &e.wire;
        quote!(Self::#i => #w,)
    });
    quote! {
        /// ADP error codes (protocol.json `errors`). The wire form is the name.
        pub mod errors {
            /// An ADP error code.
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
            pub enum ErrorCode {
                #(#vs)*
            }

            impl ErrorCode {
                /// The wire name.
                pub const fn as_str(self) -> &'static str {
                    match self {
                        #(#arms)*
                    }
                }
            }
        }
    }
}

/// The generated body (without the generated-file header).
pub(crate) fn emit(def: &Def, version: &str, s: &Settings) -> String {
    let wire: TokenStream = WIRE
        .parse()
        .expect("the wire helper template is valid Rust");
    let errors = errors(def);
    let envelope = module(
        &def.envelope,
        "The message envelope (protocol.json `envelope`).".into(),
        &[format!("Binary messages: {}", def.binary)],
        None,
        s,
    );
    let tt = def.target_type.as_ref();
    let domains = def
        .domains
        .iter()
        .map(|d| module(d, format!("The `{}` domain.", d.name), &[], tt, s));
    let trait_tt = tt.map(|(module, id)| {
        let m = ident(module);
        let t = ident(id);
        let doc = format!(
            " The target types whose sessions accept the command (protocol.json `targetTypes`, values of `{module}::{id}`); in the session of another type: INVALID_REQUEST. `None`: not fixed yet, which is not the same as every type. Always `None` for connection-scoped commands."
        );
        quote! {
            #[doc = #doc]
            const TARGET_TYPES: ::core::option::Option<&'static [#m::#t]>;
        }
    });
    let tokens = quote! {
        /// The ADP protocol version (contracts/adp/VERSION).
        pub const PROTOCOL_VERSION: &str = #version;

        /// Where a command or event may be sent (protocol.json `scope`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Scope {
            /// On the connection, without a `sessionId`.
            Connection,
            /// In a session, with a `sessionId`.
            Session,
            /// Either.
            Both,
        }

        /// An `optional` property that is also `nullable` (protocol.json): absent, an explicit
        /// JSON `null` (clears the value), or a value. Serialized: `Absent` is left out of the
        /// object, `Null` is `null`. Deserialized: a missing property is `Absent`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub enum Nullable<T> {
            /// The property is absent: unchanged.
            #[default]
            Absent,
            /// The property is `null`: the value is cleared.
            Null,
            /// The property has a value.
            Value(T),
        }

        impl<T> Nullable<T> {
            /// Whether the property is absent (it is then left out when serializing).
            pub const fn is_absent(&self) -> bool {
                matches!(self, Self::Absent)
            }
        }

        impl<T: ::serde::Serialize> ::serde::Serialize for Nullable<T> {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::core::result::Result<S::Ok, S::Error> {
                match self {
                    Self::Value(v) => s.serialize_some(v),
                    Self::Absent | Self::Null => s.serialize_none(),
                }
            }
        }

        impl<'de, T: ::serde::Deserialize<'de>> ::serde::Deserialize<'de> for Nullable<T> {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::core::result::Result<Self, D::Error> {
                <::core::option::Option<T> as ::serde::Deserialize>::deserialize(d)
                    .map(|o| o.map_or(Self::Null, Self::Value))
            }
        }

        /// A command, implemented by its parameters type.
        pub trait Command: ::serde::Serialize + ::serde::de::DeserializeOwned {
            /// The result type.
            type Returns: ::serde::Serialize + ::serde::de::DeserializeOwned;
            /// `Domain.command`: the Request `method`.
            const METHOD: &'static str;
            /// Where the command may be sent.
            const SCOPE: Scope;
            #trait_tt
        }

        /// An event, implemented by its parameters type.
        pub trait Event: ::serde::Serialize + ::serde::de::DeserializeOwned {
            /// `Domain.event`: the Event `method`.
            const METHOD: &'static str;
            /// Where the event may be sent.
            const SCOPE: Scope;
        }

        #errors
        #wire
        #envelope
        #(#domains)*
    };
    let file: syn::File = syn::parse2(tokens).expect("generated tokens are a valid Rust file");
    prettyplease::unparse(&file)
}
