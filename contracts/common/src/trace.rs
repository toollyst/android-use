//! W3C Trace Context at wire and IPC boundaries.
//!
//! Spec: W3C Trace Context Level 1, W3C Recommendation of 23 November 2021
//! (<https://www.w3.org/TR/trace-context/>). Section numbers below refer to that document.
//!
//! Inside a process the trace context is ambient (the current tracing / OpenTelemetry context of
//! the host layers); it is not a parameter of the contract traits. At every wire and IPC boundary
//! it is carried explicitly as the `traceparent` and `tracestate` fields (ADP request fields,
//! guest_link and node gRPC metadata). These types parse and produce exactly those two fields.
//! They hold no global state and need no runtime.
//!
//! Receiving (§3.2, §3.3, §4.2, §4.3). Input is untrusted. [`TraceContext::extract`] returns
//! `None` when the receiver has to start a new trace: `traceparent` is absent, appears more than
//! once, or is invalid. `tracestate` is then not parsed at all. A `tracestate` that is invalid
//! next to a valid `traceparent` is discarded as a whole (§4.3 step 3 allows that), and the
//! `traceparent` is kept.
//!
//! - `traceparent` versions above `00` are parsed as §3.2.4 says: the first 55 characters as
//!   version `00`, then the end of the value or a `-`; the rest is not interpreted. Version `ff`
//!   is invalid (§3.2.2.1).
//! - Trace-flags bits this version does not define are dropped on input
//!   ([`TraceFlags::from_bits_truncate`]), so they are zero on output (§3.2.2.5.2). Level 1
//!   defines only `sampled`.
//! - Spaces and horizontal tabs around a value and around `tracestate` list members are ignored
//!   (§3.3.1.1; HTTP optional whitespace).
//! - `tracestate` keys are accepted in the key grammar of Trace Context Level 2 (W3C Candidate
//!   Recommendation Draft, §3.3.2.2.1), a superset of Level 1 §3.3.1.3.1, so members other
//!   systems wrote in that grammar are kept and passed on. Values follow §3.3.1.3.2. More than 32
//!   members is invalid (§3.3.1.1). Of several members with the same key the left-most is kept,
//!   the others are dropped (§3.3.1.4 allows one entry per key).
//!
//! Sending (§3.4, §3.5). [`TraceParent`]'s `Display` always writes version `00` (§3.2.4).
//! [`TraceState`]'s `Display` writes the members without optional whitespace;
//! [`TraceState::header_within`] truncates by whole members as §3.3.1.5 recommends. A sender
//! should leave out an empty `tracestate` (§3.3.1.1).

use std::fmt;
use std::str::FromStr;

/// Name of the `traceparent` field (§3.2.1): the ADP request field and the gRPC metadata key.
pub const TRACEPARENT: &str = "traceparent";

/// Name of the `tracestate` field (§3.3.1): the ADP request field and the gRPC metadata key.
pub const TRACESTATE: &str = "tracestate";

/// Largest number of `tracestate` list members (§3.3.1.1).
pub const TRACESTATE_MAX_MEMBERS: usize = 32;

/// Length of a version `00` `traceparent` value; also the shortest value of a higher version
/// that is parsed (§3.2.4).
const TRACEPARENT_LEN: usize = 55;

/// A `tracestate` member longer than this is removed first when truncating (§3.3.1.5).
const TRUNCATE_FIRST_OVER: usize = 128;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a `traceparent` or `tracestate` value, or a part of one, is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TraceContextError {
    /// The version is not two lowercase hex digits followed by `-`, or it is the forbidden
    /// version `ff` (§3.2.2.1, §3.2.4).
    InvalidVersion,
    /// The value has the wrong length or a delimiter is missing: version `00` is exactly 55
    /// characters; a higher version is at least 55 characters and continues only with `-`
    /// (§3.2.2, §3.2.4).
    MalformedTraceParent,
    /// The trace-id is not 32 lowercase hex digits, or it is all zeros (§3.2.2.3).
    InvalidTraceId,
    /// The parent-id is not 16 lowercase hex digits, or it is all zeros (§3.2.2.4).
    InvalidParentId,
    /// The trace-flags are not two lowercase hex digits (§3.2.2.5).
    InvalidTraceFlags,
    /// A non-empty `tracestate` list member has no `=` (§3.3.1.3).
    MalformedTraceStateMember,
    /// A `tracestate` key does not follow the key grammar (Level 2 §3.3.2.2.1).
    InvalidTraceStateKey,
    /// A `tracestate` value does not follow §3.3.1.3.2.
    InvalidTraceStateValue,
    /// `tracestate` has more than [`TRACESTATE_MAX_MEMBERS`] list members (§3.3.1.1).
    TooManyTraceStateMembers,
}

impl fmt::Display for TraceContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::InvalidVersion => "invalid traceparent version",
            Self::MalformedTraceParent => "malformed traceparent",
            Self::InvalidTraceId => "invalid trace-id",
            Self::InvalidParentId => "invalid parent-id",
            Self::InvalidTraceFlags => "invalid trace-flags",
            Self::MalformedTraceStateMember => "malformed tracestate list member",
            Self::InvalidTraceStateKey => "invalid tracestate key",
            Self::InvalidTraceStateValue => "invalid tracestate value",
            Self::TooManyTraceStateMembers => "too many tracestate list members",
        };
        f.write_str(s)
    }
}

impl std::error::Error for TraceContextError {}

// ---------------------------------------------------------------------------
// Identifiers and flags
// ---------------------------------------------------------------------------

/// A trace-id (§3.2.2.3): 16 bytes, never all zeros.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// The trace-id with these bytes; `None` for all zeros, which is invalid.
    pub const fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        if all_zero(&bytes) {
            None
        } else {
            Some(Self(bytes))
        }
    }

    /// The bytes of the trace-id.
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0
    }

    /// Parses exactly 32 lowercase hex digits that are not all zeros.
    pub fn from_hex(hex: &str) -> Result<Self, TraceContextError> {
        decode_hex::<16>(hex.as_bytes())
            .and_then(Self::from_bytes)
            .ok_or(TraceContextError::InvalidTraceId)
    }
}

impl fmt::Display for TraceId {
    /// 32 lowercase hex digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraceId({self})")
    }
}

impl FromStr for TraceId {
    type Err = TraceContextError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// A span id: the parent-id field of `traceparent` (§3.2.2.4). 8 bytes, never all zeros.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// The span id with these bytes; `None` for all zeros, which is invalid.
    pub const fn from_bytes(bytes: [u8; 8]) -> Option<Self> {
        if all_zero(&bytes) {
            None
        } else {
            Some(Self(bytes))
        }
    }

    /// The bytes of the span id.
    pub const fn to_bytes(self) -> [u8; 8] {
        self.0
    }

    /// Parses exactly 16 lowercase hex digits that are not all zeros.
    pub fn from_hex(hex: &str) -> Result<Self, TraceContextError> {
        decode_hex::<8>(hex.as_bytes())
            .and_then(Self::from_bytes)
            .ok_or(TraceContextError::InvalidParentId)
    }
}

impl fmt::Display for SpanId {
    /// 16 lowercase hex digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SpanId({self})")
    }
}

impl FromStr for SpanId {
    type Err = TraceContextError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// The trace-flags of `traceparent` (§3.2.2.5).
///
/// It only ever holds the bits the supported spec level defines; today that is `sampled`
/// (Level 1). Another defined bit (such as the Level 2 `random` flag) can be added as a new
/// constant without changing this API.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TraceFlags(u8);

impl TraceFlags {
    /// No flag set.
    pub const NONE: Self = Self(0x00);
    /// The `sampled` flag (§3.2.2.5.1).
    pub const SAMPLED: Self = Self(0x01);

    /// Bits defined by the supported spec level.
    const DEFINED: u8 = Self::SAMPLED.0;

    /// The flags of `bits`, keeping only the defined bits (§3.2.2.5.2: other bits are zero on
    /// output).
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::DEFINED)
    }

    /// The flags as the trace-flags byte.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether `sampled` is set.
    pub const fn is_sampled(self) -> bool {
        self.0 & Self::SAMPLED.0 != 0
    }

    /// These flags with `sampled` set or cleared.
    pub const fn with_sampled(self, sampled: bool) -> Self {
        if sampled {
            Self(self.0 | Self::SAMPLED.0)
        } else {
            Self(self.0 & !Self::SAMPLED.0)
        }
    }
}

impl fmt::Debug for TraceFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraceFlags({:02x})", self.0)
    }
}

// ---------------------------------------------------------------------------
// traceparent
// ---------------------------------------------------------------------------

/// A valid `traceparent` (§3.2): trace-id, parent-id and trace-flags.
///
/// The received version is not kept: the value is always written as version `00` (§3.2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceParent {
    trace_id: TraceId,
    parent_id: SpanId,
    flags: TraceFlags,
}

impl TraceParent {
    /// A `traceparent` from its parts.
    pub const fn new(trace_id: TraceId, parent_id: SpanId, flags: TraceFlags) -> Self {
        Self {
            trace_id,
            parent_id,
            flags,
        }
    }

    /// Parses a received `traceparent` value (§3.2.2, §3.2.4). Leading and trailing spaces and
    /// horizontal tabs are ignored.
    pub fn parse(value: &str) -> Result<Self, TraceContextError> {
        let v = trim_ows(value.as_bytes());
        // Version prefix: two lowercase hex digits and a dash (§3.2.4).
        let version = match (v.get(..2).and_then(decode_hex::<1>), v.get(2)) {
            (Some([version]), Some(b'-')) if version != 0xff => version,
            _ => return Err(TraceContextError::InvalidVersion),
        };
        let fits = if version == 0 {
            v.len() == TRACEPARENT_LEN
        } else {
            // A higher version: at least 55 characters, then the end or a dash (§3.2.4).
            v.len() >= TRACEPARENT_LEN && v.get(TRACEPARENT_LEN).is_none_or(|&c| c == b'-')
        };
        if !fits || v[35] != b'-' || v[52] != b'-' {
            return Err(TraceContextError::MalformedTraceParent);
        }
        let trace_id = decode_hex::<16>(&v[3..35])
            .and_then(TraceId::from_bytes)
            .ok_or(TraceContextError::InvalidTraceId)?;
        let parent_id = decode_hex::<8>(&v[36..52])
            .and_then(SpanId::from_bytes)
            .ok_or(TraceContextError::InvalidParentId)?;
        let [flags] = decode_hex::<1>(&v[53..55]).ok_or(TraceContextError::InvalidTraceFlags)?;
        Ok(Self::new(
            trace_id,
            parent_id,
            TraceFlags::from_bits_truncate(flags),
        ))
    }

    /// The trace-id.
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    /// The parent-id: the span of the caller.
    pub const fn parent_id(&self) -> SpanId {
        self.parent_id
    }

    /// The trace-flags.
    pub const fn flags(&self) -> TraceFlags {
        self.flags
    }

    /// This `traceparent` with another parent-id: the one an outgoing call carries (§3.4).
    pub const fn with_parent_id(self, parent_id: SpanId) -> Self {
        Self { parent_id, ..self }
    }

    /// This `traceparent` with other flags (§3.4: the sampled flag is updated together with the
    /// parent-id).
    pub const fn with_flags(self, flags: TraceFlags) -> Self {
        Self { flags, ..self }
    }
}

impl fmt::Display for TraceParent {
    /// The version `00` value, e.g. `00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "00-{}-{}-{:02x}",
            self.trace_id,
            self.parent_id,
            self.flags.bits()
        )
    }
}

impl FromStr for TraceParent {
    type Err = TraceContextError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ---------------------------------------------------------------------------
// tracestate
// ---------------------------------------------------------------------------

/// A valid `tracestate` (§3.3): at most 32 key/value members with unique keys, in order (the
/// left-most member is the most recent).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TraceState {
    members: Vec<(String, String)>,
}

impl TraceState {
    /// An empty `tracestate`.
    pub const fn new() -> Self {
        Self {
            members: Vec::new(),
        }
    }

    /// Parses one received `tracestate` field value. See [`TraceState::parse_fields`].
    pub fn parse(value: &str) -> Result<Self, TraceContextError> {
        Self::parse_fields([value])
    }

    /// Parses a received `tracestate` that may arrive as several fields (HTTP header fields or
    /// gRPC metadata values with the same name), combined in order (§3.3.1.1).
    ///
    /// Empty list members and spaces and horizontal tabs around members are ignored. Of
    /// several members with the same key, the left-most is kept. Any invalid member, or more
    /// than 32 members, makes the whole value invalid.
    pub fn parse_fields<'a, I>(fields: I) -> Result<Self, TraceContextError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut members: Vec<(String, String)> = Vec::new();
        let mut count = 0usize;
        for field in fields {
            for member in field.as_bytes().split(|&c| c == b',') {
                let member = trim_ows(member);
                if member.is_empty() {
                    continue;
                }
                count += 1;
                if count > TRACESTATE_MAX_MEMBERS {
                    return Err(TraceContextError::TooManyTraceStateMembers);
                }
                let eq = member
                    .iter()
                    .position(|&c| c == b'=')
                    .ok_or(TraceContextError::MalformedTraceStateMember)?;
                let (key, value) = (&member[..eq], &member[eq + 1..]);
                if !valid_key(key) {
                    return Err(TraceContextError::InvalidTraceStateKey);
                }
                if !valid_value(value) {
                    return Err(TraceContextError::InvalidTraceStateValue);
                }
                // Both are ASCII after validation.
                let key = ascii_string(key);
                if members.iter().all(|(k, _)| *k != key) {
                    members.push((key, ascii_string(value)));
                }
            }
        }
        Ok(Self { members })
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.members
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The members, left-most first.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &str)> + '_ {
        self.members.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of members.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether there are no members.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Adds `key=value`, or updates the value of `key`, as the left-most member (§3.5). When
    /// that makes more than 32 members, the right-most member is removed (the rule Level 2
    /// §3.3.2 states; Level 1 does not say). An invalid key or value changes nothing.
    pub fn insert(&mut self, key: &str, value: &str) -> Result<(), TraceContextError> {
        if !valid_key(key.as_bytes()) {
            return Err(TraceContextError::InvalidTraceStateKey);
        }
        if !valid_value(value.as_bytes()) {
            return Err(TraceContextError::InvalidTraceStateValue);
        }
        self.members.retain(|(k, _)| k != key);
        self.members.insert(0, (key.to_string(), value.to_string()));
        self.members.truncate(TRACESTATE_MAX_MEMBERS);
        Ok(())
    }

    /// Removes `key` and returns its value (§3.5).
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let i = self.members.iter().position(|(k, _)| k == key)?;
        Some(self.members.remove(i).1)
    }

    /// The value to send, at most `max_len` characters, truncated by whole members as §3.3.1.5
    /// recommends: members longer than 128 characters are removed first, then members from the
    /// right. §3.3.1.5 asks to propagate at least 512 characters.
    pub fn header_within(&self, max_len: usize) -> String {
        let mut keep: Vec<&(String, String)> = self.members.iter().collect();
        let member_len = |(k, v): &(String, String)| k.len() + 1 + v.len();
        let total = |m: &[&(String, String)]| {
            m.iter().map(|e| member_len(e)).sum::<usize>() + m.len().saturating_sub(1)
        };
        while total(&keep) > max_len {
            match keep
                .iter()
                .rposition(|e| member_len(e) > TRUNCATE_FIRST_OVER)
            {
                Some(i) => {
                    keep.remove(i);
                }
                None => {
                    keep.pop();
                }
            }
        }
        join_members(keep.into_iter())
    }
}

impl fmt::Display for TraceState {
    /// The members joined by `,`, without optional whitespace. Empty when there are no
    /// members.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&join_members(self.members.iter()))
    }
}

impl FromStr for TraceState {
    type Err = TraceContextError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ---------------------------------------------------------------------------
// Both fields
// ---------------------------------------------------------------------------

/// The trace context received from, or sent to, the other side of a boundary.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraceContext {
    /// The `traceparent`.
    pub parent: TraceParent,
    /// The `tracestate`; empty when none was received or it was invalid.
    pub state: TraceState,
}

impl TraceContext {
    /// A trace context with an empty `tracestate`.
    pub const fn new(parent: TraceParent) -> Self {
        Self {
            parent,
            state: TraceState::new(),
        }
    }

    /// The trace context of a received message, following §4.2 and §4.3.
    ///
    /// `traceparent` and `tracestate` are every value of that name the message carries, in
    /// order. An ADP request has at most one of each (`Option<&str>` is such an iterator);
    /// gRPC metadata may have several.
    ///
    /// `None` means the receiver starts a new trace: there is not exactly one `traceparent`
    /// or it is invalid, and `tracestate` is then not parsed (§3.3). An invalid `tracestate`
    /// next to a valid `traceparent` is discarded as a whole (§4.3 step 3).
    pub fn extract<'a, P, S>(traceparent: P, tracestate: S) -> Option<Self>
    where
        P: IntoIterator<Item = &'a str>,
        S: IntoIterator<Item = &'a str>,
    {
        let mut values = traceparent.into_iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return None;
        };
        let parent = TraceParent::parse(value).ok()?;
        let state = TraceState::parse_fields(tracestate).unwrap_or_default();
        Some(Self { parent, state })
    }
}

// ---------------------------------------------------------------------------
// Helpers (bytes only: no slicing of `str`, so no input can panic on a char boundary)
// ---------------------------------------------------------------------------

const fn all_zero(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != 0 {
            return false;
        }
        i += 1;
    }
    true
}

/// Value of one lowercase hex digit (HEXDIGLC, §3.2.2).
const fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Exactly `2 * N` lowercase hex digits.
fn decode_hex<const N: usize>(hex: &[u8]) -> Option<[u8; N]> {
    if hex.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    let (pairs, _) = hex.as_chunks::<2>();
    for (byte, &[hi, lo]) in out.iter_mut().zip(pairs) {
        *byte = (hex_digit(hi)? << 4) | hex_digit(lo)?;
    }
    Some(out)
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    bytes.iter().try_for_each(|b| write!(f, "{b:02x}"))
}

/// Without leading and trailing spaces and horizontal tabs (OWS).
fn trim_ows(mut s: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' ' | b'\t'] = s {
        s = rest;
    }
    s
}

/// A key of Trace Context Level 2 §3.3.2.2.1, which accepts every key of Level 1 §3.3.1.3.1:
///
/// ```text
/// key     = ( lcalpha / DIGIT ) 0*255 ( keychar )
/// keychar = lcalpha / DIGIT / "_" / "-"/ "*" / "/" / "@"
/// ```
fn valid_key(key: &[u8]) -> bool {
    let key_char =
        |c: u8| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'*' | b'/' | b'@');
    matches!(key, [b'a'..=b'z' | b'0'..=b'9', rest @ ..] if rest.len() <= 255 && rest.iter().all(|&c| key_char(c)))
}

/// A value of §3.3.1.3.2:
///
/// ```text
/// value    = 0*255(chr) nblk-chr
/// nblk-chr = %x21-2B / %x2D-3C / %x3E-7E
/// chr      = %x20 / nblk-chr
/// ```
fn valid_value(value: &[u8]) -> bool {
    let nblk = |c: u8| matches!(c, 0x21..=0x2b | 0x2d..=0x3c | 0x3e..=0x7e);
    match value {
        [rest @ .., last] => {
            rest.len() <= 255 && nblk(*last) && rest.iter().all(|&c| c == 0x20 || nblk(c))
        }
        [] => false,
    }
}

/// `bytes` is ASCII (checked by `valid_key` / `valid_value`).
fn ascii_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&c| char::from(c)).collect()
}

fn join_members<'a>(members: impl Iterator<Item = &'a (String, String)>) -> String {
    let mut out = String::new();
    for (i, (k, v)) in members.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out
}
