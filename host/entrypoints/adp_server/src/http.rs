//! The HTTP/1.1 side: the WebSocket upgrade (RFC 6455 §4.2), with authentication before the 101.

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use droiduse_contracts_spi::{BearerToken, ChannelCtx, Credential, Principal};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{self, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};

use crate::Shared;
use crate::connection;

/// Serves one accepted stream: one upgrade request (or an HTTP error), then the WebSocket.
pub(crate) async fn serve<IO>(io: IO, ctx: ChannelCtx, shared: Arc<Shared>)
where
    IO: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let service = service_fn(move |req| {
        let (ctx, shared) = (ctx.clone(), shared.clone());
        async move { Ok::<_, Infallible>(upgrade(req, ctx, shared).await) }
    });
    if let Err(e) = http1::Builder::new()
        .serve_connection(io, service)
        .with_upgrades()
        .await
    {
        tracing::debug!(error = %e, "HTTP connection ended with an error");
    }
}

fn status(code: StatusCode) -> Response<Full<Bytes>> {
    let mut r = Response::new(Full::new(Bytes::new()));
    *r.status_mut() = code;
    r
}

fn header_has_token(req: &Request<Incoming>, name: header::HeaderName, token: &str) -> bool {
    req.headers().get_all(name).iter().any(|v| {
        v.to_str()
            .is_ok_and(|s| s.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
    })
}

/// The token of `Authorization: Bearer <token>` (RFC 6750 §2.1). `None` for any other form.
fn bearer(req: &Request<Incoming>) -> Option<BearerToken> {
    let mut values = req.headers().get_all(header::AUTHORIZATION).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return None;
    };
    let value = value.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    BearerToken::new(token.trim_start_matches(' ').to_owned()).ok()
}

async fn upgrade(
    mut req: Request<Incoming>,
    ctx: ChannelCtx,
    shared: Arc<Shared>,
) -> Response<Full<Bytes>> {
    // The request URI is never logged: a client might have put a token in its query.
    if req.uri().path() != "/" {
        return status(StatusCode::NOT_FOUND);
    }
    if req.uri().query().is_some() {
        // A token in the query string is not accepted (docs/04-adp.md §4); nothing else uses it.
        return status(StatusCode::BAD_REQUEST);
    }
    let key = req.headers().get(header::SEC_WEBSOCKET_KEY).cloned();
    let is_upgrade = req.method() == Method::GET
        && header_has_token(&req, header::CONNECTION, "upgrade")
        && header_has_token(&req, header::UPGRADE, "websocket");
    let (true, Some(key)) = (is_upgrade, key) else {
        return status(StatusCode::BAD_REQUEST);
    };
    if req.headers().get(header::SEC_WEBSOCKET_VERSION) != Some(&HeaderValue::from_static("13")) {
        let mut r = status(StatusCode::UPGRADE_REQUIRED);
        r.headers_mut().insert(
            header::SEC_WEBSOCKET_VERSION,
            HeaderValue::from_static("13"),
        );
        return r;
    }
    // No browser client is allowed yet (RFC 6455 §4.2.2 and §10.2: refuse an origin with 403).
    if req.headers().contains_key(header::ORIGIN) {
        tracing::info!("refused an upgrade that carries an Origin");
        return status(StatusCode::FORBIDDEN);
    }

    let principal: Option<Principal> = if req.headers().contains_key(header::AUTHORIZATION) {
        let authenticated = match bearer(&req) {
            Some(token) => shared
                .services
                .authenticator
                .authenticate(&Credential::Bearer(token), &ctx)
                .await
                .ok(),
            None => None,
        };
        match authenticated {
            Some(p) => Some(p),
            None => {
                tracing::info!("refused an upgrade whose Authorization does not authenticate");
                let mut r = status(StatusCode::UNAUTHORIZED);
                r.headers_mut().insert(
                    header::WWW_AUTHENTICATE,
                    HeaderValue::from_static("Bearer error=\"invalid_token\""),
                );
                return r;
            }
        }
    } else {
        None
    };

    let on_upgrade = hyper::upgrade::on(&mut req);
    let config = WebSocketConfig::default()
        .max_message_size(Some(shared.settings.max_message_bytes))
        .max_frame_size(Some(shared.settings.max_message_bytes));
    tokio::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                let ws = WebSocketStream::from_raw_socket(
                    TokioIo::new(upgraded),
                    Role::Server,
                    Some(config),
                )
                .await;
                connection::run(ws, principal, ctx, shared).await;
            }
            Err(e) => tracing::debug!(error = %e, "the upgrade did not complete"),
        }
    });

    let mut r = status(StatusCode::SWITCHING_PROTOCOLS);
    let h = r.headers_mut();
    h.insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
    h.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    h.insert(
        header::SEC_WEBSOCKET_ACCEPT,
        HeaderValue::from_str(&derive_accept_key(key.as_bytes()))
            .expect("base64 is a valid header value"),
    );
    r
}
