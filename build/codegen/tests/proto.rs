//! The prost-build stage, fed a FileDescriptorSet directly (what the pinned protoc produces),
//! so it runs without protoc: messages only, the bytes/btree_map settings, determinism.

use codegen::proto::{self, ProtoSettings};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet, MessageOptions,
    MethodDescriptorProto, ServiceDescriptorProto,
};

fn field(
    name: &str,
    number: i32,
    ty: Type,
    label: Label,
    type_name: Option<&str>,
) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.into()),
        number: Some(number),
        r#type: Some(ty as i32),
        label: Some(label as i32),
        type_name: type_name.map(Into::into),
        json_name: None,
        ..Default::default()
    }
}

/// package droiduse.fx: message Token { bytes token = 1; map<string, string> labels = 2; }
/// plus a service, which must not produce any code.
fn fds() -> FileDescriptorSet {
    let entry = DescriptorProto {
        name: Some("LabelsEntry".into()),
        field: vec![
            field("key", 1, Type::String, Label::Optional, None),
            field("value", 2, Type::String, Label::Optional, None),
        ],
        options: Some(MessageOptions {
            map_entry: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };
    let token = DescriptorProto {
        name: Some("Token".into()),
        field: vec![
            field("token", 1, Type::Bytes, Label::Optional, None),
            field(
                "labels",
                2,
                Type::Message,
                Label::Repeated,
                Some(".droiduse.fx.Token.LabelsEntry"),
            ),
        ],
        nested_type: vec![entry],
        ..Default::default()
    };
    let empty = DescriptorProto {
        name: Some("Empty".into()),
        ..Default::default()
    };
    let service = ServiceDescriptorProto {
        name: Some("TokenService".into()),
        method: vec![MethodDescriptorProto {
            name: Some("Issue".into()),
            input_type: Some(".droiduse.fx.Empty".into()),
            output_type: Some(".droiduse.fx.Token".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("droiduse/fx/fx.proto".into()),
            package: Some("droiduse.fx".into()),
            message_type: vec![token, empty],
            service: vec![service],
            syntax: Some("proto3".into()),
            ..Default::default()
        }],
    }
}

fn settings(bytes: &[&str], btree_map: &[&str]) -> ProtoSettings {
    ProtoSettings {
        bytes: bytes.iter().map(|s| s.to_string()).collect(),
        btree_map: btree_map.iter().map(|s| s.to_string()).collect(),
    }
}

fn one(out: &std::collections::BTreeMap<String, String>) -> &str {
    assert_eq!(out.keys().collect::<Vec<_>>(), ["droiduse.fx.rs"]);
    &out["droiduse.fx.rs"]
}

#[test]
fn one_file_per_package() {
    let out = proto::generate_fds(fds(), &settings(&["."], &[])).unwrap();
    let text = one(&out);
    assert!(text.contains("pub struct Token"));
    assert!(text.contains("pub struct Empty"));
}

#[test]
fn messages_only_no_service_bindings() {
    let out = proto::generate_fds(fds(), &settings(&["."], &["."])).unwrap();
    let text = one(&out);
    assert!(!text.contains("TokenService"), "{text}");
    assert!(!text.contains("tonic"), "{text}");
    assert!(!text.contains("Issue"), "{text}");
}

#[test]
fn bytes_setting() {
    let text = proto::generate_fds(fds(), &settings(&["."], &[])).unwrap();
    assert!(one(&text).contains("::prost::bytes::Bytes"));
    let text = proto::generate_fds(fds(), &settings(&[], &[])).unwrap();
    assert!(!one(&text).contains("::prost::bytes::Bytes"));
    assert!(one(&text).contains("::prost::alloc::vec::Vec<u8>"));
}

#[test]
fn btree_map_setting() {
    let text = proto::generate_fds(fds(), &settings(&["."], &["."])).unwrap();
    assert!(one(&text).contains("BTreeMap"));
    let text = proto::generate_fds(fds(), &settings(&["."], &[])).unwrap();
    assert!(one(&text).contains("HashMap"));
}

#[test]
fn deterministic_and_lf_only() {
    let a = proto::generate_fds(fds(), &settings(&["."], &["."])).unwrap();
    let b = proto::generate_fds(fds(), &settings(&["."], &["."])).unwrap();
    assert_eq!(a, b);
    assert!(!one(&a).contains('\r'));
}

#[test]
fn every_proto_under_the_root_is_found_sorted() {
    let dir = tempfile::tempdir().unwrap();
    for f in ["b/z.proto", "a/y.proto", "a/x.proto", "a/notes.txt"] {
        let p = dir.path().join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }
    assert_eq!(
        proto::discover(dir.path()).unwrap(),
        ["a/x.proto", "a/y.proto", "b/z.proto"]
    );
}

#[test]
fn a_root_without_protos_cannot_run() {
    let dir = tempfile::tempdir().unwrap();
    assert!(proto::discover(dir.path()).is_err());
}
