//! The commands after `Connection.hello`: which exist, and the ones built so far.

use droiduse_contracts_adp::envelope;
use droiduse_contracts_adp::target::{
    CloseTargetParams, CloseTargetReturns, CreateTargetParams, CreateTargetReturns,
    GetTargetsParams, GetTargetsReturns, TargetCreatedEvent, TargetDestroyedEvent, TargetID,
    TargetInfo, TargetType,
};
use droiduse_contracts_adp::{
    Command, Event, Scope, accessibility, app, connection, input, screen, target, ui,
};
use droiduse_contracts_common::{Error, ErrorCode, Result};
use droiduse_contracts_instance::{CreateInstance, Instance, InstanceId};
use droiduse_contracts_spi::{
    Action, AdpCommand, CommandScope, InstanceRef, InstanceResource, Principal, Resource,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::{COMPONENT, Shared};

const fn method<C: Command>() -> (&'static str, Scope) {
    (C::METHOD, C::SCOPE)
}

/// Every command of the protocol version, with its scope. A test keeps it equal to
/// `contracts/adp/protocol.json`.
pub(crate) const METHODS: &[(&str, Scope)] = &[
    method::<connection::AuthParams>(),
    method::<connection::HelloParams>(),
    method::<target::GetTargetsParams>(),
    method::<target::CreateTargetParams>(),
    method::<target::CloseTargetParams>(),
    method::<target::AttachToTargetParams>(),
    method::<target::DetachFromTargetParams>(),
    method::<ui::GetTreeParams>(),
    method::<ui::QuerySelectorParams>(),
    method::<ui::QuerySelectorAllParams>(),
    method::<ui::GetNodeParams>(),
    method::<accessibility::PerformActionParams>(),
    method::<input::ClickParams>(),
    method::<input::LongClickParams>(),
    method::<input::DispatchTouchParams>(),
    method::<input::InsertTextParams>(),
    method::<input::DispatchKeyParams>(),
    method::<screen::CaptureScreenshotParams>(),
    method::<screen::GetDisplaysParams>(),
    method::<app::CreateUploadParams>(),
    method::<app::UploadChunkParams>(),
    method::<app::FinishUploadParams>(),
    method::<app::InstallParams>(),
    method::<app::LaunchParams>(),
    method::<app::ListParams>(),
];

/// The scope of `method`, `None` if the protocol version has no such command.
pub(crate) fn scope(method: &str) -> Option<Scope> {
    METHODS.iter().find(|(m, _)| *m == method).map(|&(_, s)| s)
}

/// The params of `method` as its parameters type. A mismatch is `INVALID_PARAMS`.
pub(crate) fn params<P: DeserializeOwned>(
    params: Option<Map<String, Value>>,
    method: &str,
) -> Result<P> {
    serde_json::from_value(Value::Object(params.unwrap_or_default())).map_err(|e| {
        Error::new(ErrorCode::InvalidParams, e.to_string()).with_reason(COMPONENT, method)
    })
}

/// `value` as a result or event object.
pub(crate) fn object<T: Serialize>(value: &T) -> Map<String, Value> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map,
        _ => unreachable!("the generated result and event types serialize to objects"),
    }
}

fn event<E: Event>(params: &E) -> envelope::Event {
    envelope::Event {
        method: E::METHOD.to_owned(),
        session_id: None,
        params: Some(object(params)),
    }
}

/// Runs a connection-scoped command after `Connection.hello`. Events it causes are pushed to
/// `events`; they are sent before the response.
pub(crate) async fn call(
    method: &str,
    raw: Option<Map<String, Value>>,
    who: &Principal,
    shared: &Shared,
    events: &mut Vec<envelope::Event>,
) -> Result<Map<String, Value>> {
    if method == GetTargetsParams::METHOD {
        let _: GetTargetsParams = params(raw, method)?;
        get_targets(who, shared).await.map(|r| object(&r))
    } else if method == CreateTargetParams::METHOD {
        let _: CreateTargetParams = params(raw, method)?;
        create_target(who, shared, events).await.map(|r| object(&r))
    } else if method == CloseTargetParams::METHOD {
        let p: CloseTargetParams = params(raw, method)?;
        close_target(p, who, shared, events)
            .await
            .map(|r| object(&r))
    } else {
        Err(Error::new(
            ErrorCode::Unimplemented,
            format!("{method} is not built yet"),
        )
        .with_reason(COMPONENT, method))
    }
}

fn instance_resource(instance: &dyn Instance) -> Resource {
    Resource::Instance(InstanceResource::new(
        InstanceRef(instance.id().0),
        instance.labels().clone(),
    ))
}

/// Whether `who` may do `method` on `on`. A denial, and an authorizer error, is `false`.
async fn allowed(shared: &Shared, who: &Principal, method: &str, on: &Resource) -> bool {
    let action = Action::Adp(AdpCommand::new(method, CommandScope::Connection));
    match shared.services.authorizer.authorize(who, &action, on).await {
        Ok(decision) => decision.is_allowed(),
        Err(e) => {
            tracing::warn!(error = %e, "the authorizer failed; counted as a denial");
            false
        }
    }
}

async fn authorize(shared: &Shared, who: &Principal, method: &str, on: &Resource) -> Result<()> {
    if allowed(shared, who, method, on).await {
        Ok(())
    } else {
        Err(
            Error::new(ErrorCode::PolicyDenied, "denied by the authorizer")
                .with_reason(COMPONENT, method),
        )
    }
}

/// Whether `instance` is listed to `who`: only targets the principal may attach to.
async fn listed(shared: &Shared, who: &Principal, instance: &dyn Instance) -> bool {
    let attach = target::AttachToTargetParams::METHOD;
    allowed(shared, who, attach, &instance_resource(instance)).await
}

fn instance_info(instance: &dyn Instance) -> TargetInfo {
    TargetInfo {
        target_id: TargetID(instance.id().0),
        r#type: TargetType::Instance,
        attached: false,
        parent_id: None,
    }
}

async fn get_targets(who: &Principal, shared: &Shared) -> Result<GetTargetsReturns> {
    authorize(shared, who, GetTargetsParams::METHOD, &Resource::Server).await?;
    let mut target_infos = Vec::new();
    for instance in shared.services.instances.list() {
        if listed(shared, who, instance.as_ref()).await {
            target_infos.push(instance_info(instance.as_ref()));
        }
    }
    Ok(GetTargetsReturns { target_infos })
}

async fn create_target(
    who: &Principal,
    shared: &Shared,
    events: &mut Vec<envelope::Event>,
) -> Result<CreateTargetReturns> {
    authorize(shared, who, CreateTargetParams::METHOD, &Resource::Server).await?;
    let instance = shared
        .services
        .instances
        .create(CreateInstance::default())
        .await?;
    if listed(shared, who, instance.as_ref()).await {
        events.push(event(&TargetCreatedEvent {
            target_info: instance_info(instance.as_ref()),
        }));
    }
    Ok(CreateTargetReturns {
        target_id: TargetID(instance.id().0),
    })
}

async fn close_target(
    p: CloseTargetParams,
    who: &Principal,
    shared: &Shared,
    events: &mut Vec<envelope::Event>,
) -> Result<CloseTargetReturns> {
    let method = CloseTargetParams::METHOD;
    let id = InstanceId(p.target_id.0.clone());
    let Some(instance) = shared.services.instances.get(&id) else {
        return Err(
            Error::new(ErrorCode::TargetGone, "no target with this targetId")
                .with_reason(COMPONENT, method),
        );
    };
    authorize(shared, who, method, &instance_resource(instance.as_ref())).await?;
    let shown = listed(shared, who, instance.as_ref()).await;
    shared.services.instances.close(&id).await?;
    if shown {
        events.push(event(&TargetDestroyedEvent {
            target_id: p.target_id,
        }));
    }
    Ok(CloseTargetReturns {})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table lists exactly the commands of `contracts/adp/protocol.json`, with their scopes.
    #[test]
    fn methods_equal_the_protocol_definition() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../contracts/adp/protocol.json"
        );
        let def: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut want = Vec::new();
        for domain in def["domains"].as_array().unwrap() {
            let name = domain["domain"].as_str().unwrap();
            for c in domain["commands"].as_array().into_iter().flatten() {
                let scope = match c["scope"].as_str().unwrap() {
                    "connection" => Scope::Connection,
                    "session" => Scope::Session,
                    "both" => Scope::Both,
                    other => panic!("unknown scope {other}"),
                };
                want.push((format!("{name}.{}", c["name"].as_str().unwrap()), scope));
            }
        }
        let mut have: Vec<_> = METHODS.iter().map(|&(m, s)| (m.to_owned(), s)).collect();
        want.sort_by(|a, b| a.0.cmp(&b.0));
        have.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(have, want);
    }
}
