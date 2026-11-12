//! Wire format of the committed adp generated code (contracts/adp/src/generated), checked
//! through the real contract crate: camelCase names, optional fields, enums, error codes,
//! 64-bit integers as canonical JSON decimal strings, and refusal of anything else.

use droiduse_contracts_adp::errors::ErrorCode;
use droiduse_contracts_adp::ui::{FrameId, GetTreeParams, GetTreeParamsSources, Node, NodeSource};
use serde_json::{Value, json};

fn frame(vsync: Value, sf: Value) -> Value {
    json!({ "display": 0, "vsyncId": vsync, "sfFrame": sf })
}

fn parse_frame(v: Value) -> Result<FrameId, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn protocol_version_is_the_version_file() {
    let v = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/adp/VERSION"
    ))
    .unwrap();
    assert_eq!(droiduse_contracts_adp::PROTOCOL_VERSION, v.trim());
}

#[test]
fn frame_id_round_trips_with_decimal_strings() {
    let f = FrameId {
        display: 4_000_000_000,
        vsync_id: i64::MIN,
        sf_frame: u64::MAX,
    };
    let v = serde_json::to_value(&f).unwrap();
    assert_eq!(
        v,
        json!({ "display": 4_000_000_000u32, "vsyncId": "-9223372036854775808", "sfFrame": "18446744073709551615" })
    );
    assert_eq!(parse_frame(v).unwrap(), f);
}

#[test]
fn decimal_strings_reject_numbers() {
    assert!(parse_frame(frame(json!(1), json!("1"))).is_err());
    assert!(parse_frame(frame(json!("1"), json!(1))).is_err());
}

#[test]
fn decimal_strings_reject_non_canonical_or_out_of_range() {
    for bad in [
        "",
        " 1",
        "1 ",
        "+1",
        "01",
        "-0",
        "1.0",
        "1e3",
        "0x10",
        "١",
        "-",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        assert!(
            parse_frame(frame(json!(bad), json!("0"))).is_err(),
            "vsyncId {bad:?}"
        );
    }
    for bad in ["-1", "18446744073709551616", "+0", "00"] {
        assert!(
            parse_frame(frame(json!("0"), json!(bad))).is_err(),
            "sfFrame {bad:?}"
        );
    }
    assert!(parse_frame(frame(json!("0"), json!("0"))).is_ok());
    assert!(parse_frame(frame(json!("-1"), json!("10"))).is_ok());
}

#[test]
fn u32_rejects_negative_fraction_and_overflow() {
    for bad in [json!(-1), json!(1.5), json!(4_294_967_296u64), json!("1")] {
        let v = json!({ "display": bad, "vsyncId": "0", "sfFrame": "0" });
        assert!(parse_frame(v).is_err());
    }
}

#[test]
fn unknown_fields_are_rejected() {
    let mut v = frame(json!("0"), json!("0"));
    v["extra"] = json!(1);
    assert!(parse_frame(v).is_err());
}

#[test]
fn missing_required_fields_are_rejected() {
    assert!(parse_frame(json!({ "display": 0, "vsyncId": "0" })).is_err());
}

#[test]
fn node_optional_fields_are_omitted_and_names_are_camel_case() {
    let v = json!({
        "id": "n1", "source": "compose", "windowId": "w",
        "bounds": { "x": 0.0, "y": 1.0, "w": 2.0, "h": 3.0 },
        "visible": true,
        "longClickable": true,
        "children": [ {
            "id": "n2", "source": "a11y", "windowId": "w",
            "bounds": { "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0 }, "visible": false
        } ]
    });
    let n: Node = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(n.source, NodeSource::Compose);
    assert_eq!(n.long_clickable, Some(true));
    assert_eq!(n.text, None);
    assert_eq!(n.children.as_ref().unwrap()[0].source, NodeSource::A11y);
    assert_eq!(serde_json::to_value(&n).unwrap(), v);
}

#[test]
fn enum_values_are_exact() {
    assert!(serde_json::from_value::<NodeSource>(json!("View")).is_err());
    assert!(serde_json::from_value::<NodeSource>(json!("unknown")).is_err());
    assert_eq!(NodeSource::Webview.as_str(), "webview");
}

#[test]
fn get_tree_params() {
    let p: GetTreeParams =
        serde_json::from_value(json!({ "sources": ["view", "layer"], "depth": 3 })).unwrap();
    assert_eq!(
        p.sources,
        [GetTreeParamsSources::View, GetTreeParamsSources::Layer]
    );
    assert_eq!(p.window_id, None);
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        json!({ "sources": ["view", "layer"], "depth": 3 })
    );
}

#[test]
fn error_codes_are_the_wire_names() {
    assert_eq!(
        serde_json::to_value(ErrorCode::FrameExpired).unwrap(),
        json!("FRAME_EXPIRED")
    );
    assert_eq!(
        serde_json::from_value::<ErrorCode>(json!("UNSUPPORTED_VERSION")).unwrap(),
        ErrorCode::UnsupportedVersion
    );
    assert_eq!(ErrorCode::PolicyDenied.as_str(), "POLICY_DENIED");
}

// ---------------------------------------------------------------------------
// envelope, tagged unions, nullable, binary, scope
// ---------------------------------------------------------------------------

use droiduse_contracts_adp::app::{InstallParams, InstallSource, UploadId};
use droiduse_contracts_adp::envelope::{Request, Response};
use droiduse_contracts_adp::screen::CaptureScreenshotReturns;
use droiduse_contracts_adp::ui::{NodeChanges, NodeId, TreeOp};
use droiduse_contracts_adp::{Command, Nullable, Scope};

#[test]
fn envelope_request_keeps_params_as_a_json_object() {
    let v = json!({ "id": 7, "sessionId": "s1", "method": "UI.getTree", "params": { "sources": ["view"], "depth": 1 } });
    let r: Request = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(r.method, GetTreeParams::METHOD);
    let p: GetTreeParams =
        serde_json::from_value(Value::Object(r.params.clone().unwrap())).unwrap();
    assert_eq!(p.depth, 1);
    assert_eq!(serde_json::to_value(&r).unwrap(), v);
    // params must be an object
    assert!(
        serde_json::from_value::<Request>(json!({ "id": 1, "method": "x", "params": [1] }))
            .is_err()
    );
}

#[test]
fn envelope_response_error() {
    let v = json!({ "id": 7, "error": { "code": "TIMEOUT", "message": "late" } });
    let r: Response = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(r.error.as_ref().unwrap().code, ErrorCode::Timeout.as_str());
    assert_eq!(serde_json::to_value(&r).unwrap(), v);
}

#[test]
fn command_traits_carry_method_and_scope() {
    assert_eq!(GetTreeParams::METHOD, "UI.getTree");
    assert_eq!(GetTreeParams::SCOPE, Scope::Session);
    assert_eq!(
        droiduse_contracts_adp::connection::AuthParams::SCOPE,
        Scope::Connection
    );
    assert_eq!(
        droiduse_contracts_adp::target::GetTargetsParams::SCOPE,
        Scope::Both
    );
}

#[test]
fn instance_lifecycle_commands() {
    use droiduse_contracts_adp::target::{
        CloseTargetParams, CloseTargetReturns, CreateTargetParams, CreateTargetReturns,
    };
    assert_eq!(CreateTargetParams::METHOD, "Target.createTarget");
    assert_eq!(CreateTargetParams::SCOPE, Scope::Connection);
    assert_eq!(CreateTargetParams::TARGET_TYPES, None);
    // no parameters yet: anything else is refused
    serde_json::from_value::<CreateTargetParams>(json!({})).unwrap();
    assert!(serde_json::from_value::<CreateTargetParams>(json!({ "image": "x" })).is_err());
    let r: CreateTargetReturns = serde_json::from_value(json!({ "targetId": "t1" })).unwrap();
    assert_eq!(r.target_id.0, "t1");
    assert_eq!(CloseTargetParams::METHOD, "Target.closeTarget");
    assert_eq!(CloseTargetParams::SCOPE, Scope::Connection);
    let p: CloseTargetParams = serde_json::from_value(json!({ "targetId": "t1" })).unwrap();
    assert_eq!(p.target_id.0, "t1");
    serde_json::from_value::<CloseTargetReturns>(json!({})).unwrap();
}

#[test]
fn tagged_union_by_kind() {
    let v = json!({ "source": { "kind": "upload", "uploadId": "u1" } });
    let p: InstallParams = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(
        p.source,
        InstallSource::Upload {
            upload_id: UploadId("u1".into())
        }
    );
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
    // unknown kind, missing tag, unknown field: errors
    for bad in [
        json!({ "kind": "url", "uploadId": "u1" }),
        json!({ "uploadId": "u1" }),
        json!({ "kind": "upload", "uploadId": "u1", "extra": 1 }),
    ] {
        assert!(
            serde_json::from_value::<InstallSource>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn tagged_union_by_op() {
    let v = json!([
        { "op": "remove", "nodeId": "n1" },
        { "op": "move", "nodeId": "n2", "parentId": "n0" },
        { "op": "update", "nodeId": "n3", "changes": { "text": null, "visible": true } }
    ]);
    let ops: Vec<TreeOp> = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(
        ops[0],
        TreeOp::Remove {
            node_id: NodeId("n1".into())
        }
    );
    assert_eq!(serde_json::to_value(&ops).unwrap(), v);
    assert!(serde_json::from_value::<TreeOp>(json!({ "op": "rename", "nodeId": "n1" })).is_err());
    assert!(serde_json::from_value::<TreeOp>(json!({ "kind": "remove", "nodeId": "n1" })).is_err());
}

#[test]
fn nullable_keeps_absent_null_and_value_apart() {
    let c: NodeChanges = serde_json::from_value(json!({ "text": null, "hint": "h" })).unwrap();
    assert_eq!(c.text, Nullable::Null);
    assert_eq!(c.hint, Nullable::Value("h".into()));
    assert_eq!(c.role, Nullable::Absent);
    assert_eq!(c.visible, None);
    assert_eq!(
        serde_json::to_value(&c).unwrap(),
        json!({ "text": null, "hint": "h" })
    );
    // all absent: an empty object both ways
    let empty: NodeChanges = serde_json::from_value(json!({})).unwrap();
    assert_eq!(empty.text, Nullable::Absent);
    assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));
}

#[test]
fn binary_is_strict_base64() {
    let v =
        json!({ "frameId": { "display": 0, "vsyncId": "1", "sfFrame": "2" }, "data": "AAEC/w==" });
    let r: CaptureScreenshotReturns = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(r.data.as_deref(), Some(&[0u8, 1, 2, 255][..]));
    assert_eq!(serde_json::to_value(&r).unwrap(), v);
    // no padding, URL-safe alphabet, whitespace, non-canonical trailing bits
    for bad in ["AAEC/w", "AAEC_w==", "AAEC /w==", "AAEC/x=="] {
        let mut b = v.clone();
        b["data"] = json!(bad);
        assert!(
            serde_json::from_value::<CaptureScreenshotReturns>(b).is_err(),
            "{bad}"
        );
    }
}

/// The real protocol.json, for tests that must cover every command, variant or property.
fn protocol() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/adp/protocol.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The `types[]` entry `id` of `domain` in protocol.json.
fn idl_type(domain: &str, id: &str) -> Value {
    protocol()["domains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["domain"] == domain)
        .and_then(|d| {
            d["types"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == id)
        })
        .unwrap_or_else(|| panic!("{domain}.{id} in protocol.json"))
        .clone()
}

/// A wire value of an IDL property shape (`type`, `format`, `items`).
fn sample(shape: &Value) -> Value {
    match (shape["type"].as_str().unwrap(), shape["format"].as_str()) {
        ("string", Some("int64")) => json!("-9223372036854775808"),
        ("string", _) => json!("s"),
        ("integer", Some("int32")) => json!(-2_147_483_648),
        ("number", _) => json!(0.5),
        ("boolean", _) => json!(true),
        ("array", _) => json!([sample(&shape["items"]), sample(&shape["items"])]),
        other => panic!("no sample for {other:?}"),
    }
}

#[test]
fn target_types_on_the_real_commands() {
    use droiduse_contracts_adp::accessibility::PerformActionParams;
    use droiduse_contracts_adp::app::{
        CreateUploadParams, FinishUploadParams, LaunchParams, ListParams, UploadChunkParams,
    };
    use droiduse_contracts_adp::connection::{AuthParams, HelloParams};
    use droiduse_contracts_adp::input::{
        ClickParams, DispatchKeyParams, DispatchTouchParams, InsertTextParams, LongClickParams,
    };
    use droiduse_contracts_adp::screen::{CaptureScreenshotParams, GetDisplaysParams};
    use droiduse_contracts_adp::target::{
        AttachToTargetParams, CloseTargetParams, CreateTargetParams, DetachFromTargetParams,
        GetTargetsParams, TargetType as T,
    };
    use droiduse_contracts_adp::ui::{GetNodeParams, QuerySelectorAllParams, QuerySelectorParams};

    /// A command's method and generated target types.
    type Row = (&'static str, Option<&'static [T]>);
    fn row<C: Command>() -> Row {
        (C::METHOD, C::TARGET_TYPES)
    }
    let all = &[T::Instance, T::Display, T::Window, T::Process, T::Webview][..];
    let display = &[T::Display][..];
    let instance = &[T::Instance][..];
    let want: &[(Row, Option<&[T]>)] = &[
        // connection-scoped: no target
        (row::<AuthParams>(), None),
        (row::<HelloParams>(), None),
        (row::<CreateTargetParams>(), None),
        (row::<CloseTargetParams>(), None),
        // Target.*: any target type
        (row::<GetTargetsParams>(), Some(all)),
        (row::<AttachToTargetParams>(), Some(all)),
        (row::<DetachFromTargetParams>(), Some(all)),
        // the display tree and everything acting on it
        (row::<GetTreeParams>(), Some(display)),
        (row::<QuerySelectorParams>(), Some(display)),
        (row::<QuerySelectorAllParams>(), Some(display)),
        (row::<GetNodeParams>(), Some(display)),
        (row::<PerformActionParams>(), Some(display)),
        (row::<ClickParams>(), Some(display)),
        (row::<LongClickParams>(), Some(display)),
        (row::<DispatchTouchParams>(), Some(display)),
        (row::<InsertTextParams>(), Some(display)),
        (row::<DispatchKeyParams>(), Some(display)),
        (row::<CaptureScreenshotParams>(), Some(display)),
        // the instance as a whole
        (row::<GetDisplaysParams>(), Some(instance)),
        (row::<CreateUploadParams>(), Some(instance)),
        (row::<UploadChunkParams>(), Some(instance)),
        (row::<FinishUploadParams>(), Some(instance)),
        (row::<InstallParams>(), Some(instance)),
        (row::<LaunchParams>(), Some(instance)),
        (row::<ListParams>(), Some(instance)),
    ];
    for ((method, got), expected) in want {
        assert_eq!(got, expected, "{method}");
    }
    // The table covers every command of protocol.json, so a new command needs a row here.
    let mut idl: Vec<String> = Vec::new();
    for d in protocol()["domains"].as_array().unwrap() {
        for c in d["commands"].as_array().into_iter().flatten() {
            idl.push(format!(
                "{}.{}",
                d["domain"].as_str().unwrap(),
                c["name"].as_str().unwrap()
            ));
        }
    }
    let mut listed: Vec<String> = want.iter().map(|((m, _), _)| m.to_string()).collect();
    idl.sort();
    listed.sort();
    assert_eq!(listed, idl);
}

// ---------------------------------------------------------------------------
// App.IntentExtra (discriminator `type`, capitalized kinds)
// ---------------------------------------------------------------------------

#[test]
fn intent_extra_variants_have_the_exact_wire_kinds() {
    use droiduse_contracts_adp::app::IntentExtra as X;
    let k = || "k".to_string();
    let cases = [
        (
            X::String {
                key: k(),
                value: "v".into(),
            },
            "String",
            json!("v"),
        ),
        (
            X::Int {
                key: k(),
                value: i32::MIN,
            },
            "Int",
            json!(i32::MIN),
        ),
        (
            X::Long {
                key: k(),
                value: i64::MIN,
            },
            "Long",
            json!("-9223372036854775808"),
        ),
        (
            X::Float {
                key: k(),
                value: 0.5,
            },
            "Float",
            json!(0.5),
        ),
        (
            X::Double {
                key: k(),
                value: 0.25,
            },
            "Double",
            json!(0.25),
        ),
        (
            X::Boolean {
                key: k(),
                value: true,
            },
            "Boolean",
            json!(true),
        ),
        (
            X::IntArray {
                key: k(),
                value: vec![1, -1],
            },
            "IntArray",
            json!([1, -1]),
        ),
        (
            X::LongArray {
                key: k(),
                value: vec![i64::MAX],
            },
            "LongArray",
            json!(["9223372036854775807"]),
        ),
        (
            X::FloatArray {
                key: k(),
                value: vec![0.5],
            },
            "FloatArray",
            json!([0.5]),
        ),
        (
            X::DoubleArray {
                key: k(),
                value: vec![0.5],
            },
            "DoubleArray",
            json!([0.5]),
        ),
        (
            X::StringArray {
                key: k(),
                value: vec!["a".into()],
            },
            "StringArray",
            json!(["a"]),
        ),
        (
            X::IntegerArrayList {
                key: k(),
                value: vec![3],
            },
            "IntegerArrayList",
            json!([3]),
        ),
        (
            X::StringArrayList {
                key: k(),
                value: vec!["a".into()],
            },
            "StringArrayList",
            json!(["a"]),
        ),
        (
            X::Uri {
                key: k(),
                value: "content://a/b".into(),
            },
            "Uri",
            json!("content://a/b"),
        ),
        (
            X::ComponentName {
                key: k(),
                value: "p/.A".into(),
            },
            "ComponentName",
            json!("p/.A"),
        ),
    ];
    for (rust, kind, value) in &cases {
        let wire = json!({ "type": kind, "key": "k", "value": value });
        assert_eq!(&serde_json::to_value(rust).unwrap(), &wire, "{kind}");
        assert_eq!(&serde_json::from_value::<X>(wire).unwrap(), rust, "{kind}");
    }
    // Every variant of protocol.json is covered above, in IDL order.
    let idl = idl_type("App", "IntentExtra");
    assert_eq!(idl["discriminator"], "type");
    let kinds: Vec<&str> = idl["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, cases.iter().map(|c| c.1).collect::<Vec<_>>());
}

#[test]
fn intent_extra_round_trips_every_idl_variant() {
    use droiduse_contracts_adp::app::IntentExtra;
    for v in idl_type("App", "IntentExtra")["variants"]
        .as_array()
        .unwrap()
    {
        let mut wire = serde_json::Map::new();
        wire.insert("type".into(), v["kind"].clone());
        for p in v["properties"].as_array().unwrap() {
            wire.insert(p["name"].as_str().unwrap().into(), sample(p));
        }
        let wire = Value::Object(wire);
        let x: IntentExtra = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&x).unwrap(), wire);
    }
}

#[test]
fn intent_extra_rejects_wrong_shapes() {
    use droiduse_contracts_adp::app::IntentExtra;
    for bad in [
        // kinds are case-sensitive; the tag is `type`, not `kind`
        json!({ "type": "string", "key": "k", "value": "v" }),
        json!({ "kind": "String", "key": "k", "value": "v" }),
        json!({ "key": "k", "value": "v" }),
        json!({ "type": "Char", "key": "k", "value": "v" }),
        // 64-bit as canonical decimal strings only; int32 as numbers only
        json!({ "type": "Long", "key": "k", "value": 1 }),
        json!({ "type": "Long", "key": "k", "value": "01" }),
        json!({ "type": "LongArray", "key": "k", "value": [1] }),
        json!({ "type": "Int", "key": "k", "value": "1" }),
        json!({ "type": "Int", "key": "k", "value": 2_147_483_648u32 }),
        json!({ "type": "Int", "key": "k", "value": 1.5 }),
        // missing or unknown fields
        json!({ "type": "Boolean", "key": "k" }),
        json!({ "type": "Boolean", "key": "k", "value": true, "extra": 1 }),
    ] {
        assert!(
            serde_json::from_value::<IntentExtra>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn launch_intent_extras_round_trip() {
    use droiduse_contracts_adp::app::LaunchParams;
    let v = json!({
        "packageName": "p",
        "intent": {
            "action": "android.intent.action.VIEW",
            "type": "text/plain",
            "flags": ["FLAG_ACTIVITY_NEW_TASK"],
            "extras": [
                { "type": "String", "key": "a", "value": "b" },
                { "type": "Long", "key": "n", "value": "42" }
            ]
        }
    });
    let p: LaunchParams = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(p.intent.as_ref().unwrap().extras.as_ref().unwrap().len(), 2);
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
}

// ---------------------------------------------------------------------------
// Accessibility: ActionRef, ActionArguments (UPPER_SNAKE keys), UI.NodeAction
// ---------------------------------------------------------------------------

#[test]
fn action_arguments_keys_are_the_android_constant_names() {
    use droiduse_contracts_adp::accessibility::ActionArguments;
    let idl = idl_type("Accessibility", "ActionArguments");
    let props = idl["properties"].as_array().unwrap();
    // every property of protocol.json, at once
    let mut wire = serde_json::Map::new();
    for p in props {
        let name = p["name"].as_str().unwrap();
        assert!(name.starts_with("ACTION_ARGUMENT_"), "{name}");
        wire.insert(name.into(), sample(p));
    }
    let wire = Value::Object(wire);
    let a: ActionArguments = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&a).unwrap(), wire);
    // the Rust names are the snake_case of the constants
    assert_eq!(a.action_argument_row_int, Some(i32::MIN));
    assert_eq!(
        a.action_argument_set_text_charsequence.as_deref(),
        Some("s")
    );
    assert_eq!(a.action_argument_extend_selection_boolean, Some(true));
    assert_eq!(a.action_argument_progress_value, Some(0.5));
    assert_eq!(a.action_argument_scroll_amount_float, Some(0.5));
    assert_eq!(
        a.action_argument_press_and_hold_duration_millis_int,
        Some(i32::MIN)
    );
    // one key alone, the rest absent and omitted
    let one = json!({ "ACTION_ARGUMENT_SELECTION_END_INT": 3 });
    let a: ActionArguments = serde_json::from_value(one.clone()).unwrap();
    assert_eq!(a.action_argument_selection_end_int, Some(3));
    assert_eq!(serde_json::to_value(&a).unwrap(), one);
    // only the exact constant names: camelCase, snake_case and lower-case spellings are unknown
    for bad in [
        "actionArgumentSelectionEndInt",
        "action_argument_selection_end_int",
        "ACTION_ARGUMENT_SELECTION_END",
        "ACTION_ARGUMENT_SELECTION_PARCELABLE",
    ] {
        assert!(
            serde_json::from_value::<ActionArguments>(json!({ bad: 3 })).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn perform_action_params_cross_domain_refs() {
    use droiduse_contracts_adp::accessibility::{ActionRef, PerformActionParams};
    use droiduse_contracts_adp::input::ResolveOn;
    let v = json!({
        "nodeId": "n1",
        "action": { "kind": "standard", "name": "ACTION_SET_TEXT" },
        "arguments": { "ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE": "hi" },
        "resolveOn": "idle",
        "timeoutMs": 500
    });
    let p: PerformActionParams = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(p.node_id, NodeId("n1".into()));
    assert_eq!(
        p.action,
        ActionRef::Standard {
            name: "ACTION_SET_TEXT".into()
        }
    );
    assert_eq!(p.resolve_on, Some(ResolveOn::Idle));
    assert_eq!(p.timeout_ms, Some(500));
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
}

#[test]
fn action_ref_kinds() {
    use droiduse_contracts_adp::accessibility::ActionRef;
    for (rust, wire) in [
        (
            ActionRef::Standard {
                name: "ACTION_CLICK".into(),
            },
            json!({ "kind": "standard", "name": "ACTION_CLICK" }),
        ),
        (
            ActionRef::Custom { id: -7 },
            json!({ "kind": "custom", "id": -7 }),
        ),
    ] {
        assert_eq!(serde_json::to_value(&rust).unwrap(), wire);
        assert_eq!(serde_json::from_value::<ActionRef>(wire).unwrap(), rust);
    }
    for bad in [
        json!({ "kind": "Standard", "name": "ACTION_CLICK" }),
        json!({ "kind": "custom", "id": "7" }),
        json!({ "kind": "custom", "id": 7, "label": "x" }),
        json!({ "kind": "standard" }),
    ] {
        assert!(
            serde_json::from_value::<ActionRef>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn node_actions_on_a_node() {
    use droiduse_contracts_adp::ui::NodeAction;
    let v = json!([
        { "kind": "standard", "name": "ACTION_CLICK" },
        { "kind": "standard", "name": "ACTION_SCROLL_FORWARD", "label": "More" },
        { "kind": "custom", "id": 2_130_706_432, "label": "Archive" }
    ]);
    let a: Vec<NodeAction> = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(
        a,
        [
            NodeAction::Standard {
                name: "ACTION_CLICK".into(),
                label: None
            },
            NodeAction::Standard {
                name: "ACTION_SCROLL_FORWARD".into(),
                label: Some("More".into())
            },
            NodeAction::Custom {
                id: 2_130_706_432,
                label: "Archive".into()
            },
        ]
    );
    assert_eq!(serde_json::to_value(&a).unwrap(), v);
    // a custom action always has a label
    assert!(serde_json::from_value::<NodeAction>(json!({ "kind": "custom", "id": 1 })).is_err());
}

// ---------------------------------------------------------------------------
// Input: click method, conditionMet / timeoutMs, Android ACTION_* type names
// ---------------------------------------------------------------------------

#[test]
fn click_method_and_condition_met() {
    use droiduse_contracts_adp::input::{
        ClickMethod, ClickParams, ClickReturns, ClickReturnsMethod, LongClickReturns,
        LongClickReturnsMethod,
    };
    let p = json!({ "nodeId": "n1", "method": "auto", "resolveOn": "frameCommitted", "timeoutMs": 2000 });
    let params: ClickParams = serde_json::from_value(p.clone()).unwrap();
    assert_eq!(params.method, Some(ClickMethod::Auto));
    assert_eq!(serde_json::to_value(&params).unwrap(), p);

    // the result names the method actually used; it is never auto
    let r = json!({ "method": "touch", "frameId": frame(json!("5"), json!("9")), "conditionMet": false });
    let ret: ClickReturns = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(ret.method, ClickReturnsMethod::Touch);
    assert_eq!(ret.condition_met, Some(false));
    assert_eq!(serde_json::to_value(&ret).unwrap(), r);
    assert!(serde_json::from_value::<ClickReturns>(json!({ "method": "auto" })).is_err());
    assert!(serde_json::from_value::<ClickReturns>(json!({})).is_err());

    // resolveOn dispatched: no frameId, no conditionMet
    let d = json!({ "method": "accessibility" });
    let ret: LongClickReturns = serde_json::from_value(d.clone()).unwrap();
    assert_eq!(ret.method, LongClickReturnsMethod::Accessibility);
    assert_eq!((&ret.frame_id, ret.condition_met), (&None, None));
    assert_eq!(serde_json::to_value(&ret).unwrap(), d);

    // the replaced names are unknown fields now
    assert!(
        serde_json::from_value::<ClickParams>(json!({ "nodeId": "n1", "idleTimeoutMs": 1 }))
            .is_err()
    );
    assert!(
        serde_json::from_value::<ClickReturns>(json!({ "method": "touch", "idle": true })).is_err()
    );
}

#[test]
fn touch_and_key_types_are_android_action_names() {
    use droiduse_contracts_adp::input::{
        DispatchKeyParams, DispatchKeyParamsType, DispatchTouchParams, DispatchTouchParamsType,
        DispatchTouchReturns,
    };
    for (t, name) in [
        (DispatchTouchParamsType::ActionDown, "ACTION_DOWN"),
        (
            DispatchTouchParamsType::ActionPointerDown,
            "ACTION_POINTER_DOWN",
        ),
        (DispatchTouchParamsType::ActionMove, "ACTION_MOVE"),
        (
            DispatchTouchParamsType::ActionPointerUp,
            "ACTION_POINTER_UP",
        ),
        (DispatchTouchParamsType::ActionUp, "ACTION_UP"),
        (DispatchTouchParamsType::ActionCancel, "ACTION_CANCEL"),
    ] {
        assert_eq!(t.as_str(), name);
        assert_eq!(serde_json::to_value(t).unwrap(), json!(name));
    }
    let v = json!({
        "type": "ACTION_POINTER_DOWN",
        "pointers": [ { "id": 0, "x": 1.0, "y": 2.0 }, { "id": 1, "x": 3.5, "y": 4.5 } ],
        "actionPointerId": 1
    });
    let p: DispatchTouchParams = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(p.r#type, DispatchTouchParamsType::ActionPointerDown);
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
    for bad in ["down", "actionDown", "ACTION_HOVER_MOVE"] {
        assert!(
            serde_json::from_value::<DispatchTouchParams>(json!({ "type": bad, "pointers": [] }))
                .is_err(),
            "{bad}"
        );
    }
    let k = json!({ "type": "ACTION_UP", "keyCode": "KEYCODE_ENTER" });
    let p: DispatchKeyParams = serde_json::from_value(k.clone()).unwrap();
    assert_eq!(p.r#type, DispatchKeyParamsType::ActionUp);
    assert_eq!(serde_json::to_value(&p).unwrap(), k);
    let r = json!({ "frameId": frame(json!("1"), json!("2")), "conditionMet": true });
    let ret: DispatchTouchReturns = serde_json::from_value(r.clone()).unwrap();
    assert_eq!(ret.condition_met, Some(true));
    assert_eq!(serde_json::to_value(&ret).unwrap(), r);
}

#[test]
fn request_traceparent_is_an_optional_string() {
    let v = json!({ "id": 1, "method": "Connection.hello", "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01" });
    let r: Request = serde_json::from_value(v.clone()).unwrap();
    assert!(r.traceparent.is_some());
    assert_eq!(serde_json::to_value(&r).unwrap(), v);
}
