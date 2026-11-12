//! ADP generator fixture tests: definition validation (every problem kind) and the Rust
//! mapping rules. Assertions on problems compare the exact rule ids.

use codegen::adp::{self, IntegerDefault, IntegerWidth, Settings};
use serde_json::{Value, json};

/// A small definition that uses every supported construct once.
fn def() -> Value {
    json!({
        "description": "fixture",
        "errors": [
            { "name": "FRAME_EXPIRED", "description": "Expired." },
            { "name": "NODE_GONE" }
        ],
        "envelope": {
            "description": "Envelope.",
            "types": [
                {
                    "id": "Request",
                    "type": "object",
                    "properties": [
                        { "name": "id", "type": "integer" },
                        { "name": "sessionId", "optional": true, "$ref": "UI.NodeId" },
                        { "name": "params", "optional": true, "type": "object" },
                        { "name": "error", "optional": true, "$ref": "Error" }
                    ]
                },
                { "id": "Error", "type": "object", "properties": [ { "name": "code", "type": "string" } ] }
            ],
            "binary": "Binary framing."
        },
        "domains": [
            { "domain": "Target" },
            {
                "domain": "UI",
                "description": "UI domain.",
                "types": [
                    { "id": "NodeId", "description": "Opaque.", "type": "string" },
                    {
                        "id": "FrameId",
                        "type": "object",
                        "properties": [
                            { "name": "display", "type": "integer", "format": "uint32" },
                            { "name": "delta", "type": "integer", "format": "int32" },
                            { "name": "vsyncId", "description": "Decimal.", "type": "string", "format": "int64" },
                            { "name": "sfFrame", "type": "string", "format": "uint64" },
                            { "name": "lastFrame", "optional": true, "type": "string", "format": "uint64" }
                        ]
                    },
                    {
                        "id": "Node",
                        "type": "object",
                        "properties": [
                            { "name": "id", "$ref": "NodeId" },
                            { "name": "source", "type": "string", "enum": ["view", "a11y"] },
                            { "name": "text", "optional": true, "type": "string" },
                            { "name": "alpha", "optional": true, "type": "number" },
                            { "name": "visible", "type": "boolean" },
                            { "name": "children", "optional": true, "type": "array", "items": { "$ref": "Node" } },
                            { "name": "hint", "optional": true, "nullable": true, "type": "string" },
                            { "name": "seq", "optional": true, "nullable": true, "type": "string", "format": "uint64" }
                        ]
                    }
                ],
                "commands": [
                    {
                        "name": "getTree",
                        "description": "Returns the tree.",
                        "scope": "session",
                        "parameters": [
                            { "name": "sources", "type": "array", "items": { "type": "string", "enum": ["view", "a11y"] } },
                            { "name": "depth", "type": "integer" },
                            { "name": "windowId", "optional": true, "type": "string" }
                        ],
                        "returns": [
                            { "name": "frameId", "$ref": "FrameId" },
                            { "name": "root", "$ref": "Node" }
                        ]
                    },
                    { "name": "reset", "scope": "connection", "returns": [] }
                ],
                "events": [
                    { "name": "treeChanged", "scope": "both", "parameters": [ { "name": "frameId", "$ref": "FrameId" } ] }
                ]
            },
            {
                "domain": "WebView",
                "types": [
                    { "id": "Target", "type": "object", "properties": [ { "name": "node", "$ref": "UI.NodeId" } ] },
                    {
                        "id": "Source",
                        "description": "Union.",
                        "type": "object",
                        "discriminator": "kind",
                        "variants": [
                            { "kind": "upload", "description": "Uploaded.", "properties": [ { "name": "uploadId", "type": "string" } ] },
                            { "kind": "url", "properties": [ { "name": "data", "optional": true, "type": "binary" } ] }
                        ]
                    },
                    { "id": "Blob", "type": "binary" },
                    { "id": "Extra", "type": "object" }
                ]
            }
        ]
    })
}

/// Appends a type to the UI domain.
fn push_type(v: &mut Value, t: Value) {
    v["domains"][1]["types"].as_array_mut().unwrap().push(t);
}

fn gen_with(v: &Value, s: &Settings) -> Result<String, Vec<String>> {
    adp::generate(&v.to_string(), "1.2.3", s)
        .map_err(|ps| ps.iter().map(|p| p.rule.to_string()).collect())
}

fn gen_ok(v: &Value) -> String {
    match gen_with(v, &Settings::default()) {
        Ok(s) => s,
        Err(rules) => panic!("unexpected problems: {rules:?}"),
    }
}

/// The rule ids reported for `v`, sorted and deduplicated.
fn problems(v: &Value) -> Vec<String> {
    problems_with(v, &Settings::default())
}

fn problems_with(v: &Value, s: &Settings) -> Vec<String> {
    match gen_with(v, s) {
        Ok(_) => Vec::new(),
        Err(mut rules) => {
            rules.sort();
            rules.dedup();
            rules
        }
    }
}

/// Squashes all whitespace so assertions do not depend on line breaking.
fn flat(s: &str) -> String {
    let mut out: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    // prettyplease breaks long attributes and generics and adds trailing commas
    for (from, to) in [(",>", ">"), (",)", ")"), (",}", "}")] {
        out = out.replace(from, to);
    }
    out
}

fn has(out: &str, needle: &str) -> bool {
    let out = flat(out);
    let needle = flat(needle);
    match needle.strip_suffix(',') {
        // a trailing comma in the needle may have been dropped before a closing bracket
        Some(stem) => [",", "}", ")", ">"]
            .iter()
            .any(|end| out.contains(&format!("{stem}{end}"))),
        None => out.contains(&needle),
    }
}

#[track_caller]
fn assert_has(out: &str, needle: &str) {
    assert!(has(out, needle), "missing `{needle}` in:\n{out}");
}

// ---------------------------------------------------------------------------
// mapping rules
// ---------------------------------------------------------------------------

#[test]
fn version_and_errors_module() {
    let out = gen_ok(&def());
    assert_has(&out, r#"pub const PROTOCOL_VERSION: &str = "1.2.3";"#);
    assert_has(&out, "pub mod errors {");
    assert_has(&out, "pub enum ErrorCode {");
    assert_has(&out, r#"#[serde(rename = "FRAME_EXPIRED")] FrameExpired,"#);
    assert_has(&out, r#"Self::NodeGone => "NODE_GONE","#);
    assert_has(&out, "/// Expired.");
}

#[test]
fn one_module_per_domain_including_empty_ones() {
    let out = gen_ok(&def());
    assert_has(&out, "pub mod target {");
    assert_has(&out, "pub mod ui {");
    assert_has(&out, "pub mod web_view {");
    assert_has(&out, "/// UI domain.");
}

#[test]
fn structs_are_camel_case_serde() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        r#"#[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)] #[serde(rename_all = "camelCase", deny_unknown_fields)] pub struct Node {"#,
    );
}

#[test]
fn optional_is_option_and_skipped_when_none() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        r#"#[serde(default, skip_serializing_if = "::core::option::Option::is_none")] pub text: ::core::option::Option<::std::string::String>,"#,
    );
}

#[test]
fn integer_formats_map_to_fixed_widths() {
    let out = gen_ok(&def());
    assert_has(&out, "pub display: u32,");
    assert_has(&out, "pub delta: i32,");
    assert_has(&out, "pub alpha: ::core::option::Option<f64>,");
    assert_has(&out, "pub visible: bool,");
}

#[test]
fn sixty_four_bit_integers_are_decimal_strings() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        r#"#[serde(with = "super::wire::one")] pub vsync_id: i64,"#,
    );
    assert_has(
        &out,
        r#"#[serde(with = "super::wire::one")] pub sf_frame: u64,"#,
    );
    assert_has(
        &out,
        r#"#[serde(default, skip_serializing_if = "::core::option::Option::is_none", with = "super::wire::option")] pub last_frame: ::core::option::Option<u64>,"#,
    );
    assert_has(&out, "pub mod wire {");
}

#[test]
fn inline_enums_are_named_type_property() {
    let out = gen_ok(&def());
    assert_has(&out, "pub source: NodeSource,");
    assert_has(&out, "pub enum NodeSource {");
    assert_has(&out, r#"#[serde(rename = "a11y")] A11y,"#);
    // commands: <Command>Params is the type name
    assert_has(&out, "pub sources: ::std::vec::Vec<GetTreeParamsSources>,");
    assert_has(&out, "pub enum GetTreeParamsSources {");
}

#[test]
fn commands_get_params_and_returns_events_get_event() {
    let out = gen_ok(&def());
    assert_has(&out, "pub struct GetTreeParams {");
    assert_has(&out, "pub struct GetTreeReturns {");
    assert_has(&out, "pub struct ResetParams {}");
    assert_has(&out, "pub struct ResetReturns {}");
    assert_has(&out, "pub struct TreeChangedEvent {");
    assert_has(&out, "/// Returns the tree.");
}

#[test]
fn string_types_are_transparent_newtypes() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        "#[derive(Debug, Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)] #[serde(transparent)] pub struct NodeId(pub ::std::string::String);",
    );
}

#[test]
fn refs_resolve_locally_and_across_domains() {
    let out = gen_ok(&def());
    assert_has(&out, "pub id: NodeId,");
    assert_has(&out, "pub node: super::ui::NodeId,");
    // recursion through an array needs no box
    assert_has(
        &out,
        "pub children: ::core::option::Option<::std::vec::Vec<Node>>,",
    );
}

#[test]
fn direct_recursion_is_boxed() {
    let mut v = def();
    v["domains"][1]["types"][2]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "parent", "optional": true, "$ref": "Node" }));
    let out = gen_ok(&v);
    assert_has(
        &out,
        "pub parent: ::core::option::Option<::std::boxed::Box<Node>>,",
    );
}

#[test]
fn keyword_fields_use_raw_identifiers() {
    let mut v = def();
    v["domains"][1]["types"][2]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "type", "type": "string" }));
    let out = gen_ok(&v);
    assert_has(&out, "pub r#type: ::std::string::String,");
}

#[test]
fn names_that_do_not_round_trip_get_an_explicit_rename() {
    let mut v = def();
    v["domains"][1]["types"][2]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "URL", "type": "string" }));
    let out = gen_ok(&v);
    assert_has(
        &out,
        r#"#[serde(rename = "URL")] pub url: ::std::string::String,"#,
    );
}

#[test]
fn integer_without_format_default_is_i64() {
    let out = gen_ok(&def());
    assert_has(&out, "pub depth: i64,");
}

#[test]
fn integer_without_format_setting() {
    for (d, ty) in [(IntegerDefault::I32, "i32"), (IntegerDefault::U32, "u32")] {
        let s = Settings {
            integer_without_format: d,
            ..Settings::default()
        };
        let out = gen_with(&def(), &s).unwrap();
        assert_has(&out, &format!("pub depth: {ty},"));
    }
    let reject = Settings {
        integer_without_format: IntegerDefault::Reject,
        ..Settings::default()
    };
    assert_eq!(problems_with(&def(), &reject), ["adp/integer-width"]);
}

#[test]
fn integer_override_per_property() {
    let mut s = Settings {
        integer_without_format: IntegerDefault::Reject,
        ..Settings::default()
    };
    s.integer
        .insert("UI.getTree.parameters.depth".into(), IntegerWidth::U32);
    s.integer
        .insert("envelope.Request.id".into(), IntegerWidth::I64);
    let out = gen_with(&def(), &s).unwrap();
    assert_has(&out, "pub depth: u32,");
}

#[test]
fn integer_override_for_unknown_or_formatted_property_is_an_error() {
    let mut s = Settings::default();
    s.integer
        .insert("UI.getTree.parameters.nope".into(), IntegerWidth::U32);
    assert_eq!(problems_with(&def(), &s), ["adp/setting"]);

    let mut s = Settings::default();
    s.integer
        .insert("UI.FrameId.display".into(), IntegerWidth::I32);
    assert_eq!(problems_with(&def(), &s), ["adp/setting"]);
}

#[test]
fn deny_unknown_fields_setting() {
    let s = Settings {
        deny_unknown_fields: false,
        ..Settings::default()
    };
    let out = gen_with(&def(), &s).unwrap();
    assert!(!out.contains("deny_unknown_fields"));
    assert_has(
        &out,
        r#"#[serde(rename_all = "camelCase")] pub struct Node {"#,
    );
}

#[test]
fn output_is_deterministic_and_lf_only() {
    let a = gen_ok(&def());
    let b = gen_ok(&def());
    assert_eq!(a, b);
    assert!(!a.contains('\r'));
    assert!(a.ends_with('\n'));
}

// ---------------------------------------------------------------------------
// validation
// ---------------------------------------------------------------------------

#[test]
fn the_fixture_is_valid() {
    assert!(problems(&def()).is_empty());
}

#[test]
fn invalid_json() {
    let r = adp::generate("{", "1.2.3", &Settings::default()).unwrap_err();
    assert_eq!(r[0].rule, "adp/json");
}

#[test]
fn invalid_version() {
    for bad in ["", "1.2", "v1.2.3", "1.2.3\nx", "01.2.3"] {
        let r = adp::generate(&def().to_string(), bad, &Settings::default()).unwrap_err();
        assert_eq!(r[0].rule, "adp/version", "{bad:?}");
    }
}

#[test]
fn unknown_keys_are_rejected_everywhere() {
    let paths: [&dyn Fn(&mut Value); 7] = [
        &|v| v["dependencies"] = json!([]),
        &|v| v["errors"][0]["code"] = json!(1),
        &|v| v["domains"][1]["dependencies"] = json!([]),
        &|v| v["domains"][1]["types"][0]["pattern"] = json!("x"),
        &|v| v["domains"][1]["types"][1]["properties"][0]["minimum"] = json!(0),
        &|v| v["domains"][1]["commands"][0]["redirect"] = json!("x"),
        &|v| v["domains"][1]["commands"][0]["parameters"][0]["items"]["x"] = json!(1),
    ];
    for f in paths {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/unknown-key"], "{v}");
    }
}

#[test]
fn every_unknown_key_is_reported() {
    let mut v = def();
    v["a"] = json!(1);
    v["b"] = json!(1);
    let r = adp::generate(&v.to_string(), "1.2.3", &Settings::default()).unwrap_err();
    assert_eq!(r.len(), 2);
}

#[test]
fn missing_keys() {
    let cases: [&dyn Fn(&mut Value); 4] = [
        &|v| {
            v.as_object_mut().unwrap().remove("domains");
        },
        &|v| {
            v["domains"][1].as_object_mut().unwrap().remove("domain");
        },
        &|v| {
            v["domains"][1]["types"][0]
                .as_object_mut()
                .unwrap()
                .remove("id");
        },
        &|v| {
            v["domains"][1]["types"][1]["properties"][0]
                .as_object_mut()
                .unwrap()
                .remove("name");
        },
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/missing-key"], "{v}");
    }
}

#[test]
fn wrong_value_types() {
    let cases: [&dyn Fn(&mut Value); 4] = [
        &|v| v["domains"][1]["types"][1]["properties"][4]["optional"] = json!("yes"),
        &|v| v["domains"][1]["description"] = json!(3),
        &|v| v["errors"] = json!({}),
        &|v| v["domains"][1]["types"][0]["type"] = json!("any"),
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/type"], "{v}");
    }
}

#[test]
fn duplicate_names() {
    let cases: [&dyn Fn(&mut Value); 7] = [
        &|v| v["errors"][1]["name"] = json!("FRAME_EXPIRED"),
        &|v| v["domains"][2]["domain"] = json!("Target"),
        &|v| push_type(v, json!({ "id": "NodeId", "type": "string" })),
        &|v| v["domains"][1]["commands"][1]["name"] = json!("getTree"),
        &|v| v["domains"][1]["types"][1]["properties"][1]["name"] = json!("display"),
        &|v| v["domains"][1]["types"][2]["properties"][1]["enum"] = json!(["view", "view"]),
        &|v| v["domains"][1]["commands"][0]["returns"][1]["name"] = json!("frameId"),
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/duplicate"], "{v}");
    }
}

#[test]
fn unresolved_refs() {
    let cases: [&dyn Fn(&mut Value); 4] = [
        &|v| v["domains"][1]["types"][2]["properties"][0]["$ref"] = json!("Missing"),
        &|v| v["domains"][2]["types"][0]["properties"][0]["$ref"] = json!("NodeId"),
        &|v| v["domains"][2]["types"][0]["properties"][0]["$ref"] = json!("Nope.NodeId"),
        &|v| v["domains"][1]["commands"][0]["returns"][0]["$ref"] = json!("UI.Missing"),
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/unresolved-ref"], "{v}");
    }
}

#[test]
fn shape_errors() {
    let cases: [&dyn Fn(&mut Value); 8] = [
        // both $ref and type
        &|v| v["domains"][1]["types"][2]["properties"][0]["type"] = json!("string"),
        // neither
        &|v| {
            v["domains"][1]["types"][2]["properties"][0]
                .as_object_mut()
                .unwrap()
                .remove("$ref");
        },
        // enum on a non-string
        &|v| v["domains"][1]["types"][2]["properties"][4]["enum"] = json!(["a"]),
        // array without items
        &|v| {
            v["domains"][1]["types"][2]["properties"][5]
                .as_object_mut()
                .unwrap()
                .remove("items");
        },
        // items on a non-array
        &|v| v["domains"][1]["types"][2]["properties"][2]["items"] = json!({ "type": "string" }),
        // union with properties
        &|v| v["domains"][2]["types"][1]["properties"] = json!([]),
        // properties on a non-object type
        &|v| v["domains"][1]["types"][0]["properties"] = json!([]),
        // empty enum
        &|v| v["domains"][1]["types"][2]["properties"][1]["enum"] = json!([]),
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/shape"], "{v}");
    }
}

#[test]
fn format_rules() {
    let cases: [&dyn Fn(&mut Value); 5] = [
        // 64-bit integers are decimal strings, never `integer`
        &|v| v["domains"][1]["types"][1]["properties"][0]["format"] = json!("int64"),
        &|v| v["domains"][1]["types"][1]["properties"][0]["format"] = json!("uint64"),
        // 32-bit formats are for `integer` only
        &|v| v["domains"][1]["types"][1]["properties"][2]["format"] = json!("uint32"),
        // format on number / unknown format
        &|v| v["domains"][1]["types"][2]["properties"][3]["format"] = json!("double"),
        &|v| v["domains"][1]["types"][1]["properties"][0]["format"] = json!("uint16"),
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/format"], "{v}");
    }
}

#[test]
fn decimal_string_enum_is_a_shape_error() {
    let mut v = def();
    v["domains"][1]["types"][1]["properties"][2]["enum"] = json!(["1"]);
    assert_eq!(problems(&v), ["adp/shape"]);
}

#[test]
fn name_rules() {
    let cases: [&dyn Fn(&mut Value); 7] = [
        // type ids are UpperCamelCase identifiers
        &|v| push_type(v, json!({ "id": "lowerCase", "type": "string" })),
        &|v| push_type(v, json!({ "id": "Self", "type": "string" })),
        // a generated name collides with a type id
        &|v| push_type(v, json!({ "id": "GetTreeParams", "type": "string" })),
        &|v| push_type(v, json!({ "id": "NodeSource", "type": "string" })),
        // enum values that collide after case conversion / are not identifiers
        &|v| v["domains"][1]["types"][2]["properties"][1]["enum"] = json!(["a-b", "a_b"]),
        &|v| v["domains"][1]["types"][2]["properties"][1]["enum"] = json!(["1x"]),
        // domain modules may not shadow the root modules
        &|v| v["domains"][0]["domain"] = json!("Errors"),
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/name"], "{v}");
    }
}

#[test]
fn self_field_name_is_rejected() {
    let mut v = def();
    v["domains"][1]["types"][2]["properties"][2]["name"] = json!("self");
    assert_eq!(problems(&v), ["adp/name"]);
}

// ---------------------------------------------------------------------------
// ADP extensions: envelope, scope, tagged unions, binary, arbitrary objects
// ---------------------------------------------------------------------------

#[test]
fn envelope_is_a_root_module() {
    let out = gen_ok(&def());
    assert_has(&out, "pub mod envelope {");
    assert_has(&out, "/// Envelope.");
    assert_has(&out, "/// Binary messages: Binary framing.");
    assert_has(&out, "pub struct Request {");
    // envelope refs: into a domain, and envelope-local
    assert_has(
        &out,
        "pub session_id: ::core::option::Option<super::ui::NodeId>,",
    );
    assert_has(&out, "pub error: ::core::option::Option<Error>,");
    // integer without format in the envelope uses the same setting
    assert_has(&out, "pub id: i64,");
}

#[test]
fn object_without_properties_is_any_json_object() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        "pub params: ::core::option::Option<::serde_json::Map<::std::string::String, ::serde_json::Value>>,",
    );
    assert_has(
        &out,
        "#[serde(transparent)] pub struct Extra(pub ::serde_json::Map<::std::string::String, ::serde_json::Value>);",
    );
}

#[test]
fn binary_is_base64_bytes() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        r#"#[serde(transparent)] pub struct Blob(#[serde(with = "super::wire::one")] pub ::std::vec::Vec<u8>);"#,
    );
    assert_has(
        &out,
        r#"#[serde(default, skip_serializing_if = "::core::option::Option::is_none", with = "super::wire::option")] data: ::core::option::Option<::std::vec::Vec<u8>>,"#,
    );
    assert_has(&out, "base64::engine::general_purpose::STANDARD");
}

#[test]
fn tagged_union_is_an_internally_tagged_enum() {
    let out = gen_ok(&def());
    assert_has(
        &out,
        r#"#[serde(tag = "kind", deny_unknown_fields)] pub enum Source {"#,
    );
    assert_has(
        &out,
        r#"/// Uploaded. #[serde(rename = "upload", rename_all = "camelCase")] Upload { upload_id: ::std::string::String, },"#,
    );
    assert_has(
        &out,
        r#"#[serde(rename = "url", rename_all = "camelCase")] Url {"#,
    );
}

#[test]
fn scope_and_method_are_on_the_command_and_event_traits() {
    let out = gen_ok(&def());
    assert_has(&out, "pub enum Scope {");
    assert_has(&out, "pub trait Command:");
    assert_has(
        &out,
        r#"impl super::Command for GetTreeParams { type Returns = GetTreeReturns; const METHOD: &'static str = "UI.getTree"; const SCOPE: super::Scope = super::Scope::Session; }"#,
    );
    assert_has(
        &out,
        "const SCOPE: super::Scope = super::Scope::Connection;",
    );
    assert_has(
        &out,
        r#"impl super::Event for TreeChangedEvent { const METHOD: &'static str = "UI.treeChanged"; const SCOPE: super::Scope = super::Scope::Both; }"#,
    );
}

#[test]
fn envelope_and_scope_and_returns_are_required() {
    let cases: [&dyn Fn(&mut Value); 5] = [
        &|v| {
            v.as_object_mut().unwrap().remove("envelope");
        },
        &|v| {
            v["envelope"].as_object_mut().unwrap().remove("binary");
        },
        &|v| {
            v["domains"][1]["commands"][0]
                .as_object_mut()
                .unwrap()
                .remove("scope");
        },
        &|v| {
            v["domains"][1]["commands"][1]
                .as_object_mut()
                .unwrap()
                .remove("returns");
        },
        &|v| {
            v["domains"][1]["events"][0]
                .as_object_mut()
                .unwrap()
                .remove("scope");
        },
    ];
    for f in cases {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/missing-key"], "{v}");
    }
}

#[test]
fn scope_values_and_event_returns() {
    let mut v = def();
    v["domains"][1]["commands"][0]["scope"] = json!("global");
    assert_eq!(problems(&v), ["adp/type"]);
    let mut v = def();
    v["domains"][1]["events"][0]["returns"] = json!([]);
    assert_eq!(problems(&v), ["adp/unknown-key"]);
    let mut v = def();
    v["envelope"]["commands"] = json!([]);
    assert_eq!(problems(&v), ["adp/unknown-key"]);
}

#[test]
fn union_rules() {
    let shape: [&dyn Fn(&mut Value); 4] = [
        // discriminator without variants, variants without discriminator
        &|v| {
            v["domains"][2]["types"][1]
                .as_object_mut()
                .unwrap()
                .remove("variants");
        },
        &|v| {
            v["domains"][2]["types"][1]
                .as_object_mut()
                .unwrap()
                .remove("discriminator");
        },
        // a union is an object
        &|v| v["domains"][2]["types"][1]["type"] = json!("string"),
        // no variants
        &|v| v["domains"][2]["types"][1]["variants"] = json!([]),
    ];
    for f in shape {
        let mut v = def();
        f(&mut v);
        assert_eq!(problems(&v), ["adp/shape"], "{v}");
    }
    // duplicate kind
    let mut v = def();
    v["domains"][2]["types"][1]["variants"][1]["kind"] = json!("upload");
    assert_eq!(problems(&v), ["adp/duplicate"]);
    // the discriminator cannot be a variant property
    let mut v = def();
    v["domains"][2]["types"][1]["variants"][0]["properties"][0]["name"] = json!("kind");
    assert_eq!(problems(&v), ["adp/name"]);
    // unknown variant key
    let mut v = def();
    v["domains"][2]["types"][1]["variants"][0]["optional"] = json!(true);
    assert_eq!(problems(&v), ["adp/unknown-key"]);
    // inline enums inside variants have no agreed name
    let mut v = def();
    v["domains"][2]["types"][1]["variants"][0]["properties"][0]["enum"] = json!(["a"]);
    assert_eq!(problems(&v), ["adp/shape"]);
}

#[test]
fn binary_and_object_take_no_format_or_enum() {
    let mut v = def();
    v["domains"][2]["types"][2]["format"] = json!("int64");
    assert_eq!(problems(&v), ["adp/format"]);
    let mut v = def();
    v["envelope"]["types"][0]["properties"][2]["enum"] = json!(["a"]);
    assert_eq!(problems(&v), ["adp/shape"]);
}

#[test]
fn refs_into_the_envelope_need_the_qualifier() {
    let mut v = def();
    v["domains"][2]["types"][0]["properties"][0]["$ref"] = json!("envelope.Error");
    let out = gen_ok(&v);
    assert_has(&out, "pub node: super::envelope::Error,");
    let mut v = def();
    v["domains"][2]["types"][0]["properties"][0]["$ref"] = json!("Error");
    assert_eq!(problems(&v), ["adp/unresolved-ref"]);
}

#[test]
fn root_module_names_are_reserved() {
    for name in ["Envelope", "Wire", "Errors"] {
        let mut v = def();
        v["domains"][0]["domain"] = json!(name);
        assert_eq!(problems(&v), ["adp/name"], "{name}");
    }
}

#[test]
fn integer_override_in_the_envelope() {
    let mut s = Settings {
        integer_without_format: IntegerDefault::Reject,
        ..Settings::default()
    };
    assert_eq!(problems_with(&def(), &s), ["adp/integer-width"]);
    s.integer
        .insert("envelope.Request.id".into(), IntegerWidth::U32);
    s.integer
        .insert("UI.getTree.parameters.depth".into(), IntegerWidth::U32);
    let out = gen_with(&def(), &s).unwrap();
    assert_has(&out, "pub id: u32,");
}

#[test]
fn nullable_optional_properties_are_tri_state() {
    let out = gen_ok(&def());
    assert_has(&out, "pub enum Nullable<T> {");
    assert_has(
        &out,
        r#"#[serde(default, skip_serializing_if = "super::Nullable::is_absent")] pub hint: super::Nullable<::std::string::String>,"#,
    );
    assert_has(
        &out,
        r#"#[serde(default, skip_serializing_if = "super::Nullable::is_absent", with = "super::wire::nullable")] pub seq: super::Nullable<u64>,"#,
    );
}

#[test]
fn nullable_rules() {
    // only on optional properties
    let mut v = def();
    v["domains"][1]["types"][2]["properties"][4]["nullable"] = json!(true);
    assert_eq!(problems(&v), ["adp/shape"]);
    // a boolean
    let mut v = def();
    v["domains"][1]["types"][2]["properties"][2]["nullable"] = json!("yes");
    assert_eq!(problems(&v), ["adp/type"]);
    // not on array items
    let mut v = def();
    v["domains"][1]["types"][2]["properties"][5]["items"]["nullable"] = json!(true);
    assert_eq!(problems(&v), ["adp/unknown-key"]);
}
