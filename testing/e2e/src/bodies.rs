//! Scenario bodies: each drives the real component processes over the wire. A scenario
//! without a body here ends at the shared `missing:testing/e2e#scenario-body` step.

use std::ffi::OsString;
use std::time::Duration;

use serde_json::{Value, json};

use crate::harness::{
    Fallible, Mcp, Server, Ws, call, check, close_code, connect, crate_version, error_code, hello,
    refused_status,
};
use crate::manifest::{Reason, Scenario};
use crate::run::Observed;

type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// The body of `s`, run to completion or for at most `limit`; `None` if `s` has no body.
pub fn run(s: &Scenario, env: Env<'_>, limit: Duration) -> Option<Observed> {
    let id = s.id.as_str();
    if !BODIES.contains(&id) {
        return None;
    }
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return Some(Observed::Fail(format!("tokio runtime: {e}"))),
    };
    // Dropping the body (done, failed or timed out) drops its child processes, which kills them.
    let outcome = rt.block_on(async { tokio::time::timeout(limit, body(id, env)).await });
    Some(match outcome {
        Ok(Ok(o)) => o,
        Ok(Err(m)) => Observed::Fail(m),
        Err(_) => Observed::Fail(format!(
            "the body did not finish within {} s",
            limit.as_secs()
        )),
    })
}

const BODIES: &[&str] = &[
    "adp.auth.upgrade-bearer",
    "adp.auth.upgrade-invalid-token",
    "adp.auth.first-message",
    "adp.auth.first-message-invalid",
    "adp.auth.before-auth-rejected",
    "adp.auth.query-token-rejected",
    "adp.auth.second-auth-rejected",
    "adp.auth.token-file",
    "adp.hello.required-first",
    "adp.hello.second-rejected",
    "adp.hello.compatible",
    "adp.hello.unsupported-version",
    "adp.envelope.malformed",
    "adp.envelope.id-range",
    "adp.envelope.session-scope",
    "adp.envelope.unknown-method",
    "adp.envelope.invalid-params",
    "adp.close.uncorrelated-binary",
    "trace.adp.traceparent-linked",
    "trace.adp.traceparent-invalid-ignored",
    "mcp.stdio.initialize",
    "mcp.tools.list",
    "gate.probe.unimplemented",
    "gate.probe.one-trace",
];

async fn body(id: &str, env: Env<'_>) -> Fallible<Observed> {
    let server = Server::start(env).await?;
    match id {
        "adp.auth.upgrade-bearer" => upgrade_bearer(&server).await,
        "adp.auth.upgrade-invalid-token" => upgrade_invalid_token(&server).await,
        "adp.auth.first-message" => first_message(&server).await,
        "adp.auth.first-message-invalid" => first_message_invalid(&server).await,
        "adp.auth.before-auth-rejected" => before_auth(&server).await,
        "adp.auth.query-token-rejected" => query_token(&server).await,
        "adp.auth.second-auth-rejected" => second_auth(&server).await,
        "adp.auth.token-file" => token_file(&server).await,
        "adp.hello.required-first" => hello_required_first(&server).await,
        "adp.hello.second-rejected" => hello_second(&server).await,
        "adp.hello.compatible" => hello_compatible(&server).await,
        "adp.hello.unsupported-version" => hello_unsupported(&server).await,
        "adp.envelope.malformed" => malformed(&server).await,
        "adp.envelope.id-range" => id_range(&server).await,
        "adp.envelope.session-scope" => session_scope(&server).await,
        "adp.envelope.unknown-method" => unknown_method(&server).await,
        "adp.envelope.invalid-params" => invalid_params(&server).await,
        "adp.close.uncorrelated-binary" => uncorrelated_binary(&server).await,
        "trace.adp.traceparent-linked" => traceparent_linked(&server).await,
        "trace.adp.traceparent-invalid-ignored" => traceparent_invalid(&server).await,
        "mcp.stdio.initialize" => mcp_initialize(&server, env).await,
        "mcp.tools.list" => mcp_tools_list(&server, env).await,
        "gate.probe.unimplemented" => probe_unimplemented(&server, env).await,
        "gate.probe.one-trace" => probe_one_trace(&server, env).await,
        other => Err(format!("no body for {other}")),
    }
}

fn red(reason: &str) -> Fallible<Observed> {
    let reason = Reason::parse(reason)?;
    Ok(Observed::Red {
        reason: reason.clone(),
        unmet: vec![reason],
    })
}

/// `res` is the error `code` for request `id`.
fn expect_error(res: &Value, id: i64, code: &str, what: &str) -> Fallible<()> {
    check(res["id"] == id && error_code(res) == code, || {
        format!("{what}: expected {code} for id {id}, got {res}")
    })
}

/// `res` is a result for request `id`.
fn expect_result(res: &Value, id: i64, what: &str) -> Fallible<()> {
    check(res["id"] == id && res.get("result").is_some(), || {
        format!("{what}: expected a result for id {id}, got {res}")
    })
}

fn get_targets(id: i64) -> Value {
    json!({"id": id, "method": "Target.getTargets"})
}

/// Target.getTargets answers an empty list.
async fn targets_empty(ws: &mut Ws, id: i64) -> Fallible<()> {
    let res = call(ws, get_targets(id)).await?;
    check(
        res == json!({"id": id, "result": {"targetInfos": []}}),
        || format!("Target.getTargets: expected an empty list, got {res}"),
    )
}

/// The shared first step of a scenario that needs an instance: create one and expect it to
/// work. While the layer that answers is not built, its UNIMPLEMENTED reason is the RED.
async fn create_instance(ws: &mut Ws, id: i64) -> Fallible<Observed> {
    let res = call(ws, json!({"id": id, "method": "Target.createTarget"})).await?;
    if res.get("result").is_some() {
        // The rest of the scenario is not written yet.
        return Ok(crate::run::body_not_written());
    }
    let reason = res["error"]["data"]["reason"].as_str().unwrap_or("");
    if error_code(&res) == Reason::CODE && !reason.is_empty() {
        return red(reason);
    }
    Err(format!("Target.createTarget: {res}"))
}

async fn upgrade_bearer(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    targets_empty(&mut ws, 2).await?;
    Ok(Observed::Pass)
}

async fn upgrade_invalid_token(s: &Server) -> Fallible<Observed> {
    for value in ["Bearer not-the-token", "Basic dXNlcjpwYXNz", "Bearer"] {
        let resp = refused_status(connect(&s.url, &[("authorization", value)]).await)?;
        check(resp.status() == 401, || {
            format!("{value:?}: status {}", resp.status())
        })?;
        let challenge = resp.headers().get("www-authenticate");
        check(
            challenge.is_some_and(|v| v == "Bearer error=\"invalid_token\""),
            || format!("{value:?}: WWW-Authenticate {challenge:?}"),
        )?;
    }
    Ok(Observed::Pass)
}

fn auth(id: i64, token: &str) -> Value {
    json!({"id": id, "method": "Connection.auth", "params": {"token": token}})
}

async fn first_message(s: &Server) -> Fallible<Observed> {
    let mut ws = connect(&s.url, &[]).await.map_err(|e| e.to_string())?;
    let res = call(&mut ws, auth(1, &s.token)).await?;
    check(res == json!({"id": 1, "result": {}}), || {
        format!("Connection.auth: {res}")
    })?;
    expect_result(&call(&mut ws, hello(2, crate_version())).await?, 2, "hello")?;
    targets_empty(&mut ws, 3).await?;
    Ok(Observed::Pass)
}

async fn first_message_invalid(s: &Server) -> Fallible<Observed> {
    let mut ws = connect(&s.url, &[]).await.map_err(|e| e.to_string())?;
    let res = call(&mut ws, auth(1, "not-the-token")).await?;
    expect_error(&res, 1, "UNAUTHENTICATED", "a wrong Connection.auth")?;
    let code = close_code(&mut ws).await?;
    check(code == 1008, || {
        format!("close status {code}, expected 1008")
    })?;
    Ok(Observed::Pass)
}

async fn before_auth(s: &Server) -> Fallible<Observed> {
    let mut ws = connect(&s.url, &[]).await.map_err(|e| e.to_string())?;
    let requests = [
        get_targets(1),
        hello(2, crate_version()),
        json!({"id": 3, "method": "Nope.nothing"}),
        json!({"id": 4, "method": "Target.createTarget"}),
    ];
    for req in requests {
        let id = req["id"].as_i64().expect("an id");
        let res = call(&mut ws, req).await?;
        expect_error(&res, id, "UNAUTHENTICATED", "before authentication")?;
    }
    // Still open for Connection.auth.
    let res = call(&mut ws, auth(5, &s.token)).await?;
    expect_result(&res, 5, "Connection.auth after the rejected messages")?;
    Ok(Observed::Pass)
}

async fn query_token(s: &Server) -> Fallible<Observed> {
    for query in ["access_token", "token"] {
        let url = format!("{}?{query}={}", s.url, s.token);
        match connect(&url, &[]).await {
            Err(_) => {}
            Ok(mut ws) => {
                // Upgraded without a credential: the query must not have authenticated.
                let res = call(&mut ws, get_targets(1)).await?;
                expect_error(&res, 1, "UNAUTHENTICATED", &format!("token in ?{query}="))?;
            }
        }
    }
    Ok(Observed::Pass)
}

async fn second_auth(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    let res = call(&mut ws, auth(2, &s.token)).await?;
    expect_error(&res, 2, "INVALID_REQUEST", "a second Connection.auth")?;
    targets_empty(&mut ws, 3).await?;
    Ok(Observed::Pass)
}

async fn token_file(s: &Server) -> Fallible<Observed> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&s.token_file)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        check(mode & 0o077 == 0, || {
            format!("token file mode {:o}", mode & 0o777)
        })?;
    }
    // On Windows the file has the ACL of its directory; this run does not check it.
    let mut ws = s.ready().await?;
    targets_empty(&mut ws, 2).await?;
    Ok(Observed::Pass)
}

async fn authenticated(s: &Server) -> Fallible<Ws> {
    connect(&s.url, &[("authorization", &s.bearer())])
        .await
        .map_err(|e| format!("upgrade with the token: {e}"))
}

async fn hello_required_first(s: &Server) -> Fallible<Observed> {
    let mut ws = authenticated(s).await?;
    let res = call(&mut ws, get_targets(1)).await?;
    expect_error(&res, 1, "INVALID_REQUEST", "a command before hello")?;
    expect_result(&call(&mut ws, hello(2, crate_version())).await?, 2, "hello")?;
    Ok(Observed::Pass)
}

async fn hello_second(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    let res = call(&mut ws, hello(2, crate_version())).await?;
    expect_error(&res, 2, "INVALID_REQUEST", "a second hello")?;
    Ok(Observed::Pass)
}

async fn hello_compatible(s: &Server) -> Fallible<Observed> {
    let v: Vec<&str> = crate_version().split('.').collect();
    let compatible = format!("{}.{}.999", v[0], v[1]);
    let mut ws = authenticated(s).await?;
    let res = call(&mut ws, hello(1, &compatible)).await?;
    check(res["result"]["protocolVersion"] == crate_version(), || {
        format!("hello {compatible}: {res}")
    })?;
    Ok(Observed::Pass)
}

async fn hello_unsupported(s: &Server) -> Fallible<Observed> {
    let mut ws = authenticated(s).await?;
    let res = call(&mut ws, hello(1, "999.0.0")).await?;
    expect_error(&res, 1, "UNSUPPORTED_VERSION", "hello 999.0.0")?;
    check(
        res["error"]["data"]["protocolVersion"] == crate_version(),
        || format!("error.data.protocolVersion: {res}"),
    )?;
    let code = close_code(&mut ws).await?;
    check(code == 1008, || {
        format!("close status {code}, expected 1008")
    })?;
    Ok(Observed::Pass)
}

async fn malformed(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    for (id, req) in [
        (2, json!({"id": 2})),
        (3, json!({"id": 3, "method": 7})),
        (
            4,
            json!({"id": 4, "method": "Target.getTargets", "params": []}),
        ),
        (
            5,
            json!({"id": 5, "method": "Target.getTargets", "unknown": true}),
        ),
    ] {
        let res = call(&mut ws, req.clone()).await?;
        expect_error(&res, id, "INVALID_REQUEST", &req.to_string())?;
    }
    Ok(Observed::Pass)
}

async fn id_range(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    for id in [-1_i64, 1 << 53] {
        let res = call(&mut ws, get_targets(id)).await?;
        expect_error(&res, id, "INVALID_REQUEST", &format!("id {id}"))?;
    }
    targets_empty(&mut ws, (1 << 53) - 1).await?;
    Ok(Observed::Pass)
}

async fn session_scope(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    let cases = [
        // a sessionId on a connection-scoped command
        json!({"id": 2, "sessionId": "s1", "method": "Target.createTarget"}),
        // none on a session-scoped command
        json!({"id": 3, "method": "UI.getTree", "params": {"sources": ["view"], "depth": -1}}),
        // a sessionId this connection does not hold
        json!({"id": 4, "sessionId": "not-held", "method": "Target.getTargets"}),
    ];
    for req in cases {
        let id = req["id"].as_i64().expect("an id");
        let res = call(&mut ws, req.clone()).await?;
        expect_error(&res, id, "INVALID_REQUEST", &req.to_string())?;
    }
    Ok(Observed::Pass)
}

async fn unknown_method(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    let res = call(&mut ws, json!({"id": 2, "method": "Nope.nothing"})).await?;
    expect_error(&res, 2, "METHOD_NOT_FOUND", "Nope.nothing")?;
    Ok(Observed::Pass)
}

async fn invalid_params(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    for (id, params) in [(2, json!({})), (3, json!({"targetId": 5}))] {
        let req = json!({"id": id, "method": "Target.closeTarget", "params": params});
        let res = call(&mut ws, req.clone()).await?;
        expect_error(&res, id, "INVALID_PARAMS", &req.to_string())?;
    }
    // An unknown enum value needs a command with enum parameters, all of which are sent in the
    // session of a target: that needs an instance first.
    create_instance(&mut ws, 4).await
}

async fn uncorrelated_binary(s: &Server) -> Fallible<Observed> {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    let mut ws = s.ready().await?;
    ws.send(Message::binary(vec![0u8; 16]))
        .await
        .map_err(|e| format!("send binary: {e}"))?;
    let code = close_code(&mut ws).await?;
    check(code == 1002, || {
        format!("close status {code}, expected 1002")
    })?;
    Ok(Observed::Pass)
}

// ---------------------------------------------------------------------------------------
// Trace, read from the server's JSON log
// ---------------------------------------------------------------------------------------

const REQUEST_SPAN: &str = "host/entrypoints/adp_server#request";

/// The request span of a span-close line: the closed span or one enclosing it.
fn request_span(line: &Value) -> Option<&Value> {
    std::iter::once(&line["span"])
        .chain(line["spans"].as_array().into_iter().flatten())
        .find(|s| s["name"] == REQUEST_SPAN)
}

/// The span-close lines of the spans named `name`.
fn closed<'a>(log: &'a [Value], name: &str) -> Vec<&'a Value> {
    log.iter()
        .filter(|l| l["fields"]["message"] == "close" && l["span"]["name"] == name)
        .collect()
}

/// Waits until `f` finds something in the server log (stderr arrives asynchronously).
async fn in_log<T>(s: &Server, what: &str, f: impl Fn(&[Value]) -> Option<T>) -> Fallible<T> {
    for _ in 0..50 {
        if let Some(t) = f(&s.log.json()) {
            return Ok(t);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("not in the server log: {what}"))
}

fn is_trace_id(v: &Value) -> bool {
    v.as_str().is_some_and(|t| {
        t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()) && t.bytes().any(|b| b != b'0')
    })
}

const CLIENT_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

async fn traceparent_linked(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    let mut req = get_targets(2);
    req["traceparent"] = json!(format!("00-{CLIENT_TRACE}-00f067aa0ba902b7-01"));
    expect_result(
        &call(&mut ws, req).await?,
        2,
        "getTargets with a traceparent",
    )?;
    let span = in_log(s, "the request span of id 2", |log| {
        closed(log, REQUEST_SPAN)
            .into_iter()
            .find(|l| l["span"]["id"] == 2)
            .map(|l| l["span"].clone())
    })
    .await?;
    check(
        span["link_trace_id"] == CLIENT_TRACE
            && is_trace_id(&span["trace_id"])
            && span["trace_id"] != CLIENT_TRACE,
        || format!("expected a new trace linked to {CLIENT_TRACE}: {span}"),
    )?;
    Ok(Observed::Pass)
}

async fn traceparent_invalid(s: &Server) -> Fallible<Observed> {
    let mut ws = s.ready().await?;
    let invalid = [
        "not-a-traceparent".to_owned(),
        format!("00-{}-00f067aa0ba902b7-01", "0".repeat(32)),
        format!("00-{CLIENT_TRACE}-{}-01", "0".repeat(16)),
        format!("ff-{CLIENT_TRACE}-00f067aa0ba902b7-01"),
    ];
    for (i, tp) in invalid.iter().enumerate() {
        let id = 10 + i as i64;
        let mut req = get_targets(id);
        req["traceparent"] = json!(tp);
        req["tracestate"] = json!("a=1");
        expect_result(
            &call(&mut ws, req).await?,
            id,
            &format!("traceparent {tp:?}"),
        )?;
        let span = in_log(s, &format!("the request span of id {id}"), |log| {
            closed(log, REQUEST_SPAN)
                .into_iter()
                .find(|l| l["span"]["id"] == id)
                .map(|l| l["span"].clone())
        })
        .await?;
        check(
            is_trace_id(&span["trace_id"])
                && span["trace_id"] != CLIENT_TRACE
                && span.get("link_trace_id").is_none(),
            || format!("traceparent {tp:?}: expected a new unlinked trace, got {span}"),
        )?;
    }
    Ok(Observed::Pass)
}

// ---------------------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------------------

async fn mcp_initialize(s: &Server, env: Env<'_>) -> Fallible<Observed> {
    let mcp = Mcp::start(env, &s.url, &s.token_file).await?;
    let info = mcp.client.peer_info();
    check(info.is_some(), || {
        "no server info after initialize".to_owned()
    })?;
    let _ = mcp.client.cancel().await;
    Ok(Observed::Pass)
}

async fn mcp_tools_list(s: &Server, env: Env<'_>) -> Fallible<Observed> {
    let mcp = Mcp::start(env, &s.url, &s.token_file).await?;
    let tools = mcp
        .client
        .list_all_tools()
        .await
        .map_err(|e| format!("tools/list: {e}"))?;
    let _ = mcp.client.cancel().await;
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    for built in ["instance_list", "instance_create", "instance_close"] {
        check(names.iter().any(|n| n == built), || {
            format!("{built} missing: {names:?}")
        })?;
    }
    // The documented set, in the order of the scenario title.
    for (tool, prefix) in [
        ("observe", false),
        ("act", false),
        ("wait_for", false),
        ("screenshot", false),
        ("app", true),
        ("session", true),
    ] {
        let present = names.iter().any(|n| {
            if prefix {
                n.starts_with(&format!("{tool}_"))
            } else {
                n == tool
            }
        });
        if !present {
            let surface = if prefix {
                format!("{tool}-tools")
            } else {
                tool.to_owned()
            };
            return red(&format!("missing:clients/mcp#{surface}"));
        }
    }
    Ok(Observed::Pass)
}

/// Calls the MCP tool `instance_create`; its structured error.
async fn mcp_instance_create(mcp: &Mcp) -> Fallible<Value> {
    let r = mcp
        .client
        .call_tool(rmcp::model::CallToolRequestParams::new("instance_create"))
        .await
        .map_err(|e| format!("tools/call instance_create: {e}"))?;
    check(r.is_error == Some(true), || {
        format!("instance_create did not fail: {r:?}")
    })?;
    let shown = format!("{r:?}");
    r.structured_content
        .ok_or(format!("instance_create: no structured error: {shown}"))
}

async fn probe_unimplemented(s: &Server, env: Env<'_>) -> Fallible<Observed> {
    let mcp = Mcp::start(env, &s.url, &s.token_file).await?;
    let e = mcp_instance_create(&mcp).await?;
    let _ = mcp.client.cancel().await;
    check(
        e["code"] == "UNIMPLEMENTED" && e["reason"] == "host/vmm/api#create",
        || format!("instance_create: expected UNIMPLEMENTED from host/vmm/api#create, got {e}"),
    )?;
    Ok(Observed::Pass)
}

async fn probe_one_trace(s: &Server, env: Env<'_>) -> Fallible<Observed> {
    let mcp = Mcp::start(env, &s.url, &s.token_file).await?;
    mcp_instance_create(&mcp).await?;
    // droiduse-mcp logs `droiduse-mcp: instance_create: trace <id> (Target.createTarget)`.
    let mcp_trace = mcp
        .stderr
        .all()
        .iter()
        .find_map(|l| {
            l.strip_prefix("droiduse-mcp: instance_create: trace ")
                .and_then(|r| r.split_whitespace().next())
                .map(str::to_owned)
        })
        .ok_or(format!(
            "no trace id in the MCP log: {:?}",
            mcp.stderr.all()
        ))?;
    let _ = mcp.client.cancel().await;

    let server_trace = in_log(
        s,
        "the createTarget request span linked to the MCP trace",
        |log| {
            closed(log, REQUEST_SPAN)
                .into_iter()
                .find(|l| {
                    l["span"]["method"] == "Target.createTarget"
                        && l["span"]["link_trace_id"] == mcp_trace.as_str()
                })
                .map(|l| l["span"]["trace_id"].clone())
        },
    )
    .await?;
    check(is_trace_id(&server_trace), || {
        format!("server trace id {server_trace}")
    })?;
    for layer in ["host/runtime/instance#create", "host/vmm/api#create"] {
        in_log(
            s,
            &format!("a {layer} span in trace {server_trace}"),
            |log| {
                closed(log, layer)
                    .into_iter()
                    .find(|l| request_span(l).is_some_and(|r| r["trace_id"] == server_trace))
                    .map(|_| ())
            },
        )
        .await?;
    }
    Ok(Observed::Pass)
}
