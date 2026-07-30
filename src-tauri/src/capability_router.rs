//! Product-level assistant capability routing. Transport details stay behind
//! `AssistantCapabilityInvoker`; this module never names a provider operation.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::integration_domain::{
    CapabilityId, IntegrationInstance, IntegrationInstanceId, IntegrationManagement,
    IntegrationStatus,
};
use crate::product_domain::ConversationId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssistantCapabilityDefinition {
    pub id: CapabilityId,
    pub description: String,
    pub input_schema: Value,
    pub risk: CapabilityRisk,
    pub invocation_mode: CapabilityInvocationMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRisk {
    ReadOnly,
    SensitiveRead,
    ExternalMutation,
    Privileged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityInvocationMode {
    Search,
    Read,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityInvocationInput {
    pub values: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityInvocationResult {
    SearchResults {
        items: Vec<Value>,
        continuation: Option<String>,
    },
    Resource {
        item: Value,
    },
    ActionPrepared,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityResultProvenance {
    pub instance_id: IntegrationInstanceId,
    pub integration_label: String,
    pub capability_id: CapabilityId,
    pub retrieved_at: DateTime<Utc>,
}

#[async_trait]
pub trait AssistantCapabilityRegistry: Send + Sync {
    async fn get(&self, id: &CapabilityId) -> Option<AssistantCapabilityDefinition>;
    async fn list(&self) -> Vec<AssistantCapabilityDefinition>;
}

pub struct StaticCapabilityRegistry;
impl StaticCapabilityRegistry {
    pub fn curated() -> Vec<AssistantCapabilityDefinition> {
        [
            (
                "issue.search",
                "Search issues",
                CapabilityInvocationMode::Search,
            ),
            (
                "issue.read",
                "Read an issue",
                CapabilityInvocationMode::Read,
            ),
            (
                "mail.search",
                "Search mail",
                CapabilityInvocationMode::Search,
            ),
            (
                "mail.read",
                "Read a mail message",
                CapabilityInvocationMode::Read,
            ),
            (
                "documentation.search",
                "Search documentation",
                CapabilityInvocationMode::Search,
            ),
            (
                "documentation.read",
                "Read documentation",
                CapabilityInvocationMode::Read,
            ),
        ]
        .into_iter()
        .map(
            |(id, description, invocation_mode)| AssistantCapabilityDefinition {
                id: CapabilityId(id.into()),
                description: description.into(),
                input_schema: serde_json::json!({"type":"object"}),
                risk: CapabilityRisk::ReadOnly,
                invocation_mode,
            },
        )
        .collect()
    }
}
#[async_trait]
impl AssistantCapabilityRegistry for StaticCapabilityRegistry {
    async fn get(&self, id: &CapabilityId) -> Option<AssistantCapabilityDefinition> {
        Self::curated().into_iter().find(|item| item.id == *id)
    }
    async fn list(&self) -> Vec<AssistantCapabilityDefinition> {
        Self::curated()
    }
}

#[async_trait]
pub trait CapabilityIntegrationSource: Send + Sync {
    async fn list_instances(&self) -> Result<Vec<IntegrationInstance>, CapabilityRoutingError>;
}

#[async_trait]
pub trait CapabilityRoutingPolicy: Send + Sync {
    fn allows(
        &self,
        definition: &AssistantCapabilityDefinition,
        instance: &IntegrationInstance,
        is_admin: bool,
    ) -> bool;
}
pub struct DefaultRoutingPolicy;
impl CapabilityRoutingPolicy for DefaultRoutingPolicy {
    fn allows(
        &self,
        definition: &AssistantCapabilityDefinition,
        instance: &IntegrationInstance,
        is_admin: bool,
    ) -> bool {
        matches!(
            definition.risk,
            CapabilityRisk::ReadOnly | CapabilityRisk::SensitiveRead
        ) && (is_admin
            || !matches!(
                instance.management,
                IntegrationManagement::AdministratorManaged
            ))
    }
}

#[derive(Default)]
pub struct CapabilityRoutingPreference {
    values: Mutex<HashMap<(ConversationId, CapabilityId), IntegrationInstanceId>>,
}
impl CapabilityRoutingPreference {
    pub async fn get(
        &self,
        conversation_id: &ConversationId,
        capability_id: &CapabilityId,
    ) -> Option<IntegrationInstanceId> {
        self.values
            .lock()
            .await
            .get(&(conversation_id.clone(), capability_id.clone()))
            .cloned()
    }
    pub async fn set(
        &self,
        conversation_id: ConversationId,
        capability_id: CapabilityId,
        instance_id: IntegrationInstanceId,
    ) {
        self.values
            .lock()
            .await
            .insert((conversation_id, capability_id), instance_id);
    }
    pub async fn clear(&self, conversation_id: &ConversationId, capability_id: &CapabilityId) {
        self.values
            .lock()
            .await
            .remove(&(conversation_id.clone(), capability_id.clone()));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityRoute {
    pub instance_id: IntegrationInstanceId,
    pub integration_label: String,
    pub capability_id: CapabilityId,
    pub revision: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityRoutingOutcome {
    Selected {
        route: CapabilityRoute,
    },
    ClarificationRequired {
        clarification: CapabilityClarification,
    },
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityClarification {
    pub request_id: String,
    pub prompt: String,
    pub choices: Vec<CapabilityRouteChoice>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityRouteChoice {
    pub instance_id: IntegrationInstanceId,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum CapabilityRoutingError {
    UnknownCapability,
    InvalidInput,
    InstanceUnavailable,
    StaleRoute,
    InvocationFailed,
}

#[async_trait]
pub trait AssistantCapabilityInvoker: Send + Sync {
    async fn invoke(
        &self,
        route: &CapabilityRoute,
        input: &CapabilityInvocationInput,
    ) -> Result<CapabilityInvocationResult, CapabilityRoutingError>;
}

pub struct CapabilityRouter {
    registry: Arc<dyn AssistantCapabilityRegistry>,
    integrations: Arc<dyn CapabilityIntegrationSource>,
    preferences: Arc<CapabilityRoutingPreference>,
    policies: Arc<dyn CapabilityRoutingPolicy>,
}
impl CapabilityRouter {
    pub fn new(
        registry: Arc<dyn AssistantCapabilityRegistry>,
        integrations: Arc<dyn CapabilityIntegrationSource>,
        preferences: Arc<CapabilityRoutingPreference>,
        policies: Arc<dyn CapabilityRoutingPolicy>,
    ) -> Self {
        Self {
            registry,
            integrations,
            preferences,
            policies,
        }
    }
    pub async fn list_capabilities(&self) -> Vec<AssistantCapabilityDefinition> {
        self.registry.list().await
    }
    pub async fn resolve(
        &self,
        conversation_id: &ConversationId,
        capability_id: &CapabilityId,
        input: &CapabilityInvocationInput,
        is_admin: bool,
    ) -> Result<CapabilityRoutingOutcome, CapabilityRoutingError> {
        let definition = self
            .registry
            .get(capability_id)
            .await
            .ok_or(CapabilityRoutingError::UnknownCapability)?;
        validate_input(input)?;
        let mut candidates: Vec<_> = self
            .integrations
            .list_instances()
            .await?
            .into_iter()
            .filter(|i| supports_capability(i, capability_id))
            .filter(|i| i.enabled && self.policies.allows(&definition, i, is_admin))
            .filter(|i| {
                matches!(
                    i.status,
                    IntegrationStatus::Ready | IntegrationStatus::Degraded { .. }
                )
            })
            .collect();
        candidates.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        if candidates.is_empty() {
            return Ok(CapabilityRoutingOutcome::Unavailable);
        }
        if let Some(preferred) = self.preferences.get(conversation_id, capability_id).await {
            if let Some(instance) = candidates.iter().find(|i| i.id == preferred) {
                return Ok(CapabilityRoutingOutcome::Selected {
                    route: route(instance, capability_id),
                });
            }
        }
        if candidates.len() == 1 {
            return Ok(CapabilityRoutingOutcome::Selected {
                route: route(&candidates[0], capability_id),
            });
        }
        Ok(CapabilityRoutingOutcome::ClarificationRequired {
            clarification: CapabilityClarification {
                request_id: Uuid::new_v4().to_string(),
                prompt: format!("Choose an integration for {}", definition.description),
                choices: candidates
                    .into_iter()
                    .map(|i| CapabilityRouteChoice {
                        instance_id: i.id,
                        label: i.display_name,
                        description: definition.description.clone(),
                    })
                    .collect(),
            },
        })
    }
    pub async fn submit_choice(
        &self,
        conversation_id: ConversationId,
        capability_id: CapabilityId,
        instance_id: IntegrationInstanceId,
    ) {
        self.preferences
            .set(conversation_id, capability_id, instance_id)
            .await;
    }
    pub async fn clear_preference(
        &self,
        conversation_id: &ConversationId,
        capability_id: &CapabilityId,
    ) {
        self.preferences.clear(conversation_id, capability_id).await;
    }
    pub async fn invoke(
        &self,
        invoker: &dyn AssistantCapabilityInvoker,
        route: &CapabilityRoute,
        input: &CapabilityInvocationInput,
    ) -> Result<(CapabilityInvocationResult, CapabilityResultProvenance), CapabilityRoutingError>
    {
        let instance = self
            .integrations
            .list_instances()
            .await?
            .into_iter()
            .find(|i| i.id == route.instance_id)
            .ok_or(CapabilityRoutingError::InstanceUnavailable)?;
        if !instance.enabled || instance.updated_at != route.revision {
            return Err(CapabilityRoutingError::StaleRoute);
        }
        let result = invoker.invoke(route, input).await?;
        Ok((
            result,
            CapabilityResultProvenance {
                instance_id: instance.id,
                integration_label: instance.display_name,
                capability_id: route.capability_id.clone(),
                retrieved_at: Utc::now(),
            },
        ))
    }
}
fn validate_input(input: &CapabilityInvocationInput) -> Result<(), CapabilityRoutingError> {
    if input.values.is_object() {
        Ok(())
    } else {
        Err(CapabilityRoutingError::InvalidInput)
    }
}
fn route(instance: &IntegrationInstance, capability_id: &CapabilityId) -> CapabilityRoute {
    CapabilityRoute {
        instance_id: instance.id.clone(),
        integration_label: instance.display_name.clone(),
        capability_id: capability_id.clone(),
        revision: instance.updated_at,
    }
}

/// Curated assistant capabilities are a stable product vocabulary. Existing
/// integration configuration keeps its legacy IDs so Phase 5 does not change
/// configured integrations or expose provider operation names.
fn supports_capability(instance: &IntegrationInstance, capability: &CapabilityId) -> bool {
    let legacy_read = match capability.0.as_str() {
        "issue.search" | "issue.read" => "issue.read",
        "mail.search" | "mail.read" => "email.read",
        "documentation.search" | "documentation.read" => "page.read",
        _ => return false,
    };
    instance
        .configured_capabilities
        .iter()
        .any(|item| item.0 == legacy_read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration_domain::{IntegrationDefinitionId, IntegrationManagement};
    use chrono::Utc;
    struct Source(Vec<IntegrationInstance>);
    #[async_trait]
    impl CapabilityIntegrationSource for Source {
        async fn list_instances(&self) -> Result<Vec<IntegrationInstance>, CapabilityRoutingError> {
            Ok(self.0.clone())
        }
    }
    fn instance(cap: &str) -> IntegrationInstance {
        IntegrationInstance {
            version: 1,
            id: IntegrationInstanceId(Uuid::new_v4()),
            definition_id: IntegrationDefinitionId("test".into()),
            display_name: "Test".into(),
            status: IntegrationStatus::Ready,
            enabled: true,
            management: IntegrationManagement::UserManaged,
            configured_capabilities: vec![CapabilityId(cap.into())],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    fn router(items: Vec<IntegrationInstance>) -> CapabilityRouter {
        CapabilityRouter::new(
            Arc::new(StaticCapabilityRegistry),
            Arc::new(Source(items)),
            Arc::new(CapabilityRoutingPreference::default()),
            Arc::new(DefaultRoutingPolicy),
        )
    }
    #[tokio::test]
    async fn selects_single_ready_instance() {
        let r = router(vec![instance("issue.read")]);
        assert!(matches!(
            r.resolve(
                &ConversationId("c".into()),
                &CapabilityId("issue.search".into()),
                &CapabilityInvocationInput {
                    values: serde_json::json!({})
                },
                false
            )
            .await
            .unwrap(),
            CapabilityRoutingOutcome::Selected { .. }
        ));
    }
    #[tokio::test]
    async fn unknown_capability_is_rejected() {
        assert_eq!(
            router(vec![])
                .resolve(
                    &ConversationId("c".into()),
                    &CapabilityId("x".into()),
                    &CapabilityInvocationInput {
                        values: serde_json::json!({})
                    },
                    false
                )
                .await
                .unwrap_err(),
            CapabilityRoutingError::UnknownCapability
        );
    }
    #[tokio::test]
    async fn disabled_instance_is_unavailable() {
        let mut i = instance("issue.read");
        i.enabled = false;
        assert!(matches!(
            router(vec![i])
                .resolve(
                    &ConversationId("c".into()),
                    &CapabilityId("issue.search".into()),
                    &CapabilityInvocationInput {
                        values: serde_json::json!({})
                    },
                    false
                )
                .await
                .unwrap(),
            CapabilityRoutingOutcome::Unavailable
        ));
    }
    #[test]
    fn product_layers_have_no_transport_imports() {
        for source in [
            include_str!("capability_router.rs"),
            include_str!("product_domain.rs"),
            include_str!("conversation_service.rs"),
        ] {
            assert!(!source.contains("mcp_adapter"));
            assert!(!source.contains("mcp_client"));
        }
    }
}
