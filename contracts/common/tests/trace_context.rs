//! W3C Trace Context corpus and property tests for the `contracts/common` trace types.
//!
//! Where the cases come from (the `source` of each case):
//! - `w3c:<test name>`: the W3C trace-context test suite, `test/test.py` of
//!   <https://github.com/w3c/trace-context> at commit `acab820be9db7b3433668baa5cdd43f57f4c4be0`
//!   (2026-06-29). The tests of that repository are distributed under the W3C 3-clause BSD
//!   License. The suite drives an HTTP service; only the header values and the outcome the suite
//!   expects are taken here, restated as the result of parsing that value.
//! - `spec:<section>`: an example or rule of W3C Trace Context Level 1 (W3C Recommendation,
//!   23 November 2021, <https://www.w3.org/TR/trace-context/>); `spec-l2:<section>`: of Level 2
//!   (W3C Candidate Recommendation Draft, 28 March 2024, <https://www.w3.org/TR/trace-context-2/>),
//!   whose `tracestate` key grammar is accepted on input.
//! - `probe`: malformed values that `opentelemetry_sdk` 0.33.1 `TraceContextPropagator` accepted
//!   in a probe run during the design of this type (sign characters, wrong lengths, whitespace,
//!   a trailing dash on version 00, duplicate keys, more than 32 members, uppercase).
//! - `local`: further boundary cases.
//!
//! The suite at that commit expects the Level 2 key grammar for `tracestate` (keys with `@`
//! anywhere, up to 256 characters); those cases are tagged `(level2 key)`.

use droiduse_contracts_common::{
    SpanId, TRACESTATE_MAX_MEMBERS, TraceContext, TraceContextError as E, TraceFlags, TraceId,
    TraceParent, TraceState,
};

const T: &str = "12345678901234567890123456789012";
const P: &str = "1234567890123456";
const T2: &str = "0af7651916cd43dd8448eb211c80319c";
const P2: &str = "b7ad6b7169203331";

// ---------------------------------------------------------------------------
// traceparent corpus
// ---------------------------------------------------------------------------

/// Expected result of `TraceParent::parse`: the trace-id, parent-id and flags byte, or the error.
type TpWant = Result<(&'static str, &'static str, u8), E>;

fn traceparent_cases() -> Vec<(&'static str, String, TpWant)> {
    let ok = |t: &'static str, p: &'static str, f: u8| Ok((t, p, f));
    vec![
        // Valid.
        (
            "w3c:test_traceparent_included_tracestate_missing",
            format!("00-{T}-{P}-01"),
            ok(T, P, 1),
        ),
        (
            "w3c:test_tracestate_included_traceparent_included",
            format!("00-{T}-{P}-00"),
            ok(T, P, 0),
        ),
        (
            "spec:3.2.3",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".into(),
            ok("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7", 1),
        ),
        (
            "spec:3.2.3",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00".into(),
            ok("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7", 0),
        ),
        // Version 00: nothing may follow the flags.
        (
            "w3c:test_traceparent_version_0x00",
            format!("00-{T}-{P}-01."),
            Err(E::MalformedTraceParent),
        ),
        (
            "w3c:test_traceparent_version_0x00",
            format!("00-{T}-{P}-01-what-the-future-will-be-like"),
            Err(E::MalformedTraceParent),
        ),
        (
            "probe",
            format!("00-{T2}-{P2}-01-"),
            Err(E::MalformedTraceParent),
        ),
        // Higher versions (§3.2.4).
        (
            "w3c:test_traceparent_version_0xcc",
            format!("cc-{T}-{P}-01"),
            ok(T, P, 1),
        ),
        (
            "w3c:test_traceparent_version_0xcc",
            format!("cc-{T}-{P}-01-what-the-future-will-be-like"),
            ok(T, P, 1),
        ),
        (
            "w3c:test_traceparent_version_0xcc",
            format!("cc-{T}-{P}-01.what-the-future-will-be-like"),
            Err(E::MalformedTraceParent),
        ),
        (
            "probe",
            format!("cc-{T2}-{P2}-01extra"),
            Err(E::MalformedTraceParent),
        ),
        ("local", format!("cc-{T}-{P}-01-"), ok(T, P, 1)),
        (
            "local",
            format!("cc-{T}-{P}-0"),
            Err(E::MalformedTraceParent),
        ),
        ("local", format!("cc-{T}-{P}-ff"), ok(T, P, 1)),
        ("local", format!("01-{T}-{P}-01-\u{e9}"), ok(T, P, 1)),
        (
            "local",
            format!("01-{T}-{P}-01\u{e9}"),
            Err(E::MalformedTraceParent),
        ),
        // Version field.
        (
            "w3c:test_traceparent_version_0xff",
            format!("ff-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        (
            "w3c:test_traceparent_version_illegal_characters",
            format!(".0-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        (
            "w3c:test_traceparent_version_illegal_characters",
            format!("0.-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        (
            "w3c:test_traceparent_version_too_long",
            format!("000-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        (
            "w3c:test_traceparent_version_too_long",
            format!("0000-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        (
            "w3c:test_traceparent_version_too_short",
            format!("0-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        ("probe", format!("0-{T2}-{P2}-1"), Err(E::InvalidVersion)),
        ("probe", format!("+0-{T2}-{P2}-01"), Err(E::InvalidVersion)),
        ("probe", format!("FF-{T}-{P}-01"), Err(E::InvalidVersion)),
        ("probe", format!("CC-{T}-{P}-01"), Err(E::InvalidVersion)),
        ("local", String::new(), Err(E::InvalidVersion)),
        ("local", " \t ".into(), Err(E::InvalidVersion)),
        ("local", "00".into(), Err(E::InvalidVersion)),
        ("local", "00-".into(), Err(E::MalformedTraceParent)),
        ("local", "\u{e9}0-".into(), Err(E::InvalidVersion)),
        // trace-id.
        (
            "w3c:test_traceparent_trace_id_all_zero",
            format!("00-00000000000000000000000000000000-{P}-01"),
            Err(E::InvalidTraceId),
        ),
        (
            "w3c:test_traceparent_trace_id_illegal_characters",
            format!("00-.2345678901234567890123456789012-{P}-01"),
            Err(E::InvalidTraceId),
        ),
        (
            "w3c:test_traceparent_trace_id_illegal_characters",
            format!("00-1234567890123456789012345678901.-{P}-01"),
            Err(E::InvalidTraceId),
        ),
        (
            "w3c:test_traceparent_trace_id_too_long",
            format!("00-123456789012345678901234567890123-{P}-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "w3c:test_traceparent_trace_id_too_short",
            format!("00-1234567890123456789012345678901-{P}-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "probe",
            format!("00-+af7651916cd43dd8448eb211c80319c-{P2}-01"),
            Err(E::InvalidTraceId),
        ),
        (
            "probe",
            format!("00-af7651916cd43dd8448eb211c80319c-{P2}-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "probe",
            format!("00-1-{P2}-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "probe",
            format!("00-0AF7651916CD43DD8448EB211C80319C-{P2}-01"),
            Err(E::InvalidTraceId),
        ),
        (
            "local",
            format!("00-0af7651916cd43dd8448eb211c80319\u{e9}-{P2}-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "local",
            format!("00-0af7651916cd43dd8448eb211c8031\u{e9}-{P2}-01"),
            Err(E::InvalidTraceId),
        ),
        // parent-id.
        (
            "w3c:test_traceparent_parent_id_all_zero",
            format!("00-{T}-0000000000000000-01"),
            Err(E::InvalidParentId),
        ),
        (
            "w3c:test_traceparent_parent_id_illegal_characters",
            format!("00-{T}-.234567890123456-01"),
            Err(E::InvalidParentId),
        ),
        (
            "w3c:test_traceparent_parent_id_illegal_characters",
            format!("00-{T}-123456789012345.-01"),
            Err(E::InvalidParentId),
        ),
        (
            "w3c:test_traceparent_parent_id_too_long",
            format!("00-{T}-12345678901234567-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "w3c:test_traceparent_parent_id_too_short",
            format!("00-{T}-123456789012345-01"),
            Err(E::MalformedTraceParent),
        ),
        (
            "probe",
            format!("00-{T2}-+7ad6b7169203331-01"),
            Err(E::InvalidParentId),
        ),
        (
            "probe",
            format!("00-{T2}-B7AD6B7169203331-01"),
            Err(E::InvalidParentId),
        ),
        // trace-flags.
        (
            "w3c:test_traceparent_trace_flags_illegal_characters",
            format!("00-{T}-{P}-.0"),
            Err(E::InvalidTraceFlags),
        ),
        (
            "w3c:test_traceparent_trace_flags_illegal_characters",
            format!("00-{T}-{P}-0."),
            Err(E::InvalidTraceFlags),
        ),
        (
            "w3c:test_traceparent_trace_flags_too_long",
            format!("00-{T}-{P}-001"),
            Err(E::MalformedTraceParent),
        ),
        (
            "w3c:test_traceparent_trace_flags_too_short",
            format!("00-{T}-{P}-1"),
            Err(E::MalformedTraceParent),
        ),
        ("probe", format!("00-{T}-{P}-0A"), Err(E::InvalidTraceFlags)),
        ("probe", format!("00-{T}-{P}-+1"), Err(E::InvalidTraceFlags)),
        // Flags this version does not define are dropped (§3.2.2.5.2), including the Level 2
        // random flag.
        ("probe", format!("00-{T2}-{P2}-03"), ok(T2, P2, 1)),
        ("probe", format!("00-{T2}-{P2}-ff"), ok(T2, P2, 1)),
        (
            "w3c:test_propagates_random_flag (Level 2 suite; level1: flag dropped)",
            format!("00-{T}-{P}-02"),
            ok(T, P, 0),
        ),
        // Optional whitespace around the value.
        (
            "w3c:test_traceparent_ows_handling",
            format!(" 00-{T}-{P}-01"),
            ok(T, P, 1),
        ),
        (
            "w3c:test_traceparent_ows_handling",
            format!("\t00-{T}-{P}-01"),
            ok(T, P, 1),
        ),
        (
            "w3c:test_traceparent_ows_handling",
            format!("00-{T}-{P}-01 "),
            ok(T, P, 1),
        ),
        (
            "w3c:test_traceparent_ows_handling",
            format!("00-{T}-{P}-01\t"),
            ok(T, P, 1),
        ),
        (
            "w3c:test_traceparent_ows_handling",
            format!("\t 00-{T}-{P}-01 \t"),
            ok(T, P, 1),
        ),
        ("probe", format!(" 00-{T2}-{P2}-01 "), ok(T2, P2, 1)),
        // Only space and tab are optional whitespace.
        (
            "local",
            format!("00-{T}-{P}-01\n"),
            Err(E::MalformedTraceParent),
        ),
        ("local", format!("\n00-{T}-{P}-01"), Err(E::InvalidVersion)),
        (
            "local",
            format!("\u{a0}00-{T}-{P}-01"),
            Err(E::InvalidVersion),
        ),
        (
            "local",
            format!("00-{T}-{P}-01\0"),
            Err(E::MalformedTraceParent),
        ),
        // Inner whitespace.
        ("local", format!("00 -{T}-{P}-01"), Err(E::InvalidVersion)),
        (
            "local",
            format!("00-{T} -{P}-01"),
            Err(E::MalformedTraceParent),
        ),
    ]
}

#[test]
fn traceparent_corpus() {
    for (source, input, want) in traceparent_cases() {
        let got = TraceParent::parse(&input).map(|tp| {
            (
                tp.trace_id().to_string(),
                tp.parent_id().to_string(),
                tp.flags().bits(),
            )
        });
        let want = want.map(|(t, p, f)| (t.to_string(), p.to_string(), f));
        assert_eq!(got, want, "{source}: {input:?}");
    }
}

#[test]
fn traceparent_is_written_as_version_00_with_defined_flags_only() {
    let tp = TraceParent::parse(&format!("cc-{T}-{P}-ff-future")).unwrap();
    assert_eq!(tp.to_string(), format!("00-{T}-{P}-01"));
    let tp = TraceParent::parse(&format!("00-{T}-{P}-02")).unwrap();
    assert_eq!(tp.to_string(), format!("00-{T}-{P}-00"));
    let s = format!("00-{T2}-{P2}-01");
    assert_eq!(s.parse::<TraceParent>().unwrap().to_string(), s);
}

#[test]
fn traceparent_mutation_for_an_outgoing_call() {
    let tp = TraceParent::parse(&format!("00-{T}-{P}-00")).unwrap();
    let span = SpanId::from_hex(P2).unwrap();
    let out = tp
        .with_parent_id(span)
        .with_flags(tp.flags().with_sampled(true));
    assert_eq!(out.to_string(), format!("00-{T}-{P2}-01"));
    assert_eq!(out.trace_id(), tp.trace_id());
}

#[test]
fn ids_and_flags() {
    assert_eq!(TraceId::from_bytes([0; 16]), None);
    assert_eq!(SpanId::from_bytes([0; 8]), None);
    let mut b = [0u8; 16];
    b[15] = 1;
    let id = TraceId::from_bytes(b).unwrap();
    assert_eq!(id.to_string(), "00000000000000000000000000000001");
    assert_eq!(id.to_bytes(), b);
    assert_eq!(TraceId::from_hex(&id.to_string()), Ok(id));
    assert_eq!(
        format!("{id:?}"),
        "TraceId(00000000000000000000000000000001)"
    );
    assert_eq!(TraceId::from_hex(T2).unwrap().to_string(), T2);
    assert_eq!(T2.parse::<TraceId>().unwrap().to_string(), T2);
    assert_eq!(SpanId::from_hex(P2).unwrap().to_string(), P2);
    assert_eq!(P2.parse::<SpanId>().unwrap().to_string(), P2);
    for bad in [
        "",
        "0",
        "+af7651916cd43dd8448eb211c80319c",
        "0AF7651916CD43DD8448EB211C80319C",
        " 0af7651916cd43dd8448eb211c80319c",
    ] {
        assert_eq!(TraceId::from_hex(bad), Err(E::InvalidTraceId), "{bad:?}");
    }
    for bad in [
        "",
        "0000000000000000",
        "+7ad6b7169203331",
        "b7ad6b716920333",
        "b7ad6b71692033311",
    ] {
        assert_eq!(SpanId::from_hex(bad), Err(E::InvalidParentId), "{bad:?}");
    }
    assert_eq!(TraceFlags::default(), TraceFlags::NONE);
    assert!(TraceFlags::SAMPLED.is_sampled());
    assert!(!TraceFlags::NONE.is_sampled());
    assert_eq!(TraceFlags::NONE.with_sampled(true), TraceFlags::SAMPLED);
    assert_eq!(TraceFlags::SAMPLED.with_sampled(false), TraceFlags::NONE);
    assert_eq!(TraceFlags::from_bits_truncate(0xff), TraceFlags::SAMPLED);
    assert_eq!(TraceFlags::from_bits_truncate(0x02), TraceFlags::NONE);
}

// ---------------------------------------------------------------------------
// tracestate corpus
// ---------------------------------------------------------------------------

/// `count` members `bar01=01` ... in 10-member fields, as the W3C member-count test sends them.
fn bar_members(count: usize) -> Vec<String> {
    let all: Vec<String> = (1..=count).map(|i| format!("bar{i:02}={i:02}")).collect();
    all.chunks(10).map(|c| c.join(",")).collect()
}

/// Every character the Level 1 grammar allows in a value (§3.3.1.3.2), as the W3C suite builds
/// it.
fn all_value_chars() -> String {
    (0x20u8..=0x2b)
        .chain(0x2d..=0x3c)
        .chain(0x3e..=0x7e)
        .map(char::from)
        .collect()
}

/// Every character a simple key allows, as the W3C suite builds it.
const ALL_KEY_CHARS: &str = "abcdefghijklmnopqrstuvwxyz0123456789_-*/";

fn tracestate_cases() -> Vec<(&'static str, Vec<String>, Result<String, E>)> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let ok = |h: &str| Ok(h.to_string());
    let value = all_value_chars();
    let key_at = format!("{ALL_KEY_CHARS}@a-z0-9_-*/");
    let z256 = "z".repeat(256);
    let at_max = format!("{}@{}", "t".repeat(241), "v".repeat(14));
    vec![
        (
            "w3c:test_tracestate_included_traceparent_included",
            s(&["foo=1,bar=2"]),
            ok("foo=1,bar=2"),
        ),
        (
            "w3c:test_tracestate_header_name_valid_casing",
            s(&["foo=1"]),
            ok("foo=1"),
        ),
        ("w3c:test_tracestate_empty_header", s(&[""]), ok("")),
        (
            "w3c:test_tracestate_empty_header",
            s(&["foo=1", ""]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_empty_header",
            s(&["", "foo=1"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_multiple_headers_different_keys",
            s(&["foo=1,bar=2", "rojo=1,congo=2", "baz=3"]),
            ok("foo=1,bar=2,rojo=1,congo=2,baz=3"),
        ),
        // Duplicate keys: the suite accepts either value; the left-most is kept.
        (
            "w3c:test_tracestate_duplicated_keys",
            s(&["foo=1,foo=1"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_duplicated_keys",
            s(&["foo=1,foo=2"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_duplicated_keys",
            s(&["foo=1", "foo=1"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_duplicated_keys",
            s(&["foo=1", "foo=2"]),
            ok("foo=1"),
        ),
        ("probe", s(&["a=1,a=2"]), ok("a=1")),
        (
            "w3c:test_tracestate_all_allowed_characters",
            vec![format!("{ALL_KEY_CHARS}={value}")],
            Ok(format!("{ALL_KEY_CHARS}={value}")),
        ),
        (
            "w3c:test_tracestate_all_allowed_characters",
            vec![format!("{key_at}={value}")],
            Ok(format!("{key_at}={value}")),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&["foo=1 \t , \t bar=2, \t baz=3"]),
            ok("foo=1,bar=2,baz=3"),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&["foo=1\t \t,\t \tbar=2,\t \tbaz=3"]),
            ok("foo=1,bar=2,baz=3"),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&[" foo=1"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&["\tfoo=1"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&["foo=1 "]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&["foo=1\t"]),
            ok("foo=1"),
        ),
        (
            "w3c:test_tracestate_ows_handling",
            s(&["\t foo=1 \t"]),
            ok("foo=1"),
        ),
        ("probe", s(&["a=1,,b=2"]), ok("a=1,b=2")),
        ("probe", s(&["a=1 , b=2"]), ok("a=1,b=2")),
        ("probe", s(&["a=x y"]), ok("a=x y")),
        (
            "w3c:test_tracestate_key_illegal_characters",
            s(&["foo =1"]),
            Err(E::InvalidTraceStateKey),
        ),
        (
            "w3c:test_tracestate_key_illegal_characters",
            s(&["FOO=1"]),
            Err(E::InvalidTraceStateKey),
        ),
        (
            "w3c:test_tracestate_key_illegal_characters",
            s(&["foo.bar=1"]),
            Err(E::InvalidTraceStateKey),
        ),
        ("probe", s(&["Upper=1"]), Err(E::InvalidTraceStateKey)),
        (
            "w3c:test_tracestate_key_illegal_vendor_format",
            s(&["@foo=1,bar=2"]),
            Err(E::InvalidTraceStateKey),
        ),
        (
            "w3c:test_tracestate_key_illegal_vendor_format (level2 key)",
            s(&["foo@=1,bar=2"]),
            ok("foo@=1,bar=2"),
        ),
        (
            "w3c:test_tracestate_key_illegal_vendor_format (level2 key)",
            s(&["foo@@bar=1,bar=2"]),
            ok("foo@@bar=1,bar=2"),
        ),
        (
            "w3c:test_tracestate_key_illegal_vendor_format (level2 key)",
            s(&["foo@bar@baz=1,bar=2"]),
            ok("foo@bar@baz=1,bar=2"),
        ),
        (
            "w3c:test_tracestate_member_count_limit",
            bar_members(32),
            Ok(bar_members(32).join(",")),
        ),
        (
            "w3c:test_tracestate_member_count_limit",
            bar_members(33),
            Err(E::TooManyTraceStateMembers),
        ),
        (
            "probe",
            vec![
                (0..40)
                    .map(|i| format!("k{i}=v"))
                    .collect::<Vec<_>>()
                    .join(","),
            ],
            Err(E::TooManyTraceStateMembers),
        ),
        (
            "w3c:test_tracestate_key_length_limit",
            vec!["foo=1".into(), format!("{z256}=1")],
            Ok(format!("foo=1,{z256}=1")),
        ),
        (
            "w3c:test_tracestate_key_length_limit",
            vec!["foo=1".into(), format!("{}=1", "z".repeat(257))],
            Err(E::InvalidTraceStateKey),
        ),
        (
            "w3c:test_tracestate_key_length_limit",
            vec!["foo=1".into(), format!("{at_max}=1")],
            Ok(format!("foo=1,{at_max}=1")),
        ),
        (
            "w3c:test_tracestate_key_length_limit (level2 key)",
            vec!["foo=1".into(), format!("{}@v=1", "t".repeat(242))],
            Ok(format!("foo=1,{}@v=1", "t".repeat(242))),
        ),
        (
            "w3c:test_tracestate_key_length_limit (level2 key)",
            vec!["foo=1".into(), format!("t@{}=1", "v".repeat(15))],
            Ok(format!("foo=1,t@{}=1", "v".repeat(15))),
        ),
        (
            "spec-l2:3.3.2.2.1",
            vec![format!("{}@{}=1", "t".repeat(200), "v".repeat(55))],
            Ok(format!("{}@{}=1", "t".repeat(200), "v".repeat(55))),
        ),
        (
            "spec-l2:3.3.2.2.1",
            vec![format!("{}@{}=1", "t".repeat(200), "v".repeat(56))],
            Err(E::InvalidTraceStateKey),
        ),
        (
            "w3c:test_tracestate_value_illegal_characters",
            s(&["foo=bar=baz"]),
            Err(E::InvalidTraceStateValue),
        ),
        (
            "w3c:test_tracestate_value_illegal_characters",
            s(&["foo=,bar=3"]),
            Err(E::InvalidTraceStateValue),
        ),
        (
            "spec:3.3.2",
            s(&["rojo=00f067aa0ba902b7"]),
            ok("rojo=00f067aa0ba902b7"),
        ),
        (
            "spec:3.3.2",
            s(&["rojo=00f067aa0ba902b7,congo=t61rcWkgMzE"]),
            ok("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE"),
        ),
        (
            "spec:3.3.2",
            s(&["rojo=00f067aa0ba902b7", "congo=t61rcWkgMzE"]),
            ok("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE"),
        ),
        ("spec:3.3.1.3.1", s(&["1a@b=1"]), ok("1a@b=1")),
        ("spec-l2:3.3.2.2.1", s(&["1a=1"]), ok("1a=1")),
        ("spec-l2:3.3.2.2.1", s(&["a@1b=1"]), ok("a@1b=1")),
        ("spec-l2:3.3.2.2.1", s(&["a@=1"]), ok("a@=1")),
        (
            "spec-l2:3.3.2.2.1",
            s(&["_a=1"]),
            Err(E::InvalidTraceStateKey),
        ),
        (
            "spec-l2:3.3.2.2.1",
            s(&["a.b=1"]),
            Err(E::InvalidTraceStateKey),
        ),
        ("spec:3.3.1.3.1", s(&["=1"]), Err(E::InvalidTraceStateKey)),
        ("local", s(&["foo"]), Err(E::MalformedTraceStateMember)),
        (
            "local",
            s(&["foo=1,bar"]),
            Err(E::MalformedTraceStateMember),
        ),
        (
            "spec:3.3.1.3.2",
            vec![format!("a={}", "v".repeat(256))],
            Ok(format!("a={}", "v".repeat(256))),
        ),
        (
            "spec:3.3.1.3.2",
            vec![format!("a={}", "v".repeat(257))],
            Err(E::InvalidTraceStateValue),
        ),
        ("spec:3.3.1.3.2", s(&["a= 1"]), ok("a= 1")),
        ("local", s(&["a=x\ty"]), Err(E::InvalidTraceStateValue)),
        ("local", s(&["a=\u{e9}"]), Err(E::InvalidTraceStateValue)),
        ("local", s(&["a=\u{1}"]), Err(E::InvalidTraceStateValue)),
        ("local", s(&["a\u{e9}=1"]), Err(E::InvalidTraceStateKey)),
        ("local", s(&["a=1\n"]), Err(E::InvalidTraceStateValue)),
        // Empty members do not count towards the 32.
        (
            "local",
            vec![format!("{},,,", bar_members(32).join(",")), " , ".into()],
            Ok(bar_members(32).join(",")),
        ),
        // Duplicates count towards the 32 (they are list members of the received value).
        (
            "local",
            vec![format!("{},bar01=x", bar_members(32).join(","))],
            Err(E::TooManyTraceStateMembers),
        ),
    ]
}

#[test]
fn tracestate_corpus() {
    for (source, fields, want) in tracestate_cases() {
        let got =
            TraceState::parse_fields(fields.iter().map(String::as_str)).map(|ts| ts.to_string());
        assert_eq!(got, want, "{source}: {fields:?}");
    }
}

#[test]
fn tracestate_lookup_and_order() {
    let ts = TraceState::parse("rojo=00f067aa0ba902b7, congo=t61rcWkgMzE").unwrap();
    assert_eq!(ts.len(), 2);
    assert!(!ts.is_empty());
    assert_eq!(ts.get("rojo"), Some("00f067aa0ba902b7"));
    assert_eq!(ts.get("congo"), Some("t61rcWkgMzE"));
    assert_eq!(ts.get("nope"), None);
    let order: Vec<_> = ts.iter().collect();
    assert_eq!(
        order,
        [("rojo", "00f067aa0ba902b7"), ("congo", "t61rcWkgMzE")]
    );
    assert_eq!("a=1".parse::<TraceState>().unwrap().get("a"), Some("1"));
    assert!(TraceState::new().is_empty());
    assert_eq!(TraceState::new().to_string(), "");
}

#[test]
fn tracestate_insert_update_remove() {
    let mut ts = TraceState::parse("rojo=1,congo=2").unwrap();
    // A new key goes to the left (§3.5).
    ts.insert("droiduse", "a").unwrap();
    assert_eq!(ts.to_string(), "droiduse=a,rojo=1,congo=2");
    // An updated key moves to the left; the others keep their order.
    ts.insert("congo", "3").unwrap();
    assert_eq!(ts.to_string(), "congo=3,droiduse=a,rojo=1");
    // Invalid input changes nothing.
    assert_eq!(ts.insert("Bad", "1"), Err(E::InvalidTraceStateKey));
    assert_eq!(ts.insert("ok", ""), Err(E::InvalidTraceStateValue));
    assert_eq!(ts.insert("ok", "a,b"), Err(E::InvalidTraceStateValue));
    assert_eq!(ts.insert("ok", "trailing "), Err(E::InvalidTraceStateValue));
    assert_eq!(ts.to_string(), "congo=3,droiduse=a,rojo=1");
    assert_eq!(ts.remove("droiduse"), Some("a".to_string()));
    assert_eq!(ts.remove("droiduse"), None);
    assert_eq!(ts.to_string(), "congo=3,rojo=1");

    // At 32 members, adding one removes the right-most.
    let mut full = TraceState::parse_fields(bar_members(32).iter().map(String::as_str)).unwrap();
    assert_eq!(full.len(), TRACESTATE_MAX_MEMBERS);
    full.insert("new", "1").unwrap();
    assert_eq!(full.len(), TRACESTATE_MAX_MEMBERS);
    assert_eq!(full.iter().next(), Some(("new", "1")));
    assert_eq!(full.get("bar32"), None);
    assert_eq!(full.get("bar31"), Some("31"));
    // Updating an existing key at 32 members removes nothing else.
    full.insert("bar05", "x").unwrap();
    assert_eq!(full.len(), TRACESTATE_MAX_MEMBERS);
    assert_eq!(full.get("bar31"), Some("31"));
}

#[test]
fn tracestate_truncation_removes_whole_members() {
    let long = "l".repeat(130);
    let mut ts = TraceState::new();
    for (k, v) in [
        ("e", "5"),
        ("d", "4"),
        ("big", long.as_str()),
        ("b", "2"),
        ("a", "1"),
    ] {
        ts.insert(k, v).unwrap();
    }
    let full = ts.to_string();
    assert_eq!(full, format!("a=1,b=2,big={long},d=4,e=5"));
    assert_eq!(ts.header_within(full.len()), full);
    assert_eq!(ts.header_within(usize::MAX), full);
    // Members over 128 characters go first, then members from the right.
    assert_eq!(ts.header_within(full.len() - 1), "a=1,b=2,d=4,e=5");
    assert_eq!(ts.header_within(15), "a=1,b=2,d=4,e=5");
    assert_eq!(ts.header_within(14), "a=1,b=2,d=4");
    assert_eq!(ts.header_within(3), "a=1");
    assert_eq!(ts.header_within(2), "");
    assert_eq!(ts.header_within(0), "");
    for max in 0..full.len() + 2 {
        let h = ts.header_within(max);
        assert!(h.len() <= max, "{max}: {h}");
        assert!(TraceState::parse(&h).is_ok());
    }
}

// ---------------------------------------------------------------------------
// Both fields (§4.2, §4.3)
// ---------------------------------------------------------------------------

#[test]
fn extract_follows_the_processing_model() {
    let tp = format!("00-{T}-{P}-01");
    let parent = TraceParent::parse(&tp).unwrap();

    // w3c:test_both_traceparent_and_tracestate_missing
    assert_eq!(TraceContext::extract(None::<&str>, None::<&str>), None);
    // w3c:test_tracestate_included_traceparent_missing
    assert_eq!(TraceContext::extract(None::<&str>, Some("foo=1")), None);
    // w3c:test_traceparent_duplicated
    let two = [
        format!("00-{T}-{P}-01"),
        format!("00-12345678901234567890123456789011-{P}-01"),
    ];
    assert_eq!(
        TraceContext::extract(two.iter().map(String::as_str), ["foo=1"]),
        None
    );
    assert_eq!(
        TraceContext::extract([tp.as_str(), tp.as_str()], None::<&str>),
        None
    );
    // Invalid traceparent: tracestate is not parsed.
    assert_eq!(
        TraceContext::extract(Some(format!("ff-{T}-{P}-01").as_str()), Some("foo=1")),
        None
    );
    // Valid traceparent, no tracestate.
    assert_eq!(
        TraceContext::extract(Some(tp.as_str()), None::<&str>),
        Some(TraceContext::new(parent))
    );
    // w3c:test_tracestate_included_traceparent_included
    let got = TraceContext::extract(Some(tp.as_str()), Some("foo=1,bar=2")).unwrap();
    assert_eq!(got.parent, parent);
    assert_eq!(got.state.to_string(), "foo=1,bar=2");
    // Invalid tracestate next to a valid traceparent: tracestate discarded, traceparent kept.
    let got = TraceContext::extract(Some(tp.as_str()), Some("foo=1,BAR=2")).unwrap();
    assert_eq!(got, TraceContext::new(parent));
    // Several tracestate fields are combined.
    let got = TraceContext::extract([tp.as_str()], ["foo=1", "", "bar=2"]).unwrap();
    assert_eq!(got.state.to_string(), "foo=1,bar=2");
}

// ---------------------------------------------------------------------------
// Property-style tests (std only)
// ---------------------------------------------------------------------------

/// Lowercase hex digit.
fn hexlc(c: u8) -> bool {
    c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
}

/// An independent statement of §3.2.2 / §3.2.4 for values without surrounding whitespace:
/// the character class of every position of the 55-character prefix, then what may follow.
fn oracle_valid(v: &[u8]) -> bool {
    if v.len() < 55 {
        return false;
    }
    for (i, &c) in v[..55].iter().enumerate() {
        let want_dash = matches!(i, 2 | 35 | 52);
        if want_dash != (c == b'-') || (!want_dash && !hexlc(c)) {
            return false;
        }
    }
    let version = &v[..2];
    if version == b"ff" {
        return false;
    }
    if v[3..35].iter().all(|&c| c == b'0') || v[36..52].iter().all(|&c| c == b'0') {
        return false;
    }
    if version == b"00" {
        v.len() == 55
    } else {
        v.len() == 55 || v[55] == b'-'
    }
}

/// Every single-byte substitution (all ASCII bytes at every position) of values next to the
/// boundaries: ids one digit away from all zeros, version one digit away from `ff`, versions 00
/// and higher, with and without a suffix.
#[test]
fn traceparent_single_byte_substitutions_match_the_grammar() {
    let bases = [
        "00-00000000000000000000000000000001-0000000000000001-01".to_string(),
        "fe-10000000000000000000000000000000-1000000000000000-00".to_string(),
        "fe-10000000000000000000000000000000-1000000000000000-00-x".to_string(),
        "0f-00000000000000000000000000000001-0000000000000001-ff".to_string(),
    ];
    let mut checked = 0;
    for base in &bases {
        for i in 0..base.len() {
            for b in 0u8..=0x7f {
                let mut v = base.clone().into_bytes();
                v[i] = b;
                let s = String::from_utf8(v).unwrap();
                let trimmed = s.trim_matches(|c| c == ' ' || c == '\t');
                let ok = TraceParent::parse(&s).is_ok();
                assert_eq!(ok, oracle_valid(trimmed.as_bytes()), "{s:?}");
                if let Ok(tp) = TraceParent::parse(&s) {
                    // Round trip through the version 00 form.
                    assert_eq!(TraceParent::parse(&tp.to_string()), Ok(tp), "{s:?}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 25_000);
}

/// Every prefix of a valid value, and the value followed by every ASCII byte.
#[test]
fn traceparent_truncation_and_extension() {
    for base in [format!("00-{T}-{P}-01"), format!("cc-{T}-{P}-01")] {
        for n in 0..base.len() {
            assert!(TraceParent::parse(&base[..n]).is_err(), "{:?}", &base[..n]);
        }
        for b in 0u8..=0x7f {
            let s = format!("{base}{}", char::from(b));
            let want = matches!(b, b' ' | b'\t') || (base.starts_with("cc") && b == b'-');
            assert_eq!(TraceParent::parse(&s).is_ok(), want, "{s:?}");
        }
    }
}

/// xorshift64*: a deterministic generator, so a failure always reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Characters that matter to the grammars, plus non-ASCII and control characters.
const ALPHABET: &[char] = &[
    '0',
    '1',
    '9',
    'a',
    'f',
    'g',
    'z',
    'A',
    'F',
    'Z',
    '-',
    '+',
    '.',
    ',',
    '=',
    '@',
    '_',
    '*',
    '/',
    ' ',
    '\t',
    '\n',
    '\0',
    '~',
    '\u{7f}',
    '\u{e9}',
    '\u{a0}',
    '\u{2028}',
    '\u{10ffff}',
];

/// Random strings, random edits of valid values: no panic, and every accepted value round-trips.
#[test]
fn random_inputs_never_panic_and_round_trip() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let seeds = [
        format!("00-{T}-{P}-01"),
        format!("cc-{T}-{P}-01-future"),
        "rojo=00f067aa0ba902b7,congo=t61rcWkgMzE".to_string(),
        format!("a@b=1, {}=2 ,x=y z", "k".repeat(250)),
    ];
    for _ in 0..40_000 {
        let s: String = if rng.below(2) == 0 {
            let len = rng.below(90);
            (0..len).map(|_| *rng.pick(ALPHABET)).collect()
        } else {
            let mut chars: Vec<char> = rng.pick(&seeds).chars().collect();
            for _ in 0..1 + rng.below(3) {
                let i = rng.below(chars.len() + 1);
                match rng.below(3) {
                    0 if i < chars.len() => chars[i] = *rng.pick(ALPHABET),
                    1 if i < chars.len() => {
                        chars.remove(i);
                    }
                    _ => chars.insert(i, *rng.pick(ALPHABET)),
                }
            }
            chars.into_iter().collect()
        };
        if let Ok(tp) = TraceParent::parse(&s) {
            assert_eq!(TraceParent::parse(&tp.to_string()), Ok(tp), "{s:?}");
        }
        if let Ok(ts) = TraceState::parse(&s) {
            assert!(ts.len() <= TRACESTATE_MAX_MEMBERS);
            assert_eq!(TraceState::parse(&ts.to_string()), Ok(ts.clone()), "{s:?}");
            let _ = ts.header_within(rng.below(64));
        }
        let _ = TraceContext::extract(Some(s.as_str()), Some(s.as_str()));
        let _ = TraceId::from_hex(&s);
        let _ = SpanId::from_hex(&s);
    }
}

/// Random valid `tracestate` values with random optional whitespace and empty members parse to
/// exactly the members that were generated, in order.
#[test]
fn random_valid_tracestate_parses_to_its_members() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let key_chars: Vec<char> = ALL_KEY_CHARS.chars().collect();
    let lc: Vec<char> = ('a'..='z').collect();
    let value_chars: Vec<char> = all_value_chars().chars().collect();
    let ows = [" ", "\t", "", "", " \t "];
    for round in 0..2_000 {
        let n = rng.below(TRACESTATE_MAX_MEMBERS + 1);
        let mut members: Vec<(String, String)> = Vec::new();
        while members.len() < n {
            let mut key: String = std::iter::once(*rng.pick(&lc))
                .chain((0..rng.below(20)).map(|_| *rng.pick(&key_chars)))
                .collect();
            if rng.below(4) == 0 {
                key.push('@');
                key.push(*rng.pick(&lc));
                key.extend((0..rng.below(13)).map(|_| *rng.pick(&key_chars)));
            }
            let mut value: String = (0..rng.below(30))
                .map(|_| *rng.pick(&value_chars))
                .collect();
            value.push('~');
            if members.iter().all(|(k, _)| *k != key) {
                members.push((key, value));
            }
        }
        let mut header = String::new();
        for (i, (k, v)) in members.iter().enumerate() {
            if i > 0 {
                header.push_str(rng.pick(&ows));
                header.push(',');
                if rng.below(5) == 0 {
                    header.push(',');
                }
            }
            header.push_str(rng.pick(&ows));
            header.push_str(&format!("{k}={v}"));
        }
        let ts =
            TraceState::parse(&header).unwrap_or_else(|e| panic!("round {round}: {e}: {header:?}"));
        let got: Vec<(String, String)> = ts
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(got, members, "{header:?}");
    }
}
