//! protocol.json → validated model. Phase 1 checks the structure (allowed and required keys,
//! value types) and stops there if anything is wrong, so later checks see a complete
//! definition and report no follow-on problems. Phase 2 checks the meaning (duplicates,
//! `$ref`, shapes, formats, integer widths, Rust names) and builds the model.

use std::collections::{BTreeMap, BTreeSet};

use heck::{ToSnakeCase, ToUpperCamelCase};
use serde_json::{Map, Value};

use super::{IntegerDefault, IntegerWidth, Problem, Settings, problem};

// ---------------------------------------------------------------------------
// model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct Meta {
    pub description: Option<String>,
    pub experimental: bool,
    pub deprecated: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Def {
    pub errors: Vec<ErrorDef>,
    /// `envelope`: its types (no commands or events) as the pseudo-domain `envelope`.
    pub envelope: Domain,
    /// `envelope.binary`: the binary message framing, prose.
    pub binary: String,
    pub domains: Vec<Domain>,
    /// The `targetTypes` enum as `(module, type id)`; `None` when the definition lacks it
    /// (then no command uses `targetTypes` and `Command` has no `TARGET_TYPES`).
    pub target_type: Option<(String, String)>,
}

/// Where a command or event may be sent (`scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Connection,
    Session,
    Both,
}

#[derive(Debug, Clone)]
pub(crate) struct ErrorDef {
    pub wire: String,
    pub variant: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Domain {
    pub name: String,
    pub module: String,
    pub meta: Meta,
    pub types: Vec<TypeDef>,
    /// Inline enums, in definition order.
    pub enums: Vec<InlineEnum>,
    pub commands: Vec<Command>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone)]
pub(crate) struct TypeDef {
    pub id: String,
    pub meta: Meta,
    pub body: TypeBody,
}

#[derive(Debug, Clone)]
pub(crate) enum TypeBody {
    Struct(Vec<Field>),
    Enum(Vec<Variant>),
    Newtype(Ty),
    /// Tagged union: `discriminator` names the tag property.
    Union {
        tag: String,
        variants: Vec<UnionVariant>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct UnionVariant {
    /// The tag value (`kind`).
    pub wire: String,
    pub ident: String,
    pub description: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub(crate) struct InlineEnum {
    pub name: String,
    /// `Type.property` it belongs to, for the doc comment.
    pub owner: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub(crate) struct Command {
    pub name: String,
    pub meta: Meta,
    pub scope: Scope,
    /// `targetTypes` as variants of the target type enum; `None`: not fixed yet.
    pub target_types: Option<Vec<Variant>>,
    pub params_name: String,
    pub returns_name: String,
    pub params: Vec<Field>,
    pub returns: Vec<Field>,
}

#[derive(Debug, Clone)]
pub(crate) struct Event {
    pub name: String,
    pub meta: Meta,
    pub scope: Scope,
    pub struct_name: String,
    pub params: Vec<Field>,
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub wire: String,
    /// Rust field name, without `r#`.
    pub ident: String,
    /// Needs `r#` (a keyword).
    pub raw: bool,
    /// `rename_all = "camelCase"` does not produce `wire`; needs an explicit rename.
    pub rename: bool,
    pub optional: bool,
    /// `nullable` (only with `optional`): absent, `null` and a value are three states.
    pub nullable: bool,
    pub meta: Meta,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub(crate) struct Variant {
    pub wire: String,
    pub ident: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Int {
    I32,
    U32,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dec {
    I64,
    U64,
}

#[derive(Debug, Clone)]
pub(crate) enum Ty {
    String,
    /// An inline enum of the same domain.
    Enum(String),
    Int(Int),
    /// A 64-bit integer carried as a decimal string.
    Decimal(Dec),
    Number,
    Boolean,
    /// `binary`: bytes carried as base64 (RFC 4648 §4, padded).
    Binary,
    /// `object` without `properties`: any JSON object.
    Object,
    Array(Box<Ty>),
    Ref {
        /// Module of another domain; `None` for the same domain.
        module: Option<String>,
        name: String,
        target: (usize, usize),
        boxed: bool,
    },
}

// ---------------------------------------------------------------------------
// phase 1: structure
// ---------------------------------------------------------------------------

const ROOT_KEYS: &[&str] = &["description", "errors", "envelope", "domains"];
const ENVELOPE_KEYS: &[&str] = &["description", "types", "binary"];
const VARIANT_KEYS: &[&str] = &["kind", "description", "properties"];
const ERROR_KEYS: &[&str] = &["name", "description"];
const DOMAIN_KEYS: &[&str] = &[
    "domain",
    "description",
    "experimental",
    "deprecated",
    "types",
    "commands",
    "events",
];
const TYPE_KEYS: &[&str] = &[
    "id",
    "description",
    "experimental",
    "deprecated",
    "type",
    "properties",
    "enum",
    "items",
    "format",
    "discriminator",
    "variants",
];
const PROP_KEYS: &[&str] = &[
    "name",
    "description",
    "optional",
    "nullable",
    "experimental",
    "deprecated",
    "$ref",
    "type",
    "enum",
    "items",
    "format",
];
const ITEM_KEYS: &[&str] = &["$ref", "type", "enum", "items", "format"];
const COMMAND_KEYS: &[&str] = &[
    "name",
    "description",
    "experimental",
    "deprecated",
    "scope",
    "targetTypes",
    "parameters",
    "returns",
];
const EVENT_KEYS: &[&str] = &[
    "name",
    "description",
    "experimental",
    "deprecated",
    "scope",
    "parameters",
];
const TYPES: &[&str] = &[
    "string", "integer", "number", "boolean", "binary", "array", "object",
];
const SCOPES: &[&str] = &["connection", "session", "both"];

#[derive(Debug, Clone, Default)]
struct RShape {
    loc: String,
    r#ref: Option<String>,
    ty: Option<String>,
    enum_: Option<Vec<String>>,
    items: Option<Box<RShape>>,
    format: Option<String>,
    properties: Option<Vec<RProp>>,
}

#[derive(Debug, Clone)]
struct RProp {
    loc: String,
    name: String,
    optional: bool,
    nullable: bool,
    meta: Meta,
    shape: RShape,
}

#[derive(Debug, Clone)]
struct RType {
    loc: String,
    id: String,
    meta: Meta,
    shape: RShape,
    discriminator: Option<String>,
    variants: Option<Vec<RVariant>>,
}

#[derive(Debug, Clone)]
struct RVariant {
    loc: String,
    kind: String,
    description: Option<String>,
    properties: Vec<RProp>,
}

#[derive(Debug, Clone)]
struct RCommand {
    loc: String,
    name: String,
    meta: Meta,
    scope: Scope,
    target_types: Option<Vec<String>>,
    params: Vec<RProp>,
    returns: Vec<RProp>,
}

#[derive(Debug, Clone)]
struct REvent {
    loc: String,
    name: String,
    meta: Meta,
    scope: Scope,
    params: Vec<RProp>,
}

#[derive(Debug, Clone)]
struct REnvelope {
    description: Option<String>,
    types: Vec<RType>,
    binary: String,
}

#[derive(Debug, Clone)]
struct RDomain {
    loc: String,
    name: String,
    meta: Meta,
    types: Vec<RType>,
    commands: Vec<RCommand>,
    events: Vec<REvent>,
}

#[derive(Debug, Clone)]
struct RError {
    loc: String,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Clone)]
struct RDef {
    errors: Vec<RError>,
    envelope: REnvelope,
    domains: Vec<RDomain>,
}

struct P1<'p> {
    ps: &'p mut Vec<Problem>,
}

impl P1<'_> {
    fn object<'v>(
        &mut self,
        v: &'v Value,
        loc: &str,
        allowed: &[&str],
        required: &[&str],
    ) -> Option<&'v Map<String, Value>> {
        let Some(m) = v.as_object() else {
            self.ps
                .push(problem("adp/type", loc, "must be a JSON object"));
            return None;
        };
        for k in m.keys() {
            if !allowed.contains(&k.as_str()) {
                self.ps.push(problem(
                    "adp/unknown-key",
                    loc,
                    format!("unknown key `{k}` (allowed: {})", allowed.join(", ")),
                ));
            }
        }
        let mut ok = true;
        for k in required {
            if !m.contains_key(*k) {
                self.ps.push(problem(
                    "adp/missing-key",
                    loc,
                    format!("missing key `{k}`"),
                ));
                ok = false;
            }
        }
        ok.then_some(m)
    }

    fn string(&mut self, m: &Map<String, Value>, key: &str, loc: &str) -> Option<String> {
        match m.get(key)? {
            Value::String(s) => Some(s.clone()),
            _ => {
                self.ps.push(problem(
                    "adp/type",
                    loc,
                    format!("`{key}` must be a string"),
                ));
                None
            }
        }
    }

    fn boolean(&mut self, m: &Map<String, Value>, key: &str, loc: &str) -> bool {
        match m.get(key) {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                self.ps.push(problem(
                    "adp/type",
                    loc,
                    format!("`{key}` must be true or false"),
                ));
                false
            }
        }
    }

    fn array<'v>(&mut self, m: &'v Map<String, Value>, key: &str, loc: &str) -> &'v [Value] {
        match m.get(key) {
            None => &[],
            Some(Value::Array(a)) => a,
            Some(_) => {
                self.ps.push(problem(
                    "adp/type",
                    loc,
                    format!("`{key}` must be an array"),
                ));
                &[]
            }
        }
    }

    fn meta(&mut self, m: &Map<String, Value>, loc: &str) -> Meta {
        Meta {
            description: self.string(m, "description", loc),
            experimental: self.boolean(m, "experimental", loc),
            deprecated: self.boolean(m, "deprecated", loc),
        }
    }

    fn shape(&mut self, m: &Map<String, Value>, loc: &str) -> RShape {
        let ty = self.string(m, "type", loc);
        if let Some(t) = &ty
            && !TYPES.contains(&t.as_str())
        {
            self.ps.push(problem(
                "adp/type",
                loc,
                format!("`type` {t:?} is not one of {}", TYPES.join(", ")),
            ));
        }
        let enum_ = match m.get("enum") {
            None => None,
            Some(Value::Array(a)) if a.iter().all(Value::is_string) => Some(
                a.iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect(),
            ),
            Some(_) => {
                self.ps.push(problem(
                    "adp/type",
                    loc,
                    "`enum` must be an array of strings",
                ));
                None
            }
        };
        let items = m.get("items").and_then(|v| {
            let iloc = format!("{loc}.items");
            let im = self.object(v, &iloc, ITEM_KEYS, &[])?;
            Some(Box::new(self.shape(im, &iloc)))
        });
        let properties = m.contains_key("properties").then(|| {
            self.array(m, "properties", loc)
                .iter()
                .enumerate()
                .filter_map(|(i, p)| self.prop(p, &format!("{loc}.properties[{i}]")))
                .collect()
        });
        RShape {
            loc: loc.to_string(),
            r#ref: self.string(m, "$ref", loc),
            ty,
            enum_,
            items,
            format: self.string(m, "format", loc),
            properties,
        }
    }

    fn prop(&mut self, v: &Value, loc: &str) -> Option<RProp> {
        let m = self.object(v, loc, PROP_KEYS, &["name"])?;
        let name = self.string(m, "name", loc)?;
        let loc = format!("{loc} ({name})");
        Some(RProp {
            optional: self.boolean(m, "optional", &loc),
            nullable: self.boolean(m, "nullable", &loc),
            meta: self.meta(m, &loc),
            shape: self.shape(m, &loc),
            loc,
            name,
        })
    }

    fn props(&mut self, m: &Map<String, Value>, key: &str, loc: &str) -> Vec<RProp> {
        self.array(m, key, loc)
            .iter()
            .enumerate()
            .filter_map(|(i, p)| self.prop(p, &format!("{loc}.{key}[{i}]")))
            .collect()
    }

    fn root(&mut self, v: &Value) -> Option<RDef> {
        let m = self.object(v, "protocol.json", ROOT_KEYS, &["domains"])?;
        self.string(m, "description", "protocol.json");
        let errors = self
            .array(m, "errors", "protocol.json")
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let loc = format!("errors[{i}]");
                let em = self.object(e, &loc, ERROR_KEYS, &["name"])?;
                Some(RError {
                    name: self.string(em, "name", &loc)?,
                    description: self.string(em, "description", &loc),
                    loc,
                })
            })
            .collect();
        let envelope = m.get("envelope").and_then(|e| self.envelope(e));
        let domains = self
            .array(m, "domains", "protocol.json")
            .iter()
            .enumerate()
            .filter_map(|(i, d)| self.domain(d, &format!("domains[{i}]")))
            .collect();
        if envelope.is_none() && !m.contains_key("envelope") {
            self.ps.push(problem(
                "adp/missing-key",
                "protocol.json",
                "missing key `envelope`",
            ));
        }
        Some(RDef {
            errors,
            envelope: envelope?,
            domains,
        })
    }

    fn envelope(&mut self, v: &Value) -> Option<REnvelope> {
        let loc = "envelope";
        let m = self.object(v, loc, ENVELOPE_KEYS, &["types", "binary"])?;
        let types = self.types(m, loc);
        Some(REnvelope {
            description: self.string(m, "description", loc),
            binary: self.string(m, "binary", loc)?,
            types,
        })
    }

    fn types(&mut self, m: &Map<String, Value>, loc: &str) -> Vec<RType> {
        self.array(m, "types", loc)
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let tloc = format!("{loc}.types[{i}]");
                let tm = self.object(t, &tloc, TYPE_KEYS, &["id", "type"])?;
                let id = self.string(tm, "id", &tloc)?;
                let tloc = format!("{tloc} ({id})");
                let variants = tm.contains_key("variants").then(|| {
                    self.array(tm, "variants", &tloc)
                        .iter()
                        .enumerate()
                        .filter_map(|(j, v)| {
                            let vloc = format!("{tloc}.variants[{j}]");
                            let vm = self.object(v, &vloc, VARIANT_KEYS, &["kind"])?;
                            let kind = self.string(vm, "kind", &vloc)?;
                            let vloc = format!("{vloc} ({kind})");
                            Some(RVariant {
                                description: self.string(vm, "description", &vloc),
                                properties: self.props(vm, "properties", &vloc),
                                loc: vloc,
                                kind,
                            })
                        })
                        .collect()
                });
                Some(RType {
                    meta: self.meta(tm, &tloc),
                    shape: self.shape(tm, &tloc),
                    discriminator: self.string(tm, "discriminator", &tloc),
                    variants,
                    loc: tloc,
                    id,
                })
            })
            .collect()
    }

    /// An optional array of strings.
    fn strings(&mut self, m: &Map<String, Value>, key: &str, loc: &str) -> Option<Vec<String>> {
        match m.get(key)? {
            Value::Array(a) if a.iter().all(Value::is_string) => Some(
                a.iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect(),
            ),
            _ => {
                self.ps.push(problem(
                    "adp/type",
                    loc,
                    format!("`{key}` must be an array of strings"),
                ));
                None
            }
        }
    }

    fn scope(&mut self, m: &Map<String, Value>, loc: &str) -> Scope {
        match self.string(m, "scope", loc).as_deref() {
            Some("connection") => Scope::Connection,
            Some("session") => Scope::Session,
            Some("both") => Scope::Both,
            Some(other) => {
                self.ps.push(problem(
                    "adp/type",
                    loc,
                    format!("`scope` {other:?} is not one of {}", SCOPES.join(", ")),
                ));
                Scope::Both
            }
            // missing: reported as adp/missing-key by `object`; wrong type by `string`
            None => Scope::Both,
        }
    }

    fn domain(&mut self, v: &Value, loc: &str) -> Option<RDomain> {
        let m = self.object(v, loc, DOMAIN_KEYS, &["domain"])?;
        let name = self.string(m, "domain", loc)?;
        let loc = format!("{loc} ({name})");
        let types = self.types(m, &loc);
        let commands = self
            .array(m, "commands", &loc)
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let cloc = format!("{loc}.commands[{i}]");
                // `returns` is explicit, also when empty
                let cm = self.object(c, &cloc, COMMAND_KEYS, &["name", "scope", "returns"])?;
                let name = self.string(cm, "name", &cloc)?;
                let cloc = format!("{cloc} ({name})");
                Some(RCommand {
                    meta: self.meta(cm, &cloc),
                    scope: self.scope(cm, &cloc),
                    target_types: self.strings(cm, "targetTypes", &cloc),
                    params: self.props(cm, "parameters", &cloc),
                    returns: self.props(cm, "returns", &cloc),
                    loc: cloc,
                    name,
                })
            })
            .collect();
        let events = self
            .array(m, "events", &loc)
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let eloc = format!("{loc}.events[{i}]");
                let em = self.object(e, &eloc, EVENT_KEYS, &["name", "scope"])?;
                let name = self.string(em, "name", &eloc)?;
                let eloc = format!("{eloc} ({name})");
                Some(REvent {
                    meta: self.meta(em, &eloc),
                    scope: self.scope(em, &eloc),
                    params: self.props(em, "parameters", &eloc),
                    loc: eloc,
                    name,
                })
            })
            .collect();
        Some(RDomain {
            meta: self.meta(m, &loc),
            loc,
            name,
            types,
            commands,
            events,
        })
    }
}

// ---------------------------------------------------------------------------
// names
// ---------------------------------------------------------------------------

/// Strict and reserved keywords (Rust 2024).
const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// Keywords that cannot be raw identifiers.
const NOT_RAW: &[&str] = &["self", "Self", "super", "crate", "_"];

/// Module names the generator itself uses at the root.
const RESERVED_MODULES: &[&str] = &["errors", "wire", "envelope"];

fn ascii_ident(s: &str) -> bool {
    let mut b = s.bytes();
    matches!(b.next(), Some(c) if c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && s != "_"
}

/// A type or variant name: ASCII identifier, upper-case first letter, not a keyword.
fn type_name_ok(s: &str) -> bool {
    ascii_ident(s) && s.starts_with(|c: char| c.is_ascii_uppercase()) && !KEYWORDS.contains(&s)
}

/// serde `rename_all = "camelCase"` applied to a field name.
fn serde_camel(field: &str) -> String {
    let mut pascal = String::new();
    let mut cap = true;
    for ch in field.chars() {
        if ch == '_' {
            cap = true;
        } else if cap {
            pascal.extend(ch.to_uppercase());
            cap = false;
        } else {
            pascal.push(ch);
        }
    }
    match pascal.chars().next() {
        Some(first) => first.to_lowercase().collect::<String>() + &pascal[first.len_utf8()..],
        None => pascal,
    }
}

// ---------------------------------------------------------------------------
// phase 2: meaning
// ---------------------------------------------------------------------------

struct P2<'a> {
    ps: Vec<Problem>,
    settings: &'a Settings,
    /// Domain name → (index, type id → type index), first occurrences only.
    lookup: BTreeMap<String, (usize, BTreeMap<String, usize>)>,
    modules: Vec<String>,
    /// Setting keys of every `integer` without `format`.
    unfixed: BTreeSet<String>,
}

/// Where a shape sits, for names, setting keys and messages.
struct Ctx<'c> {
    domain: usize,
    /// Struct the field belongs to (`Node`, `GetTreeParams`).
    owner: &'c str,
    /// Wire name of the property.
    prop: &'c str,
    /// `[adp.integer]` key.
    key: &'c str,
    /// Inline enums may be named `<owner><Property>` here (not inside union variants).
    inline_enums: bool,
}

impl P2<'_> {
    fn err(&mut self, rule: &'static str, loc: &str, msg: impl Into<String>) {
        self.ps.push(problem(rule, loc, msg));
    }

    /// Records `name` in a namespace; reports a collision with an earlier entry.
    fn claim(&mut self, names: &mut BTreeMap<String, String>, name: &str, what: String, loc: &str) {
        if let Some(prev) = names.get(name) {
            let msg = format!("Rust name `{name}` of {what} collides with {prev}");
            self.err("adp/name", loc, msg);
        } else {
            names.insert(name.to_string(), what);
        }
    }

    fn variants(&mut self, values: &[String], loc: &str) -> Vec<Variant> {
        let mut seen = BTreeSet::new();
        let mut idents = BTreeMap::new();
        let mut out = Vec::new();
        for v in values {
            if !seen.insert(v.clone()) {
                self.err(
                    "adp/duplicate",
                    loc,
                    format!("enum value {v:?} is listed twice"),
                );
                continue;
            }
            let ident = v.to_upper_camel_case();
            if !type_name_ok(&ident) {
                self.err(
                    "adp/name",
                    loc,
                    format!("enum value {v:?} gives no valid Rust variant name (`{ident}`)"),
                );
                continue;
            }
            self.claim(&mut idents, &ident, format!("enum value {v:?}"), loc);
            out.push(Variant {
                wire: v.clone(),
                ident,
            });
        }
        out
    }

    fn resolve_ref(&mut self, r: &str, c: &Ctx, loc: &str) -> Option<Ty> {
        let (domain, id) = match r.split_once('.') {
            Some((d, id)) => (d.to_string(), id),
            None => (String::new(), r),
        };
        let found = if domain.is_empty() {
            self.lookup
                .values()
                .find(|(i, _)| *i == c.domain)
                .and_then(|(i, types)| types.get(id).map(|t| (*i, *t)))
        } else {
            self.lookup
                .get(&domain)
                .and_then(|(i, types)| types.get(id).map(|t| (*i, *t)))
        };
        let Some((d, t)) = found else {
            self.err(
                "adp/unresolved-ref",
                loc,
                format!("`$ref` {r:?} names no type"),
            );
            return None;
        };
        Some(Ty::Ref {
            module: (d != c.domain).then(|| self.modules[d].clone()),
            name: id.to_string(),
            target: (d, t),
            boxed: false,
        })
    }

    /// A property (or array item) shape → type. Inline enums are added to `enums`.
    fn ty(
        &mut self,
        sh: &RShape,
        c: &Ctx,
        names: &mut BTreeMap<String, String>,
        enums: &mut Vec<InlineEnum>,
    ) -> Option<Ty> {
        let loc = sh.loc.as_str();
        let extra = |sh: &RShape, keys: &[&str]| -> Vec<&'static str> {
            let mut v = Vec::new();
            if keys.contains(&"enum") && sh.enum_.is_some() {
                v.push("enum");
            }
            if keys.contains(&"items") && sh.items.is_some() {
                v.push("items");
            }
            if keys.contains(&"properties") && sh.properties.is_some() {
                v.push("properties");
            }
            v
        };
        let shape_err = |me: &mut Self, what: &str| {
            me.err("adp/shape", loc, what.to_string());
            None
        };
        match (&sh.r#ref, &sh.ty) {
            (Some(_), Some(_)) => shape_err(self, "has both `$ref` and `type`"),
            (None, None) => shape_err(self, "has neither `$ref` nor `type`"),
            (Some(r), None) => {
                let bad = extra(sh, &["enum", "items", "properties"]);
                if !bad.is_empty() || sh.format.is_some() {
                    return shape_err(
                        self,
                        "`$ref` takes no `enum`, `items`, `format` or `properties`",
                    );
                }
                self.resolve_ref(r, c, loc)
            }
            (None, Some(t)) => {
                let t = t.as_str();
                let bad_for = |keys: &[&str]| extra(sh, keys);
                match t {
                    "string" => {
                        let bad = bad_for(&["items", "properties"]);
                        if !bad.is_empty() {
                            return shape_err(
                                self,
                                &format!("`string` takes no `{}`", bad.join("`, `")),
                            );
                        }
                        match (&sh.enum_, sh.format.as_deref()) {
                            (Some(_), Some(_)) => shape_err(self, "an `enum` takes no `format`"),
                            (Some(values), None) if values.is_empty() => {
                                shape_err(self, "an `enum` needs at least one value")
                            }
                            (Some(_), None) if !c.inline_enums => shape_err(
                                self,
                                "an inline enum inside a union variant has no agreed Rust name: give the enum its own type id and use `$ref`",
                            ),
                            (Some(values), None) => {
                                let name = format!("{}{}", c.owner, c.prop.to_upper_camel_case());
                                if !type_name_ok(&name) {
                                    self.err("adp/name", loc, format!("inline enum name `{name}` is not a valid Rust type name"));
                                    return None;
                                }
                                let variants = self.variants(values, loc);
                                self.claim(
                                    names,
                                    &name,
                                    format!("the inline enum of {}.{}", c.owner, c.prop),
                                    loc,
                                );
                                enums.push(InlineEnum {
                                    name: name.clone(),
                                    owner: format!("{}.{}", c.owner, c.prop),
                                    variants,
                                });
                                Some(Ty::Enum(name))
                            }
                            (None, None) => Some(Ty::String),
                            (None, Some("int64")) => Some(Ty::Decimal(Dec::I64)),
                            (None, Some("uint64")) => Some(Ty::Decimal(Dec::U64)),
                            (None, Some(f @ ("int32" | "uint32"))) => {
                                self.err("adp/format", loc, format!("format {f:?} belongs to `integer`; a `string` takes int64 or uint64"));
                                None
                            }
                            (None, Some(f)) => {
                                self.err(
                                    "adp/format",
                                    loc,
                                    format!("unknown format {f:?} for `string` (int64, uint64)"),
                                );
                                None
                            }
                        }
                    }
                    "integer" => {
                        let bad = bad_for(&["enum", "items", "properties"]);
                        if !bad.is_empty() {
                            return shape_err(
                                self,
                                &format!("`integer` takes no `{}`", bad.join("`, `")),
                            );
                        }
                        match sh.format.as_deref() {
                            Some("int32") => Some(Ty::Int(Int::I32)),
                            Some("uint32") => Some(Ty::Int(Int::U32)),
                            Some(f @ ("int64" | "uint64")) => {
                                self.err("adp/format", loc, format!("64-bit integers are JSON decimal strings: use `type: \"string\"` with format {f:?}"));
                                None
                            }
                            Some(f) => {
                                self.err(
                                    "adp/format",
                                    loc,
                                    format!("unknown format {f:?} for `integer` (int32, uint32)"),
                                );
                                None
                            }
                            None => self.unfixed_integer(c, loc),
                        }
                    }
                    "binary" | "object" => {
                        let bad = bad_for(&["enum", "items", "properties"]);
                        if !bad.is_empty() {
                            return shape_err(
                                self,
                                &format!("`{t}` takes no `{}`", bad.join("`, `")),
                            );
                        }
                        if let Some(f) = &sh.format {
                            self.err(
                                "adp/format",
                                loc,
                                format!("`{t}` takes no format (got {f:?})"),
                            );
                            return None;
                        }
                        Some(if t == "binary" {
                            Ty::Binary
                        } else {
                            Ty::Object
                        })
                    }
                    "number" | "boolean" => {
                        let bad = bad_for(&["enum", "items", "properties"]);
                        if !bad.is_empty() {
                            return shape_err(
                                self,
                                &format!("`{t}` takes no `{}`", bad.join("`, `")),
                            );
                        }
                        if let Some(f) = &sh.format {
                            self.err(
                                "adp/format",
                                loc,
                                format!("`{t}` takes no format (got {f:?})"),
                            );
                            return None;
                        }
                        Some(if t == "number" {
                            Ty::Number
                        } else {
                            Ty::Boolean
                        })
                    }
                    "array" => {
                        let bad = bad_for(&["enum", "properties"]);
                        if !bad.is_empty() {
                            return shape_err(
                                self,
                                &format!("`array` takes no `{}`", bad.join("`, `")),
                            );
                        }
                        if let Some(f) = &sh.format {
                            self.err(
                                "adp/format",
                                loc,
                                format!("`array` takes no format (got {f:?})"),
                            );
                            return None;
                        }
                        let Some(items) = &sh.items else {
                            return shape_err(self, "an `array` needs `items`");
                        };
                        let inner = self.ty(items, c, names, enums)?;
                        Some(Ty::Array(Box::new(inner)))
                    }
                    // unreachable: phase 1 rejects other type names
                    other => shape_err(self, &format!("unknown type {other:?}")),
                }
            }
        }
    }

    fn unfixed_integer(&mut self, c: &Ctx, loc: &str) -> Option<Ty> {
        self.unfixed.insert(c.key.to_string());
        let width = match self.settings.integer.get(c.key) {
            Some(IntegerWidth::I64) => Int::I64,
            Some(IntegerWidth::I32) => Int::I32,
            Some(IntegerWidth::U32) => Int::U32,
            None => match self.settings.integer_without_format {
                IntegerDefault::I64 => Int::I64,
                IntegerDefault::I32 => Int::I32,
                IntegerDefault::U32 => Int::U32,
                IntegerDefault::Reject => {
                    self.err(
                        "adp/integer-width",
                        loc,
                        format!(
                            "`integer` without `format`: the width is not defined; add format int32/uint32 to the definition, or set [adp.integer] \"{}\" in build/codegen/codegen.toml",
                            c.key
                        ),
                    );
                    return None;
                }
            },
        };
        Some(Ty::Int(width))
    }

    #[allow(clippy::too_many_arguments)]
    fn fields(
        &mut self,
        props: &[RProp],
        domain: usize,
        owner: &str,
        key_prefix: &str,
        inline_enums: bool,
        names: &mut BTreeMap<String, String>,
        enums: &mut Vec<InlineEnum>,
    ) -> Vec<Field> {
        let mut seen = BTreeSet::new();
        let mut idents = BTreeMap::new();
        let mut out = Vec::new();
        for p in props {
            if !seen.insert(p.name.clone()) {
                self.err(
                    "adp/duplicate",
                    &p.loc,
                    format!("property {:?} is listed twice in {owner}", p.name),
                );
                continue;
            }
            let ident = p.name.to_snake_case();
            let raw = KEYWORDS.contains(&ident.as_str());
            if !ascii_ident(&ident) || NOT_RAW.contains(&ident.as_str()) {
                self.err(
                    "adp/name",
                    &p.loc,
                    format!(
                        "property {:?} gives no valid Rust field name (`{ident}`)",
                        p.name
                    ),
                );
                continue;
            }
            self.claim(
                &mut idents,
                &ident,
                format!("property {:?}", p.name),
                &p.loc,
            );
            if p.nullable && !p.optional {
                self.err(
                    "adp/shape",
                    &p.loc,
                    "`nullable` applies to `optional` properties only (explicit null clears an optional field)",
                );
                continue;
            }
            let key = format!("{key_prefix}.{}", p.name);
            let c = Ctx {
                domain,
                owner,
                prop: &p.name,
                key: &key,
                inline_enums,
            };
            let Some(ty) = self.ty(&p.shape, &c, names, enums) else {
                continue;
            };
            if !helper_depth_ok(&ty) {
                self.err(
                    "adp/shape",
                    &p.loc,
                    "nested arrays of decimal-string integers or binary are not supported",
                );
                continue;
            }
            out.push(Field {
                rename: serde_camel(&ident) != p.name,
                wire: p.name.clone(),
                ident,
                raw,
                optional: p.optional,
                nullable: p.nullable,
                meta: p.meta.clone(),
                ty,
            });
        }
        out
    }

    /// Phase 2 for one domain (or the envelope, which has types only).
    fn domain(&mut self, d: usize, rd: &RDomain, module: String) -> Domain {
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        let mut enums = Vec::new();
        let mut types = Vec::new();
        // type ids first: they are the names the definition chose
        let mut type_ids = BTreeSet::new();
        for t in &rd.types {
            if !type_ids.insert(t.id.clone()) {
                continue; // reported below
            }
            if type_name_ok(&t.id) {
                names.insert(t.id.clone(), format!("type {}", t.id));
            } else {
                self.err(
                    "adp/name",
                    &t.loc,
                    format!(
                        "type id {:?} must be an ASCII identifier starting with an upper-case letter and not a keyword",
                        t.id
                    ),
                );
            }
        }
        let mut cmd_names = BTreeSet::new();
        let mut commands_raw = Vec::new();
        for c in &rd.commands {
            if !cmd_names.insert(c.name.clone()) {
                self.err(
                    "adp/duplicate",
                    &c.loc,
                    format!("command {:?} is listed twice", c.name),
                );
                continue;
            }
            let base = c.name.to_upper_camel_case();
            let (p, r) = (format!("{base}Params"), format!("{base}Returns"));
            if !type_name_ok(&p) {
                self.err(
                    "adp/name",
                    &c.loc,
                    format!("command {:?} gives no valid Rust type name (`{p}`)", c.name),
                );
                continue;
            }
            self.claim(
                &mut names,
                &p,
                format!("the parameters of command {}", c.name),
                &c.loc,
            );
            self.claim(
                &mut names,
                &r,
                format!("the result of command {}", c.name),
                &c.loc,
            );
            commands_raw.push((c, p, r));
        }
        let mut ev_names = BTreeSet::new();
        let mut events_raw = Vec::new();
        for e in &rd.events {
            if !ev_names.insert(e.name.clone()) {
                self.err(
                    "adp/duplicate",
                    &e.loc,
                    format!("event {:?} is listed twice", e.name),
                );
                continue;
            }
            let s = format!("{}Event", e.name.to_upper_camel_case());
            if !type_name_ok(&s) {
                self.err(
                    "adp/name",
                    &e.loc,
                    format!("event {:?} gives no valid Rust type name (`{s}`)", e.name),
                );
                continue;
            }
            self.claim(&mut names, &s, format!("event {}", e.name), &e.loc);
            events_raw.push((e, s));
        }

        let mut seen = BTreeSet::new();
        for t in &rd.types {
            if !seen.insert(t.id.clone()) {
                self.err(
                    "adp/duplicate",
                    &t.loc,
                    format!("type id {:?} is defined twice", t.id),
                );
                continue;
            }
            let key = format!("{}.{}", rd.name, t.id);
            if let Some(body) = self.type_body(t, d, &key, &mut names, &mut enums) {
                types.push(TypeDef {
                    id: t.id.clone(),
                    meta: t.meta.clone(),
                    body,
                });
            }
        }
        let mut commands = Vec::new();
        for (c, p, r) in commands_raw {
            let pk = format!("{}.{}.parameters", rd.name, c.name);
            let rk = format!("{}.{}.returns", rd.name, c.name);
            let params = self.fields(&c.params, d, &p, &pk, true, &mut names, &mut enums);
            let returns = self.fields(&c.returns, d, &r, &rk, true, &mut names, &mut enums);
            // checked here; resolved against the target type enum in `build`
            let target_types = c.target_types.as_ref().map(|values| {
                if c.scope == Scope::Connection {
                    self.err(
                        "adp/shape",
                        &c.loc,
                        "`targetTypes` applies to commands with scope session or both",
                    );
                }
                if values.is_empty() {
                    self.err(
                        "adp/shape",
                        &c.loc,
                        "`targetTypes` is empty: omit it while no target type is fixed",
                    );
                }
                let mut seen = BTreeSet::new();
                for v in values {
                    if !seen.insert(v) {
                        self.err(
                            "adp/duplicate",
                            &c.loc,
                            format!("target type {v:?} is listed twice in `targetTypes`"),
                        );
                    }
                }
                values
                    .iter()
                    .map(|v| Variant {
                        wire: v.clone(),
                        ident: String::new(),
                    })
                    .collect()
            });
            commands.push(Command {
                name: c.name.clone(),
                meta: c.meta.clone(),
                scope: c.scope,
                target_types,
                params_name: p,
                returns_name: r,
                params,
                returns,
            });
        }
        let mut events = Vec::new();
        for (e, s) in events_raw {
            let k = format!("{}.{}.parameters", rd.name, e.name);
            let params = self.fields(&e.params, d, &s, &k, true, &mut names, &mut enums);
            events.push(Event {
                name: e.name.clone(),
                meta: e.meta.clone(),
                scope: e.scope,
                struct_name: s,
                params,
            });
        }
        Domain {
            name: rd.name.clone(),
            module,
            meta: rd.meta.clone(),
            types,
            enums,
            commands,
            events,
        }
    }

    fn type_body(
        &mut self,
        t: &RType,
        d: usize,
        key: &str,
        names: &mut BTreeMap<String, String>,
        enums: &mut Vec<InlineEnum>,
    ) -> Option<TypeBody> {
        let sh = &t.shape;
        let union = t.discriminator.is_some() || t.variants.is_some();
        if union {
            return self.union(t, d, key, names, enums);
        }
        match sh.ty.as_deref() {
            Some("object") if sh.properties.is_some() => {
                if sh.enum_.is_some() || sh.items.is_some() || sh.format.is_some() {
                    self.err(
                        "adp/shape",
                        &t.loc,
                        "an `object` type takes no `enum`, `items` or `format`",
                    );
                    return None;
                }
                let props = sh.properties.as_deref().unwrap_or_default();
                Some(TypeBody::Struct(
                    self.fields(props, d, &t.id, key, true, names, enums),
                ))
            }
            Some("string")
                if sh.enum_.is_some()
                    && sh.format.is_none()
                    && sh.items.is_none()
                    && sh.properties.is_none() =>
            {
                let values = sh.enum_.as_deref().unwrap_or_default();
                if values.is_empty() {
                    self.err("adp/shape", &t.loc, "an `enum` needs at least one value");
                    return None;
                }
                Some(TypeBody::Enum(self.variants(values, &t.loc)))
            }
            _ => {
                if let Some(items) = &sh.items
                    && items.enum_.is_some()
                {
                    self.err(
                        "adp/shape",
                        &t.loc,
                        "an inline enum in an array type has no property to be named after: give the enum its own type id",
                    );
                    return None;
                }
                let c = Ctx {
                    domain: d,
                    owner: &t.id,
                    prop: "",
                    key,
                    inline_enums: false,
                };
                let ty = self.ty(sh, &c, names, enums)?;
                if !helper_depth_ok(&ty) {
                    self.err(
                        "adp/shape",
                        &t.loc,
                        "nested arrays of decimal-string integers or binary are not supported",
                    );
                    return None;
                }
                Some(TypeBody::Newtype(ty))
            }
        }
    }

    /// A tagged union: `type: object`, `discriminator` (the tag property) and `variants`
    /// (each with a unique `kind`, the tag value, and its own `properties`).
    fn union(
        &mut self,
        t: &RType,
        d: usize,
        key: &str,
        names: &mut BTreeMap<String, String>,
        enums: &mut Vec<InlineEnum>,
    ) -> Option<TypeBody> {
        let sh = &t.shape;
        let (Some(tag), Some(variants)) = (&t.discriminator, &t.variants) else {
            self.err(
                "adp/shape",
                &t.loc,
                "a tagged union needs both `discriminator` and `variants`",
            );
            return None;
        };
        if sh.ty.as_deref() != Some("object")
            || sh.properties.is_some()
            || sh.enum_.is_some()
            || sh.items.is_some()
            || sh.format.is_some()
        {
            self.err(
                "adp/shape",
                &t.loc,
                "a tagged union is `type: object` with `discriminator` and `variants` only (no `properties`, `enum`, `items`, `format`)",
            );
            return None;
        }
        if tag.is_empty() {
            self.err("adp/shape", &t.loc, "`discriminator` is empty");
            return None;
        }
        if variants.is_empty() {
            self.err(
                "adp/shape",
                &t.loc,
                "a tagged union needs at least one variant",
            );
            return None;
        }
        let mut kinds = BTreeSet::new();
        let mut idents = BTreeMap::new();
        let mut out = Vec::new();
        for v in variants {
            if !kinds.insert(v.kind.clone()) {
                self.err(
                    "adp/duplicate",
                    &v.loc,
                    format!("variant kind {:?} is listed twice", v.kind),
                );
                continue;
            }
            let ident = v.kind.to_upper_camel_case();
            if !type_name_ok(&ident) {
                self.err(
                    "adp/name",
                    &v.loc,
                    format!(
                        "variant kind {:?} gives no valid Rust variant name (`{ident}`)",
                        v.kind
                    ),
                );
                continue;
            }
            self.claim(
                &mut idents,
                &ident,
                format!("variant kind {:?}", v.kind),
                &v.loc,
            );
            if v.properties.iter().any(|p| &p.name == tag) {
                self.err(
                    "adp/name",
                    &v.loc,
                    format!(
                        "property {tag:?} is the discriminator and cannot be a variant property"
                    ),
                );
                continue;
            }
            let vkey = format!("{key}.{}", v.kind);
            let fields = self.fields(&v.properties, d, &t.id, &vkey, false, names, enums);
            out.push(UnionVariant {
                wire: v.kind.clone(),
                ident,
                description: v.description.clone(),
                fields,
            });
        }
        Some(TypeBody::Union {
            tag: tag.clone(),
            variants: out,
        })
    }
}

/// The string-carried helpers (decimal integers, base64) cover a value and one level of
/// array.
fn helper_depth_ok(ty: &Ty) -> bool {
    match ty {
        Ty::Array(inner) => match inner.as_ref() {
            Ty::Array(inner2) => !contains_string_carried(inner2),
            _ => true,
        },
        _ => true,
    }
}

fn contains_string_carried(ty: &Ty) -> bool {
    match ty {
        Ty::Decimal(_) | Ty::Binary => true,
        Ty::Array(inner) => contains_string_carried(inner),
        _ => false,
    }
}

fn direct_target(ty: &Ty) -> Option<(usize, usize)> {
    match ty {
        Ty::Ref { target, .. } => Some(*target),
        _ => None,
    }
}

/// Direct (not through an array) type references of a type.
fn direct_refs(t: &TypeDef) -> Vec<(usize, usize)> {
    match &t.body {
        TypeBody::Struct(fields) => fields.iter().filter_map(|f| direct_target(&f.ty)).collect(),
        TypeBody::Union { variants, .. } => variants
            .iter()
            .flat_map(|v| v.fields.iter().filter_map(|f| direct_target(&f.ty)))
            .collect(),
        TypeBody::Newtype(ty) => direct_target(ty).into_iter().collect(),
        TypeBody::Enum(_) => Vec::new(),
    }
}

/// Boxes every direct reference that leads back to the referring type. `index` maps a
/// definition position (domain, type index in the definition) to the model position.
fn box_cycles(domains: &mut [Domain], index: &BTreeMap<(usize, usize), (usize, usize)>) {
    let mut edges: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    for (&def_idx, &(d, t)) in index {
        edges.insert(def_idx, direct_refs(&domains[d].types[t]));
    }
    let reaches = |from: (usize, usize), to: (usize, usize)| -> bool {
        let mut stack = vec![from];
        let mut seen = BTreeSet::new();
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if seen.insert(n) {
                stack.extend(edges.get(&n).into_iter().flatten().copied());
            }
        }
        false
    };
    for (&def_idx, &(d, t)) in index {
        let fix = |ty: &mut Ty| {
            if let Ty::Ref { target, boxed, .. } = ty
                && reaches(*target, def_idx)
            {
                *boxed = true;
            }
        };
        match &mut domains[d].types[t].body {
            TypeBody::Struct(fields) => fields.iter_mut().for_each(|f| fix(&mut f.ty)),
            TypeBody::Union { variants, .. } => variants
                .iter_mut()
                .flat_map(|v| v.fields.iter_mut())
                .for_each(|f| fix(&mut f.ty)),
            TypeBody::Newtype(ty) => fix(ty),
            TypeBody::Enum(_) => {}
        }
    }
}

/// The name of the envelope pseudo-domain: `$ref` qualifier and module name.
pub(crate) const ENVELOPE: &str = "envelope";

/// Validates the definition and builds the model.
pub(crate) fn build(v: &Value, settings: &Settings) -> Result<Def, Vec<Problem>> {
    let mut ps = Vec::new();
    let raw = P1 { ps: &mut ps }.root(v);
    let raw = match raw {
        Some(raw) if ps.is_empty() => raw,
        _ => return Err(ps),
    };

    let mut p2 = P2 {
        ps,
        settings,
        lookup: BTreeMap::new(),
        modules: Vec::new(),
        unfixed: BTreeSet::new(),
    };

    // errors
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    let mut idents = BTreeMap::new();
    for e in &raw.errors {
        if !seen.insert(e.name.clone()) {
            p2.err(
                "adp/duplicate",
                &e.loc,
                format!("error {:?} is listed twice", e.name),
            );
            continue;
        }
        let variant = e.name.to_upper_camel_case();
        if !type_name_ok(&variant) {
            p2.err(
                "adp/name",
                &e.loc,
                format!("error {:?} gives no valid Rust variant name", e.name),
            );
            continue;
        }
        p2.claim(&mut idents, &variant, format!("error {:?}", e.name), &e.loc);
        errors.push(ErrorDef {
            wire: e.name.clone(),
            variant,
            description: e.description.clone(),
        });
    }

    // The envelope is position 0, as the pseudo-domain `envelope`; then the domains. Names,
    // modules and the type lookup use the first occurrence; duplicates are reported and
    // skipped.
    let envelope_domain = RDomain {
        loc: ENVELOPE.to_string(),
        name: ENVELOPE.to_string(),
        meta: Meta {
            description: raw.envelope.description.clone(),
            ..Meta::default()
        },
        types: raw.envelope.types.clone(),
        commands: Vec::new(),
        events: Vec::new(),
    };
    let mut kept: Vec<(&RDomain, String)> = Vec::new();
    let mut module_names = BTreeMap::new();
    let all: Vec<&RDomain> = std::iter::once(&envelope_domain)
        .chain(raw.domains.iter())
        .collect();
    for (i, rd) in all.into_iter().enumerate() {
        let module = rd.name.to_snake_case();
        if i > 0
            && (!ascii_ident(&module)
                || KEYWORDS.contains(&module.as_str())
                || RESERVED_MODULES.contains(&module.as_str()))
        {
            p2.err(
                "adp/name",
                &rd.loc,
                format!(
                    "domain {:?} gives the module name `{module}`, which is not a usable Rust module name (reserved: {})",
                    rd.name,
                    RESERVED_MODULES.join(", ")
                ),
            );
            continue;
        }
        if p2.lookup.contains_key(&rd.name) {
            p2.err(
                "adp/duplicate",
                &rd.loc,
                format!("domain {:?} is listed twice", rd.name),
            );
            continue;
        }
        p2.claim(
            &mut module_names,
            &module,
            format!("domain {:?}", rd.name),
            &rd.loc,
        );
        let mut types = BTreeMap::new();
        for (i, t) in rd.types.iter().enumerate() {
            types.entry(t.id.clone()).or_insert(i);
        }
        p2.lookup.insert(rd.name.clone(), (kept.len(), types));
        p2.modules.push(module.clone());
        kept.push((rd, module));
    }

    let mut domains = Vec::new();
    for (d, (rd, module)) in kept.into_iter().enumerate() {
        domains.push(p2.domain(d, rd, module));
    }

    for k in settings.integer.keys() {
        if !p2.unfixed.contains(k) {
            p2.err(
                "adp/setting",
                "build/codegen/codegen.toml [adp.integer]",
                format!("{k:?} is not an `integer` without `format` in the definition"),
            );
        }
    }

    if !p2.ps.is_empty() {
        return Err(p2.ps);
    }

    // Ref targets are definition positions; map them to model positions for boxing.
    let mut index = BTreeMap::new();
    for (d, dom) in domains.iter().enumerate() {
        let (_, ids) = &p2.lookup[&dom.name];
        for (t, td) in dom.types.iter().enumerate() {
            index.insert((d, ids[&td.id]), (d, t));
        }
    }
    box_cycles(&mut domains, &index);
    let envelope = domains.remove(0);
    // Ref modules were computed with the envelope at position 0; positions are not used
    // after boxing.
    let target_type = resolve_target_types(&mut domains, settings)?;
    Ok(Def {
        errors,
        envelope,
        binary: raw.envelope.binary,
        domains,
        target_type,
    })
}

/// The `targetTypes` enum (setting `target_type`, `<Domain>.<Type>`): `(module, type id)`
/// when the definition has it, after mapping every command's `targetTypes` to its variants.
fn resolve_target_types(
    domains: &mut [Domain],
    settings: &Settings,
) -> Result<Option<(String, String)>, Vec<Problem>> {
    let setting_loc = "build/codegen/codegen.toml [adp] target_type";
    let wanted = settings.target_type.split_once('.');
    let found = wanted.and_then(|(d, t)| {
        let dom = domains.iter().find(|x| x.name == d)?;
        let td = dom.types.iter().find(|x| x.id == t)?;
        Some((dom.module.clone(), td.id.clone(), td.body.clone()))
    });
    let used = domains
        .iter()
        .flat_map(|d| &d.commands)
        .any(|c| c.target_types.is_some());
    let (module, id, variants) = match found {
        Some((m, id, TypeBody::Enum(vs))) => (m, id, vs),
        Some(_) => {
            return Err(vec![problem(
                "adp/setting",
                setting_loc,
                format!("{:?} is not a string enum type", settings.target_type),
            )]);
        }
        None if used => {
            return Err(vec![problem(
                "adp/setting",
                setting_loc,
                format!(
                    "{:?} names no type of the definition, but commands use `targetTypes`",
                    settings.target_type
                ),
            )]);
        }
        None => return Ok(None),
    };
    let mut ps = Vec::new();
    for d in domains.iter_mut() {
        let dname = d.name.clone();
        for c in &mut d.commands {
            for v in c.target_types.iter_mut().flatten() {
                match variants.iter().find(|x| x.wire == v.wire) {
                    Some(x) => v.ident = x.ident.clone(),
                    None => ps.push(problem(
                        "adp/unresolved-ref",
                        format!("{dname}.{}", c.name),
                        format!(
                            "target type {:?} is not a value of {}",
                            v.wire, settings.target_type
                        ),
                    )),
                }
            }
        }
    }
    if ps.is_empty() {
        Ok(Some((module, id)))
    } else {
        Err(ps)
    }
}
