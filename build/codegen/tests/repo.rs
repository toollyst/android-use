//! The real repo: the committed adp output is up to date (the proto targets need the pinned
//! protoc and are checked by `cargo xtask codegen --check` in CI), the real config is valid,
//! and the hand-written gRPC metadata key constants match the proto headers and the W3C field
//! names of `contracts/common`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use codegen::{Mode, Options};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn real_adp_output_is_up_to_date() {
    let r = codegen::run(
        &repo_root(),
        &Options {
            mode: Mode::Check,
            only: Some("adp".into()),
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let lines: Vec<String> = r.violations.iter().map(|v| v.to_string()).collect();
    assert!(lines.is_empty(), "{lines:#?}");
}

#[test]
fn real_config_names_every_target() {
    let cfg = codegen::config::load(&repo_root()).unwrap();
    let names: Vec<&str> = cfg.targets().collect();
    assert_eq!(names, ["guest_link", "node", "adp"]);
}

/// The gRPC metadata keys a proto file's header comment lists: the first column of the
/// lines under `Metadata keys`, up to the next empty `//` line (`a, b   description`).
fn proto_header_keys(rel: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(repo_root().join(rel)).unwrap();
    let mut lines = text.lines().skip_while(|l| !l.contains("Metadata keys"));
    assert!(lines.next().is_some(), "{rel}: no metadata key section");
    let mut keys = BTreeSet::new();
    for l in lines.take_while(|l| l.trim_end() != "//") {
        let Some(rest) = l.strip_prefix("//   ") else {
            continue;
        };
        if rest.starts_with(' ') {
            continue; // continuation of a description
        }
        let column = rest.split("  ").next().unwrap();
        keys.extend(column.split(", ").map(|k| k.trim().to_string()));
    }
    keys
}

fn set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|k| k.to_string()).collect()
}

#[test]
fn metadata_key_constants_match_the_proto_headers() {
    use droiduse_contracts_guest_link as gl;
    use droiduse_contracts_node as node;
    assert_eq!(
        proto_header_keys("contracts/guest_link/proto/droiduse/guest_link/guest_link.proto"),
        set(&[
            gl::TRACEPARENT_METADATA_KEY,
            gl::TRACESTATE_METADATA_KEY,
            gl::SESSION_TOKEN_METADATA_KEY,
        ])
    );
    assert_eq!(
        proto_header_keys("contracts/node/proto/droiduse/node/node.proto"),
        set(&[
            node::TRACEPARENT_METADATA_KEY,
            node::TRACESTATE_METADATA_KEY
        ])
    );
    assert_eq!(gl::TRACEPARENT_METADATA_KEY, "traceparent");
    assert_eq!(gl::TRACESTATE_METADATA_KEY, "tracestate");
    // Each contract keeps its own constants; they stay equal to the W3C field names of
    // contracts/common.
    use droiduse_contracts_common as common;
    for key in [gl::TRACEPARENT_METADATA_KEY, node::TRACEPARENT_METADATA_KEY] {
        assert_eq!(key, common::TRACEPARENT);
    }
    for key in [gl::TRACESTATE_METADATA_KEY, node::TRACESTATE_METADATA_KEY] {
        assert_eq!(key, common::TRACESTATE);
    }
    assert_eq!(gl::SESSION_TOKEN_METADATA_KEY, "droiduse-session-token-bin");
    // gRPC: keys ending in -bin carry binary values
    assert!(gl::SESSION_TOKEN_METADATA_KEY.ends_with("-bin"));
}

/// `(variant, wire name)` of `contracts/common` `ErrorCode`: each variant's doc comment ends
/// with the wire name in backticks and parentheses.
fn common_error_codes() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(repo_root().join("contracts/common/src/lib.rs")).unwrap();
    let body = text
        .split("pub enum ErrorCode {")
        .nth(1)
        .and_then(|rest| rest.split("\n}").next())
        .expect("contracts/common: pub enum ErrorCode");
    let mut out = Vec::new();
    let mut wire = None;
    for line in body.lines().map(str::trim) {
        if let Some(doc) = line.strip_prefix("///") {
            if let Some(w) = doc
                .trim_end()
                .strip_suffix("`)")
                .and_then(|d| d.rsplit_once("(`"))
            {
                wire = Some(w.1.to_string());
            }
        } else if let Some(variant) = line.strip_suffix(',') {
            let w = wire
                .take()
                .unwrap_or_else(|| panic!("ErrorCode::{variant}: no (`WIRE`) doc"));
            out.push((variant.to_string(), w));
        }
    }
    out
}

#[test]
fn adp_error_codes_equal_the_common_error_codes() {
    use droiduse_contracts_adp::errors::ErrorCode;
    let def: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("contracts/adp/protocol.json")).unwrap(),
    )
    .unwrap();
    let adp: Vec<String> = def["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect();
    let common = common_error_codes();
    let common_wire: Vec<String> = common.iter().map(|(_, w)| w.clone()).collect();
    assert_eq!(
        adp, common_wire,
        "protocol.json errors[] vs contracts/common ErrorCode"
    );
    for (variant, wire) in &common {
        let generated: ErrorCode = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(format!("{generated:?}"), *variant);
    }
}
