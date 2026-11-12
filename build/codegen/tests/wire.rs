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
