//! One WebSocket connection: authentication, `Connection.hello`, the envelope rules, dispatch.

use std::sync::Arc;
use std::time::Duration;

use droiduse_contracts_adp::connection::{AuthParams, HelloParams, HelloReturns};
use droiduse_contracts_adp::{Command, PROTOCOL_VERSION, Scope, envelope};
use droiduse_contracts_common::{Error, ErrorCode};
use droiduse_contracts_spi::{BearerToken, ChannelCtx, Credential, Principal};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tracing::Instrument;

use crate::{COMPONENT, Shared, dispatch, trace};

/// Largest request id (2^53 - 1).
const MAX_ID: i64 = (1 << 53) - 1;

/// How long to wait for the client's close frame after the server closed.
const CLOSE_WAIT: Duration = Duration::from_secs(5);

/// What one incoming message leads to.
#[derive(Default)]
struct Outcome {
    /// Sent first, in order.
    events: Vec<envelope::Event>,
    /// Then the response.
    response: Option<envelope::Response>,
    /// Then the connection is closed with this status.
    close: Option<(CloseCode, &'static str)>,
}

impl Outcome {
    fn close(code: CloseCode, reason: &'static str) -> Self {
        Self {
            close: Some((code, reason)),
            ..Self::default()
        }
    }
}

struct Connection {
    principal: Option<Principal>,
    hello_done: bool,
    ctx: ChannelCtx,
    shared: Arc<Shared>,
}

/// Serves the connection until either side closes it.
pub(crate) async fn run<S>(
    mut ws: WebSocketStream<S>,
    principal: Option<Principal>,
    ctx: ChannelCtx,
    shared: Arc<Shared>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let mut conn = Connection {
        principal,
        hello_done: false,
        ctx,
        shared,
    };
    while let Some(message) = ws.next().await {
        let outcome = match message {
            Ok(Message::Text(text)) => conn.handle(text.as_str()).await,
            // No request takes a binary message yet (docs/04-adp.md §4: close with 1002).
            Ok(Message::Binary(_)) => Outcome::close(
                CloseCode::Protocol,
                "binary message with no matching request",
            ),
            // Pings are answered and a close is echoed by tungstenite while reading.
            Ok(_) => continue,
            Err(e) => {
                tracing::debug!(error = %e, "the WebSocket connection failed");
                return;
            }
        };
        if send(&mut ws, outcome).await.is_err() {
            return;
        }
    }
}

/// Sends `outcome`; `Err` once the connection is closed.
async fn send<S>(ws: &mut WebSocketStream<S>, outcome: Outcome) -> Result<(), ()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let texts = outcome
        .events
        .iter()
        .map(serde_json::to_string)
        .chain(outcome.response.iter().map(serde_json::to_string));
    for text in texts {
        let text = text.expect("envelope types serialize");
        ws.send(Message::text(text)).await.map_err(|_| ())?;
    }
    if let Some((code, reason)) = outcome.close {
        let frame = CloseFrame {
            code,
            reason: reason.into(),
        };
        let _ = ws.close(Some(frame)).await;
        // Let the client answer the close (RFC 6455 §7.1.1), but not forever.
        let _ =
            tokio::time::timeout(CLOSE_WAIT, async { while ws.next().await.is_some() {} }).await;
        return Err(());
    }
    Ok(())
}

fn response(
    req: &envelope::Request,
    result: Result<Map<String, Value>, envelope::Error>,
) -> envelope::Response {
    let (result, error) = match result {
        Ok(r) => (Some(r), None),
        Err(e) => (None, Some(e)),
    };
    envelope::Response {
        id: req.id,
        session_id: req.session_id.clone(),
        result,
        error,
    }
}

/// An ADP error answered by this component for `method`.
fn adp_error(code: ErrorCode, message: impl Into<String>, method: &str) -> envelope::Error {
    to_adp(
        code,
        &Error::new(code, message).with_reason(COMPONENT, method),
    )
}

fn to_adp(code: ErrorCode, e: &Error) -> envelope::Error {
    let data = e.reason.as_ref().map(|r| {
        let mut data = Map::new();
        data.insert("reason".to_owned(), Value::String(r.clone()));
        data
    });
    envelope::Error {
        code: code.as_str().to_owned(),
        message: e.message.clone(),
        data,
    }
}

impl Connection {
    async fn handle(&mut self, text: &str) -> Outcome {
        // A message that cannot be answered, because it has no usable id, closes the connection.
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Outcome::close(CloseCode::Policy, "the message is not JSON");
        };
        let Some(id) = value.get("id").and_then(Value::as_i64) else {
            return Outcome::close(CloseCode::Policy, "the message has no integer id");
        };
        let req: envelope::Request = match serde_json::from_value(value) {
            Ok(req) => req,
            Err(e) => {
                let code = if self.principal.is_some() {
                    ErrorCode::InvalidRequest
                } else {
                    ErrorCode::Unauthenticated
                };
                let error = adp_error(code, format!("not a Request: {e}"), "request");
                return Outcome {
                    response: Some(envelope::Response {
                        id,
                        session_id: None,
                        result: None,
                        error: Some(error),
                    }),
                    ..Outcome::default()
                };
            }
        };
        let span = trace::request_span(&req, self.shared.settings.trace_mode);
        self.request(req).instrument(span).await
    }

    async fn request(&mut self, req: envelope::Request) -> Outcome {
        let method = req.method.as_str();
        let Some(principal) = self.principal.clone() else {
            return self.first_message(req).await;
        };
        let fail = |code, message: &str| Outcome {
            response: Some(response(&req, Err(adp_error(code, message, method)))),
            ..Outcome::default()
        };
        if !(0..=MAX_ID).contains(&req.id) {
            return fail(
                ErrorCode::InvalidRequest,
                "id is not an integer from 0 to 2^53-1",
            );
        }
        if req.session_id.is_some() {
            return fail(
                ErrorCode::InvalidRequest,
                "this connection holds no such session",
            );
        }
        if method == AuthParams::METHOD {
            return fail(
                ErrorCode::InvalidRequest,
                "the connection is already authenticated",
            );
        }
        if method == HelloParams::METHOD {
            if self.hello_done {
                return fail(
                    ErrorCode::InvalidRequest,
                    "Connection.hello was already sent",
                );
            }
            return self.hello(&req);
        }
        if !self.hello_done {
            return fail(ErrorCode::InvalidRequest, "Connection.hello comes first");
        }
        match dispatch::scope(method) {
            None => {
                return fail(
                    ErrorCode::MethodNotFound,
                    "no such command in this protocol version",
                );
            }
            Some(Scope::Session) => {
                return fail(
                    ErrorCode::InvalidRequest,
                    "the command is sent in a session",
                );
            }
            Some(_) => {}
        }
        let mut events = Vec::new();
        let result = dispatch::call(
            method,
            req.params.clone(),
            &principal,
            &self.shared,
            &mut events,
        )
        .await;
        match result {
            Ok(r) => Outcome {
                events,
                response: Some(response(&req, Ok(r))),
                close: None,
            },
            Err(e) => match e.code {
                Some(code) => Outcome {
                    events,
                    response: Some(response(&req, Err(to_adp(code, &e)))),
                    close: None,
                },
                // ADP has no code for it yet: the error cannot be answered on the protocol.
                None => {
                    tracing::error!(error = %e, "an error without an ADP code; closing the connection");
                    Outcome {
                        events,
                        close: Some((CloseCode::Error, "internal error")),
                        ..Outcome::default()
                    }
                }
            },
        }
    }

    /// Before authentication only a well-formed `Connection.auth` is taken.
    async fn first_message(&mut self, req: envelope::Request) -> Outcome {
        let method = req.method.as_str();
        let unauthenticated = |message: &str, close: bool| Outcome {
            response: Some(response(
                &req,
                Err(adp_error(ErrorCode::Unauthenticated, message, method)),
            )),
            close: close.then_some((CloseCode::Policy, "authentication failed")),
            ..Outcome::default()
        };
        if method != AuthParams::METHOD || req.session_id.is_some() {
            return unauthenticated("not authenticated: Connection.auth comes first", false);
        }
        let token = dispatch::params::<AuthParams>(req.params.clone(), method)
            .ok()
            .and_then(|p| BearerToken::new(p.token).ok());
        let authenticated = match token {
            Some(token) => self
                .shared
                .services
                .authenticator
                .authenticate(&Credential::Bearer(token), &self.ctx)
                .await
                .ok(),
            None => None,
        };
        match authenticated {
            Some(principal) => {
                self.principal = Some(principal);
                Outcome {
                    response: Some(response(&req, Ok(Map::new()))),
                    ..Outcome::default()
                }
            }
            None => {
                tracing::info!("Connection.auth failed");
                unauthenticated("the token does not authenticate", true)
            }
        }
    }

    fn hello(&mut self, req: &envelope::Request) -> Outcome {
        let method = req.method.as_str();
        let answer = |result| Outcome {
            response: Some(response(req, result)),
            ..Outcome::default()
        };
        let p = match dispatch::params::<HelloParams>(req.params.clone(), method) {
            Ok(p) => p,
            Err(e) => return answer(Err(to_adp(ErrorCode::InvalidParams, &e))),
        };
        let Ok(client) = semver::Version::parse(&p.protocol_version) else {
            return answer(Err(adp_error(
                ErrorCode::InvalidParams,
                "protocolVersion is not a semantic version",
                method,
            )));
        };
        let server = semver::Version::parse(PROTOCOL_VERSION).expect("PROTOCOL_VERSION is semver");
        // Majors match, and for 0.x also the minors (docs/04-adp.md §9).
        let compatible =
            client.major == server.major && (server.major != 0 || client.minor == server.minor);
        if !compatible {
            let mut e = adp_error(
                ErrorCode::UnsupportedVersion,
                "no protocol version both sides support",
                method,
            );
            e.data.get_or_insert_with(Map::new).insert(
                "protocolVersion".to_owned(),
                Value::String(PROTOCOL_VERSION.to_owned()),
            );
            let mut outcome = answer(Err(e));
            outcome.close = Some((CloseCode::Policy, "unsupported protocol version"));
            return outcome;
        }
        self.hello_done = true;
        answer(Ok(dispatch::object(&HelloReturns {
            protocol_version: PROTOCOL_VERSION.to_owned(),
        })))
    }
}
