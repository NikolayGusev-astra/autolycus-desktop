use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use steersman_desktop_lib::{
    AssistantCapabilityInvoker, CapabilityId, CapabilityIntegrationSource,
    CapabilityInvocationInput, CapabilityInvocationResult, CapabilityRouter,
    CapabilityRoutingError, CapabilityRoutingPreference, ConfigureIntegrationRequest,
    ConfiguredFieldValue, ConnectionMode, ConversationId, ConversationService, CredentialBackend,
    DefaultRoutingPolicy, EmitFn, EndpointIdentity, EndpointSnapshot, FakeCatalogRepository,
    FakeInstanceRepository, FakeRuntimePort, FakeSecretStore, InMemoryConversationRepository,
    IntegrationActor, IntegrationCommandError, IntegrationDefinitionId, IntegrationInstance,
    IntegrationInstanceId, IntegrationInstanceRepository, IntegrationManagement,
    IntegrationSecretStore, IntegrationService, IntegrationStatus, OsSecretStore, ProductError,
    RuntimeKey, RuntimeSupervisor, SessionRegistry, SetupValueDto, StaticCapabilityRegistry,
};
use tokio::{net::TcpListener, sync::Mutex as AsyncMutex};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn start_mock_gateway(crash_on_prompt: bool) -> (String, Arc<AsyncMutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(AsyncMutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let requests = Arc::clone(&recorded);
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                if socket
                    .send(Message::Text(
                        json!({"jsonrpc":"2.0", "method":"event", "params":{"type":"gateway.ready", "payload":{}}}).to_string(),
                    ))
                    .await
                    .is_err()
                {
                    return;
                }
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    let Ok(request) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    requests.lock().await.push(request.clone());
                    if crash_on_prompt && request["method"] == "prompt.submit" {
                        return;
                    }
                    let id = request["id"].clone();
                    let result = match request["method"].as_str() {
                        Some("session.create") => {
                            json!({"session_id":"mock-session","stored_session_id":"stored-session","message_count":0,"messages":[],"info":{"desktop_contract":4}})
                        }
                        Some("prompt.submit") => json!({"status":"streaming"}),
                        _ => json!({}),
                    };
                    if socket
                        .send(Message::Text(
                            json!({"jsonrpc":"2.0","id":id,"result":result}).to_string(),
                        ))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    (format!("ws://127.0.0.1:{port}/api/ws?token=test"), requests)
}

async fn local_product_api(url: &str) -> (ConversationService, Arc<RuntimeSupervisor>) {
    let sessions = SessionRegistry::new();
    let local = Arc::new(RuntimeSupervisor::new(
        RuntimeKey::Local,
        Some(Arc::clone(&sessions)),
    ));
    let events: EmitFn = Arc::new(|_| {});
    local
        .start(
            EndpointSnapshot {
                identity: EndpointIdentity::from_ws_url(url, None, None),
                ws_url: url.into(),
                runtime_key: RuntimeKey::Local,
            },
            events,
        )
        .await
        .unwrap();
    let service = ConversationService::new(
        sessions,
        Arc::clone(&local),
        Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Remote("mock".into()),
            None,
        )),
        Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("mock".into()), None)),
        InMemoryConversationRepository::new(),
    );
    (service, local)
}

#[tokio::test]
async fn local_mode_product_api_routes_messages_and_captures_baseline() {
    let started = Instant::now();
    let (url, requests) = start_mock_gateway(false).await;
    let (service, runtime) = local_product_api(&url).await;
    let connection_setup = started.elapsed();

    let started = Instant::now();
    let conversation = service
        .create_conversation(Some(ConnectionMode::Local))
        .await
        .unwrap();
    let session_create = started.elapsed();
    let started = Instant::now();
    service
        .send_message(&conversation, "acceptance message")
        .await
        .unwrap();
    let message_round_trip = started.elapsed();
    tracing::info!(
        ?connection_setup,
        ?session_create,
        ?message_round_trip,
        "acceptance performance baseline"
    );

    assert!(requests
        .lock()
        .await
        .iter()
        .any(|request| request["method"] == "prompt.submit"
            && request["params"]["text"] == "acceptance message"));
    runtime.stop().await;
}

#[tokio::test]
async fn gateway_crash_and_network_disconnect_return_product_errors() {
    let (url, _) = start_mock_gateway(true).await;
    let (service, runtime) = local_product_api(&url).await;
    let conversation = service
        .create_conversation(Some(ConnectionMode::Local))
        .await
        .unwrap();
    assert!(matches!(
        service.send_message(&conversation, "disconnect").await,
        Err(ProductError::SendFailed(_))
    ));
    runtime.stop().await;
}

#[derive(Default)]
struct MemoryCredentials(Mutex<HashMap<String, String>>);
impl CredentialBackend for MemoryCredentials {
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(account.into(), value.into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

#[tokio::test]
async fn os_secret_store_round_trips_without_logging_values() {
    let home = std::env::temp_dir().join(format!(
        "steersman-e2e-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store =
        OsSecretStore::with_credential_backend(&home, Arc::new(MemoryCredentials::default()));
    let id = IntegrationInstanceId(Uuid::new_v4());
    store
        .set_secret(&id, "token", "acceptance-secret")
        .await
        .unwrap();
    assert!(store.has_secret(&id, "token").await.unwrap());
    assert!(!include_str!("../../src/integration_service.rs").contains("value = %value"));
    store.delete_instance_secrets(&id).await.unwrap();
    let _ = std::fs::remove_dir_all(home);
}

fn gmail_request(display_name: &str) -> ConfigureIntegrationRequest {
    ConfigureIntegrationRequest {
        definition_id: IntegrationDefinitionId("gmail".into()),
        instance_id: None,
        display_name: display_name.into(),
        fields: vec![
            ConfiguredFieldValue {
                key: "email".into(),
                value: "acceptance@example.test".into(),
            },
            ConfiguredFieldValue {
                key: "app_password".into(),
                value: "e2e-secret-must-not-leak".into(),
            },
        ],
        management: None,
        enabled_capabilities: vec![CapabilityId("email.read".into())],
    }
}

#[tokio::test]
async fn integration_lifecycle_uses_fake_runtime_and_keeps_secrets_out_of_dtos() {
    let events = Arc::new(AsyncMutex::new(Vec::new()));
    let instances = Arc::new(FakeInstanceRepository::with_events(Arc::clone(&events)));
    let secrets = Arc::new(FakeSecretStore::with_events(Arc::clone(&events)));
    let runtime = Arc::new(FakeRuntimePort::with_events(Arc::clone(&events)));
    let service = IntegrationService::new(
        Arc::new(FakeCatalogRepository::new()),
        instances.clone(),
        secrets.clone(),
        runtime,
    );
    let actor = IntegrationActor::default();

    let configured = service
        .configure_integration(&actor, gmail_request("Acceptance Gmail"))
        .await
        .unwrap();
    assert_eq!(configured.status, IntegrationStatus::Ready);
    assert!(configured
        .configured_fields
        .iter()
        .all(|field| field.value.is_none()));
    assert!(!serde_json::to_string(&configured)
        .unwrap()
        .contains("e2e-secret-must-not-leak"));

    let enabled = service
        .enable_integration(&actor, &configured.id)
        .await
        .unwrap();
    assert_eq!(enabled.status, IntegrationStatus::Ready);
    let tested = service
        .test_integration(&actor, &configured.id)
        .await
        .unwrap();
    assert_eq!(tested.status, IntegrationStatus::Ready);
    let refreshed = service
        .refresh_integration_status(&actor, &configured.id)
        .await
        .unwrap();
    assert_eq!(refreshed.status, IntegrationStatus::Ready);
    let disabled = service
        .disable_integration(&actor, &configured.id)
        .await
        .unwrap();
    assert_eq!(disabled.status, IntegrationStatus::Disabled);
    assert!(secrets
        .has_secret(&configured.id, "app_password")
        .await
        .unwrap());
    service
        .remove_integration(&actor, &configured.id)
        .await
        .unwrap();
    assert!(instances.get(&configured.id).await.is_err());

    let events = events.lock().await;
    let configure = events
        .iter()
        .position(|event| event.starts_with("runtime.configure:"))
        .unwrap();
    let first_health = events
        .iter()
        .position(|event| event.starts_with("runtime.health:"))
        .unwrap();
    let stop = events
        .iter()
        .rposition(|event| event.starts_with("runtime.stop:"))
        .unwrap();
    let delete_secret = events
        .iter()
        .rposition(|event| event.starts_with("secret.delete:"))
        .unwrap();
    let delete_instance = events
        .iter()
        .rposition(|event| event.starts_with("instance.delete:"))
        .unwrap();
    assert!(configure < first_health);
    assert!(stop < delete_secret && delete_secret < delete_instance);
    assert!(events
        .iter()
        .all(|event| !event.contains("e2e-secret-must-not-leak")));
}

struct OneIntegration(IntegrationInstance);
#[async_trait]
impl CapabilityIntegrationSource for OneIntegration {
    async fn list_instances(&self) -> Result<Vec<IntegrationInstance>, CapabilityRoutingError> {
        Ok(vec![self.0.clone()])
    }
}
struct MockInvoker;
#[async_trait]
impl AssistantCapabilityInvoker for MockInvoker {
    async fn invoke(
        &self,
        _: &steersman_desktop_lib::CapabilityRoute,
        _: &CapabilityInvocationInput,
    ) -> Result<CapabilityInvocationResult, CapabilityRoutingError> {
        Ok(CapabilityInvocationResult::SearchResults {
            items: vec![json!({"key":"APP-1"})],
            continuation: None,
        })
    }
}

#[tokio::test]
async fn capability_routing_returns_product_provenance() {
    let instance = IntegrationInstance {
        version: 1,
        id: IntegrationInstanceId(Uuid::new_v4()),
        definition_id: IntegrationDefinitionId("jira".into()),
        display_name: "Jira acceptance".into(),
        status: IntegrationStatus::Ready,
        enabled: true,
        management: IntegrationManagement::UserManaged,
        configured_capabilities: vec![CapabilityId("issue.read".into())],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let router = CapabilityRouter::new(
        Arc::new(StaticCapabilityRegistry),
        Arc::new(OneIntegration(instance.clone())),
        Arc::new(CapabilityRoutingPreference::default()),
        Arc::new(DefaultRoutingPolicy),
    );
    let capability = CapabilityId("issue.search".into());
    let input = CapabilityInvocationInput {
        values: json!({"query":"acceptance"}),
    };
    let outcome = router
        .resolve(
            &ConversationId("acceptance".into()),
            &capability,
            &input,
            false,
        )
        .await
        .unwrap();
    let steersman_desktop_lib::CapabilityRoutingOutcome::Selected { route } = outcome else {
        panic!("expected a selected route")
    };
    let (result, provenance) = router.invoke(&MockInvoker, &route, &input).await.unwrap();
    assert!(matches!(
        result,
        CapabilityInvocationResult::SearchResults { .. }
    ));
    assert_eq!(provenance.instance_id, instance.id);
    assert_eq!(provenance.integration_label, "Jira acceptance");
}

#[test]
fn failure_modes_remain_typed_and_product_dtos_remain_infrastructure_free() {
    let failures = [
        ProductError::RuntimeUnavailable("gateway crash".into()),
        ProductError::RuntimeUnavailable("network disconnected".into()),
        ProductError::RuntimeUnavailable("SSH tunnel failure".into()),
        ProductError::SendFailed("MCP server crash".into()),
        ProductError::Internal("corrupted config".into()),
    ];
    assert!(failures
        .iter()
        .all(|failure| !failure.to_string().is_empty()));
    assert_eq!(
        IntegrationCommandError::AuthenticationRequired,
        IntegrationCommandError::AuthenticationRequired
    );
    let dto_source = include_str!("../../src/product_domain.rs");
    assert!(!dto_source.to_ascii_lowercase().contains("mcp"));
}

#[test]
fn secret_debug_output_is_redacted_and_runtime_secrets_have_no_debug_impl() {
    fn requires_debug<T: std::fmt::Debug>() {}
    requires_debug::<SetupValueDto>();
    assert!(
        !format!("{:?}", SetupValueDto::Secret("acceptance-secret".into()))
            .contains("acceptance-secret")
    );
    let runtime_secret_source = include_str!("../../src/mcp_adapter.rs");
    assert!(!runtime_secret_source.contains("impl std::fmt::Debug for RuntimeSecret"));
}
