//! The server over a real socket: authentication, hello, the envelope rules, the Target commands.

mod common;

use common::{call, close_code, connect, connect_hello, connect_with, start};
use droiduse_entrypoints_adp_server::TraceMode;
use serde_json::json;
use tokio_tungstenite::tungstenite;

fn http_status(e: tungstenite::Error) -> tungstenite::http::Response<Option<Vec<u8>>> {
    match e {
        tungstenite::Error::Http(resp) => *resp,
        other => panic!("expected an HTTP refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_bad_authorization_header_gets_401() {
    let r = start(TraceMode::default()).await;
    for value in ["Bearer wrong-token", "Basic abc", "Bearer"] {
        let resp = http_status(
            connect_with(&r.url, &[("authorization", value)])
                .await
                .unwrap_err(),
        );
        assert_eq!(resp.status(), 401, "{value}");
        assert_eq!(
            resp.headers()["www-authenticate"],
            "Bearer error=\"invalid_token\""
        );
    }
}

#[tokio::test]
async fn an_origin_gets_403_and_a_query_gets_400() {
    let r = start(TraceMode::default()).await;
    let bearer = format!("Bearer {}", r.token);
    let headers = [
        ("authorization", bearer.as_str()),
        ("origin", "https://example.com"),
    ];
    let resp = http_status(connect_with(&r.url, &headers).await.unwrap_err());
    assert_eq!(resp.status(), 403);

    let url = format!("{}?access_token={}", r.url, r.token);
    let resp = http_status(connect_with(&url, &[]).await.unwrap_err());
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn header_auth_then_hello_then_get_targets_is_empty() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect_hello(&r).await;
    let res = call(&mut ws, json!({"id": 2, "method": "Target.getTargets"})).await;
    assert_eq!(res, json!({"id": 2, "result": {"targetInfos": []}}));
}

#[tokio::test]
async fn first_message_auth_path() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect_with(&r.url, &[]).await.unwrap();
    let res = call(
        &mut ws,
        json!({"id": 1, "method": "Connection.auth", "params": {"token": r.token}}),
    )
    .await;
    assert_eq!(res, json!({"id": 1, "result": {}}));
    let res = call(
        &mut ws,
        json!({"id": 2, "method": "Connection.hello", "params": {"protocolVersion": "0.1.3"}}),
    )
    .await;
    assert_eq!(res["result"]["protocolVersion"], "0.1.0");
    let res = call(
        &mut ws,
        json!({"id": 3, "method": "Target.getTargets", "params": {}}),
    )
    .await;
    assert_eq!(res["result"]["targetInfos"], json!([]));
    // A connection is authenticated once.
    let res = call(
        &mut ws,
        json!({"id": 4, "method": "Connection.auth", "params": {"token": r.token}}),
    )
    .await;
    assert_eq!(res["error"]["code"], "INVALID_REQUEST");
}

#[tokio::test]
async fn a_command_before_auth_is_unauthenticated() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect_with(&r.url, &[]).await.unwrap();
    let res = call(&mut ws, json!({"id": 1, "method": "Target.getTargets"})).await;
    assert_eq!(res["id"], 1);
    assert_eq!(res["error"]["code"], "UNAUTHENTICATED");
    let res = call(
        &mut ws,
        json!({"id": 2, "method": "Connection.hello", "params": {"protocolVersion": "0.1.0"}}),
    )
    .await;
    assert_eq!(res["error"]["code"], "UNAUTHENTICATED");
    // The connection stays open for Connection.auth.
    let res = call(
        &mut ws,
        json!({"id": 3, "method": "Connection.auth", "params": {"token": r.token}}),
    )
    .await;
    assert_eq!(res["result"], json!({}));
}

#[tokio::test]
async fn a_failed_connection_auth_closes_with_1008() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect_with(&r.url, &[]).await.unwrap();
    let res = call(
        &mut ws,
        json!({"id": 1, "method": "Connection.auth", "params": {"token": "wrong"}}),
    )
    .await;
    assert_eq!(res["error"]["code"], "UNAUTHENTICATED");
    assert_eq!(close_code(&mut ws).await, 1008);
}

#[tokio::test]
async fn create_target_answers_unimplemented_from_the_vmm() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect_hello(&r).await;
    let res = call(&mut ws, json!({"id": 2, "method": "Target.createTarget"})).await;
    assert_eq!(res["id"], 2);
    assert_eq!(res["error"]["code"], "UNIMPLEMENTED", "{res}");
    assert_eq!(res["error"]["data"]["reason"], "host/vmm/api#create");
    assert!(res.get("result").is_none());
    // No instance remains.
    let res = call(&mut ws, json!({"id": 3, "method": "Target.getTargets"})).await;
    assert_eq!(res["result"]["targetInfos"], json!([]));
}

#[tokio::test]
async fn the_envelope_rules() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect(&r).await;
    let res = call(&mut ws, json!({"id": 1, "method": "Target.getTargets"})).await;
    assert_eq!(
        res["error"]["code"], "INVALID_REQUEST",
        "before hello: {res}"
    );

    let hello =
        json!({"id": 2, "method": "Connection.hello", "params": {"protocolVersion": "0.1.0"}});
    assert!(call(&mut ws, hello.clone()).await.get("result").is_some());
    let res = call(&mut ws, hello).await;
    assert_eq!(res["error"]["code"], "INVALID_REQUEST", "second hello");

    let cases = [
        (
            json!({"id": 3, "method": "Nope.nothing"}),
            "METHOD_NOT_FOUND",
            None,
        ),
        (
            json!({"id": 4, "method": "Target.attachToTarget", "params": {"targetId": "t"}}),
            "UNIMPLEMENTED",
            Some("host/entrypoints/adp_server#Target.attachToTarget"),
        ),
        (
            json!({"id": 5, "method": "UI.getTree", "params": {}}),
            "INVALID_REQUEST",
            None,
        ),
        (
            json!({"id": 6, "sessionId": "s", "method": "Target.getTargets"}),
            "INVALID_REQUEST",
            None,
        ),
        (
            json!({"id": 7, "method": "Target.closeTarget", "params": {}}),
            "INVALID_PARAMS",
            None,
        ),
        (
            json!({"id": 8, "method": "Target.getTargets", "params": {"extra": 1}}),
            "INVALID_PARAMS",
            None,
        ),
        (
            json!({"id": 9, "method": "Target.closeTarget", "params": {"targetId": "none"}}),
            "TARGET_GONE",
            Some("host/entrypoints/adp_server#Target.closeTarget"),
        ),
        (
            json!({"id": -1, "method": "Target.getTargets"}),
            "INVALID_REQUEST",
            None,
        ),
        (
            json!({"id": 10, "method": "Target.getTargets", "x": 1}),
            "INVALID_REQUEST",
            None,
        ),
    ];
    for (req, code, reason) in cases {
        let res = call(&mut ws, req.clone()).await;
        assert_eq!(res["id"], req["id"]);
        assert_eq!(res["error"]["code"], code, "{req} -> {res}");
        if let Some(reason) = reason {
            assert_eq!(res["error"]["data"]["reason"], reason, "{req} -> {res}");
        }
    }
    let res = call(
        &mut ws,
        json!({"id": 6, "sessionId": "s", "method": "Target.getTargets"}),
    )
    .await;
    assert_eq!(res["sessionId"], "s", "a response echoes the sessionId");
}

#[tokio::test]
async fn an_incompatible_hello_closes_with_1008() {
    let r = start(TraceMode::default()).await;
    let mut ws = connect(&r).await;
    let res = call(
        &mut ws,
        json!({"id": 1, "method": "Connection.hello", "params": {"protocolVersion": "0.2.0"}}),
    )
    .await;
    assert_eq!(res["error"]["code"], "UNSUPPORTED_VERSION");
    assert_eq!(res["error"]["data"]["protocolVersion"], "0.1.0");
    assert_eq!(close_code(&mut ws).await, 1008);
}
