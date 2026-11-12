//! The `droiduse-mcp` binary as an MCP client starts it (a child process over stdio),
//! against the SDK's fake ADP server.

use droiduse_sdk::fake::{CREATE_REASON, FakeServer};
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::{ServiceExt, serde_json};

const TOKEN: &str = "mcp-test-token";

async fn start() -> (
    FakeServer,
    tempfile::TempDir,
    RunningService<RoleClient, ()>,
) {
    let fake = FakeServer::start(TOKEN, false).await;
    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("token");
    std::fs::write(&token_file, format!("{TOKEN}\n")).unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_droiduse-mcp"));
    cmd.arg("--adp-url")
        .arg(&fake.url)
        .arg("--token-file")
        .arg(&token_file);
    let transport = TokioChildProcess::new(cmd).unwrap();
    let client = ().serve(transport).await.unwrap();
    (fake, dir, client)
}

#[tokio::test]
async fn tools_list_has_the_instance_tools() {
    let (_fake, _dir, client) = start().await;
    let mut names: Vec<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["instance_close", "instance_create", "instance_list"]
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn instance_create_maps_the_adp_error_with_code_and_reason() {
    let (fake, _dir, client) = start().await;
    let r = client
        .call_tool(CallToolRequestParams::new("instance_create"))
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(true), "{r:?}");
    let s = r.structured_content.clone().expect("structured error");
    assert_eq!(s["code"], "UNIMPLEMENTED");
    assert_eq!(s["reason"], CREATE_REASON);
    let text = &r.content[0].as_text().expect("text content").text;
    assert!(
        text.contains("UNIMPLEMENTED") && text.contains(CREATE_REASON),
        "{text}"
    );
    // one connection: bearer header, hello, then the command, both with the call's traceparent
    assert_eq!(fake.authorization(), [Some(format!("Bearer {TOKEN}"))]);
    let reqs = fake.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[1]["method"], "Target.createTarget");
    assert!(reqs[0]["traceparent"].is_string());
    assert_eq!(reqs[0]["traceparent"], reqs[1]["traceparent"]);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn instance_list_returns_the_targets() {
    let (_fake, _dir, client) = start().await;
    let r = client
        .call_tool(CallToolRequestParams::new("instance_list"))
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(false), "{r:?}");
    assert_eq!(
        r.structured_content,
        Some(serde_json::json!({ "targetInfos": [] }))
    );
    client.cancel().await.unwrap();
}
