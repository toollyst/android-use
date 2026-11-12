//! The trace of one request (`docs/02-architecture.md` §5, `docs/04-adp.md` §4).
//!
//! Every request runs in a span `host/entrypoints/adp_server#request` with the field
//! `trace_id`; the spans the layers below open while serving it are its descendants, so they
//! belong to that trace. No exporter yet: the spans reach only the log.

use droiduse_contracts_adp::envelope::Request;
use droiduse_contracts_common::{TraceContext, TraceId};

use crate::TraceMode;

/// A new random trace id (§3.2.2.3: not all zero).
fn new_trace_id() -> TraceId {
    loop {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the OS random number generator failed");
        if let Some(id) = TraceId::from_bytes(bytes) {
            return id;
        }
    }
}

/// The span of `req`.
pub(crate) fn request_span(req: &Request, mode: TraceMode) -> tracing::Span {
    let received = TraceContext::extract(req.traceparent.as_deref(), req.tracestate.as_deref());
    let (trace_id, link) = match (mode, received) {
        (TraceMode::Continue, Some(c)) => (c.parent.trace_id(), None),
        (TraceMode::NewWithLink, Some(c)) => (new_trace_id(), Some(c.parent.trace_id())),
        (_, None) => (new_trace_id(), None),
    };
    let span = tracing::info_span!(
        "host/entrypoints/adp_server#request",
        trace_id = %trace_id,
        link_trace_id = tracing::field::Empty,
        method = %req.method,
        id = req.id,
    );
    if let Some(link) = link {
        span.record("link_trace_id", tracing::field::display(link));
    }
    span
}
