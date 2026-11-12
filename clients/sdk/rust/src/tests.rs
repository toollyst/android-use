use droiduse_contracts_adp::target::{CreateTargetParams, GetTargetsParams};

use super::*;
use crate::fake::{CREATE_REASON, FakeServer};

const TOKEN: &str = "t0ken-for-tests";

#[tokio::test]
async fn connect_sends_the_bearer_header_and_hello_first() {
    let fake = FakeServer::start(TOKEN, false).await;
    let client = Client::connect(&fake.url, TOKEN, None).await.unwrap();
    assert_eq!(client.server_version(), PROTOCOL_VERSION);
    assert_eq!(fake.authorization(), [Some(format!("Bearer {TOKEN}"))]);
    let reqs = fake.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["method"], "Connection.hello");
    assert_eq!(reqs[0]["params"]["protocolVersion"], PROTOCOL_VERSION);
    client.close().await.unwrap();
}

#[tokio::test]
async fn a_refused_upgrade_is_an_error() {
    let fake = FakeServer::start(TOKEN, false).await;
    let err = Client::connect(&fake.url, "wrong", None)
        .await
        .err()
        .unwrap();
    assert!(
        matches!(&err, Error::WebSocket(tungstenite::Error::Http(r)) if r.status() == 401),
        "{err}"
    );
}

#[tokio::test]
async fn responses_are_matched_by_id_and_events_do_not_break_it() {
    // The fake answers the two calls in reverse order, and sends an event before one of them.
    let fake = FakeServer::start(TOKEN, true).await;
    let client = Client::connect(&fake.url, TOKEN, None).await.unwrap();
    let (targets, created) = tokio::join!(
        client.call(GetTargetsParams {}),
        client.call(CreateTargetParams {})
    );
    assert_eq!(targets.unwrap().target_infos, []);
    assert!(matches!(created, Err(Error::Adp(_))));
    client.close().await.unwrap();
}

#[tokio::test]
async fn an_adp_error_carries_code_message_and_reason() {
    let fake = FakeServer::start(TOKEN, false).await;
    let client = Client::connect(&fake.url, TOKEN, None).await.unwrap();
    let Err(Error::Adp(e)) = client.call(CreateTargetParams {}).await else {
        panic!("createTarget must fail");
    };
    assert_eq!(e.code, "UNIMPLEMENTED");
    assert_eq!(e.known_code(), Some(ErrorCode::Unimplemented));
    assert_eq!(e.reason.as_deref(), Some(CREATE_REASON));
    assert!(!e.message.is_empty());
}

#[tokio::test]
async fn traceparent_is_on_the_wire_only_when_given() {
    let fake = FakeServer::start(TOKEN, false).await;
    let tp = new_traceparent().unwrap();
    let client = Client::connect(&fake.url, TOKEN, Some(tp)).await.unwrap();
    client
        .call_traced(GetTargetsParams {}, Some(tp))
        .await
        .unwrap();
    client.call(GetTargetsParams {}).await.unwrap();
    let reqs = fake.requests();
    for r in &reqs[..2] {
        let wire = r["traceparent"].as_str().expect("traceparent sent");
        assert_eq!(TraceParent::parse(wire).unwrap(), tp);
    }
    assert!(reqs[2].get("traceparent").is_none(), "{}", reqs[2]);
    assert!(tp.flags().is_sampled());
    assert_ne!(new_traceparent().unwrap().trace_id(), tp.trace_id());
}

#[test]
fn the_token_file_content_is_trimmed() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("token");
    std::fs::write(&f, format!("{TOKEN}\n")).unwrap();
    assert_eq!(read_token_file(&f).unwrap(), TOKEN);
    std::fs::write(&f, "\n").unwrap();
    assert!(read_token_file(&f).is_err());
    assert!(read_token_file(&dir.path().join("absent")).is_err());
}
