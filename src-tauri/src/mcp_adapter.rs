//! MCP infrastructure adapter.  This is deliberately the only integration
//! runtime implementation which knows about the MCP wire/process boundary.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{
    integration_domain::{
        CapabilityId, IntegrationCatalogRepository, IntegrationCommandError,
        IntegrationDefinitionId, IntegrationInstance, IntegrationInstanceId, IntegrationIssue,
        IntegrationRuntimePort, IntegrationStatus,
    },
    mcp_client::{McpClientPool, McpStdioClient},
};

pub const MCP_STATELESS_VERSION: &str = "2026-07-28";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpProtocolMode {
    Stateless2026,
    LegacySession,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCompatibility {
    pub selected_version: String,
    pub mode: McpProtocolMode,
    pub server_info: McpServerInfo,
    pub capabilities: Vec<String>,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerDescriptor {
    pub definition_id: IntegrationDefinitionId,
    pub package: String,
    pub transport_kind: McpTransportKind,
    pub version_constraints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransportKind {
    Stdio,
    StreamableHttp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTransportConfig {
    pub kind: McpTransportKind,
    /// Curated, literal process arguments. They are never passed to a shell.
    pub args: Vec<String>,
    pub env_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAuthorizationKind {
    None,
    EnvSecret,
    BearerToken,
    OAuthReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuthorizationConfig {
    pub kind: McpAuthorizationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpCapabilitySnapshot {
    pub tools: Vec<String>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCapabilityRequirement {
    pub product_capability: CapabilityId,
    pub required_tools: Vec<String>,
    pub optional_resources: Vec<String>,
}

/// A value which intentionally has no Debug, Display, or Serialize impl.
#[derive(Clone, PartialEq, Eq)]
pub struct RuntimeSecret(String);
impl RuntimeSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    fn expose(&self) -> &str {
        &self.0
    }
}

/// Resolved only in memory immediately before a process is launched.  Do not
/// persist or log this type.
#[derive(Clone)]
pub struct ResolvedIntegrationConfiguration {
    pub instance_id: IntegrationInstanceId,
    pub descriptor: McpServerDescriptor,
    pub transport: McpTransportConfig,
    pub auth: McpAuthorizationConfig,
    pub env: HashMap<String, RuntimeSecret>,
    pub capabilities: McpCapabilitySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRuntimeIdentity {
    pub definition_id: IntegrationDefinitionId,
    pub instance_id: IntegrationInstanceId,
    pub non_secret_fingerprint: String,
    pub secret_fingerprint: String,
    pub transport_fingerprint: String,
}

pub struct McpRuntimeHandle {
    pub instance_id: IntegrationInstanceId,
    pub process: McpStdioClient,
    pub config_identity: McpRuntimeIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpHealthSnapshot {
    pub status: IntegrationStatus,
    pub error_count: u64,
    pub last_error: Option<String>,
    pub issue: Option<IntegrationIssue>,
}

struct ActiveRuntime {
    #[allow(dead_code)]
    generation: u64,
    handle: McpRuntimeHandle,
    capabilities: McpCapabilitySnapshot,
}

/// The adapter keeps process handles and capability cache per *instance*, not
/// per definition, so two Jira accounts can never share credentials or state.
pub struct McpIntegrationRuntimeAdapter {
    catalog: Arc<dyn IntegrationCatalogRepository>,
    mcp_client: Arc<McpClientPool>,
    runtimes: Mutex<HashMap<IntegrationInstanceId, ActiveRuntime>>,
    configurations: Mutex<HashMap<IntegrationInstanceId, ResolvedIntegrationConfiguration>>,
    generations: Mutex<HashMap<IntegrationInstanceId, u64>>,
}

impl McpIntegrationRuntimeAdapter {
    pub fn new(
        catalog: Arc<dyn IntegrationCatalogRepository>,
        mcp_client: Arc<McpClientPool>,
    ) -> Self {
        Self {
            catalog,
            mcp_client,
            runtimes: Mutex::default(),
            configurations: Mutex::default(),
            generations: Mutex::default(),
        }
    }

    pub fn resolve_descriptor(
        id: &IntegrationDefinitionId,
    ) -> Result<McpServerDescriptor, IntegrationCommandError> {
        let package = match id.0.as_str() {
            "gmail" => "gmail-mcp",
            "jira" => "jira-mcp",
            "confluence" => "confluence-mcp",
            _ => return Err(IntegrationCommandError::DefinitionNotFound),
        };
        Ok(McpServerDescriptor {
            definition_id: id.clone(),
            package: package.into(),
            transport_kind: McpTransportKind::Stdio,
            version_constraints: vec![MCP_STATELESS_VERSION.into(), "2024-11-05".into()],
        })
    }

    fn transport_for(descriptor: &McpServerDescriptor) -> McpTransportConfig {
        let env_allowlist = match descriptor.definition_id.0.as_str() {
            "gmail" => vec!["GMAIL_EMAIL".into(), "GMAIL_APP_PASSWORD".into()],
            "jira" => vec![
                "JIRA_BASE_URL".into(),
                "JIRA_EMAIL".into(),
                "JIRA_API_TOKEN".into(),
            ],
            "confluence" => vec![
                "CONFLUENCE_BASE_URL".into(),
                "CONFLUENCE_EMAIL".into(),
                "CONFLUENCE_API_TOKEN".into(),
            ],
            _ => Vec::new(),
        };
        McpTransportConfig {
            kind: descriptor.transport_kind,
            args: Vec::new(),
            env_allowlist,
        }
    }

    fn auth_for(descriptor: &McpServerDescriptor) -> McpAuthorizationConfig {
        let kind = match descriptor.definition_id.0.as_str() {
            "gmail" => McpAuthorizationKind::EnvSecret,
            "jira" | "confluence" => McpAuthorizationKind::BearerToken,
            _ => McpAuthorizationKind::None,
        };
        McpAuthorizationConfig { kind }
    }

    fn fingerprint(
        instance: &IntegrationInstance,
        cfg: &ResolvedIntegrationConfiguration,
    ) -> McpRuntimeIdentity {
        fn hash(parts: impl IntoIterator<Item = String>) -> String {
            let mut h = Sha256::new();
            for p in parts {
                h.update(p.as_bytes());
                h.update([0]);
            }
            hex::encode(h.finalize())
        }
        let mut keys: Vec<_> = cfg.env.keys().cloned().collect();
        keys.sort();
        let secret = hash(
            keys.iter()
                .filter_map(|k| cfg.env.get(k).map(|v| format!("{k}:{}", v.expose()))),
        );
        McpRuntimeIdentity {
            definition_id: instance.definition_id.clone(),
            instance_id: instance.id.clone(),
            non_secret_fingerprint: hash([
                instance.definition_id.0.clone(),
                instance.enabled.to_string(),
            ]),
            secret_fingerprint: secret,
            transport_fingerprint: hash(cfg.transport.args.clone()),
        }
    }

    async fn configuration(
        &self,
        instance: &IntegrationInstance,
    ) -> Result<ResolvedIntegrationConfiguration, IntegrationCommandError> {
        self.catalog.get_definition(&instance.definition_id).await?;
        let descriptor = Self::resolve_descriptor(&instance.definition_id)?;
        Ok(ResolvedIntegrationConfiguration {
            instance_id: instance.id.clone(),
            transport: Self::transport_for(&descriptor),
            auth: Self::auth_for(&descriptor),
            descriptor,
            env: HashMap::new(),
            capabilities: McpCapabilitySnapshot::default(),
        })
    }

    async fn launch(
        &self,
        instance: &IntegrationInstance,
        cfg: &ResolvedIntegrationConfiguration,
    ) -> Result<ActiveRuntime, IntegrationCommandError> {
        if cfg.transport.kind != McpTransportKind::Stdio {
            return Err(IntegrationCommandError::RuntimeUnavailable);
        }
        let allowed: HashSet<_> = cfg.transport.env_allowlist.iter().collect();
        if cfg.env.keys().any(|key| !allowed.contains(key)) {
            return Err(IntegrationCommandError::ConfigurationInvalid { fields: vec![] });
        }
        let env: HashMap<_, _> = cfg
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.expose().to_owned()))
            .collect();
        let mut process = self
            .mcp_client
            .spawn_stdio(&cfg.descriptor.package, &cfg.transport.args, &env)
            .map_err(|_| IntegrationCommandError::RuntimeUnavailable)?;
        // Stateless discovery is always first. Authentication and malformed
        // replies fail closed; only a known protocol rejection may fall back.
        let discovered = match process.discover().await {
            Ok(result) => result,
            Err(error) if is_legacy_protocol_rejection(&error) => {
                process
                    .initialize()
                    .await
                    .map_err(|error| map_error(&error))?;
                serde_json::Value::Null
            }
            Err(error) => return Err(map_error(&error)),
        };
        let capabilities = snapshot_from_discovery(&discovered);
        let generation = *self
            .generations
            .lock()
            .await
            .get(&instance.id)
            .unwrap_or(&0);
        Ok(ActiveRuntime {
            generation,
            handle: McpRuntimeHandle {
                instance_id: instance.id.clone(),
                config_identity: Self::fingerprint(instance, cfg),
                process,
            },
            capabilities,
        })
    }

    pub async fn capability_status(
        &self,
        instance: &IntegrationInstance,
        capability: &CapabilityId,
    ) -> Result<NormalizedCapability, IntegrationCommandError> {
        let requirements = capability_requirements(&instance.definition_id);
        let Some(requirement) = requirements
            .into_iter()
            .find(|it| it.product_capability == *capability)
        else {
            return Ok(NormalizedCapability::Unsupported);
        };
        let runtimes = self.runtimes.lock().await;
        let Some(runtime) = runtimes.get(&instance.id) else {
            return Ok(NormalizedCapability::TemporarilyUnavailable);
        };
        if requirement
            .required_tools
            .iter()
            .all(|tool| runtime.capabilities.tools.contains(tool))
        {
            Ok(NormalizedCapability::Available)
        } else {
            Ok(NormalizedCapability::Unsupported)
        }
    }

    pub async fn health_snapshot(&self, id: &IntegrationInstanceId) -> McpHealthSnapshot {
        let exists = self.runtimes.lock().await.contains_key(id);
        if exists {
            McpHealthSnapshot {
                status: IntegrationStatus::Ready,
                error_count: 0,
                last_error: None,
                issue: None,
            }
        } else {
            McpHealthSnapshot {
                status: IntegrationStatus::Degraded {
                    reason: IntegrationIssue::RuntimeUnavailable,
                },
                error_count: 1,
                last_error: Some("runtime unavailable".into()),
                issue: Some(IntegrationIssue::RuntimeUnavailable),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizedCapability {
    Available,
    TemporarilyUnavailable,
    AuthorizationRequired,
    Unsupported,
}

pub fn capability_requirements(
    definition: &IntegrationDefinitionId,
) -> Vec<McpCapabilityRequirement> {
    let pairs: &[(&str, &[&str])] = match definition.0.as_str() {
        "gmail" => &[
            ("email.read", &["list_inbox"]),
            ("email.send", &["send_email"]),
        ],
        "jira" => &[
            ("issue.read", &["search_issues"]),
            ("issue.write", &["create_issue"]),
            ("issue.transition", &["transition_issue"]),
        ],
        "confluence" => &[
            ("page.read", &["search_pages"]),
            ("page.write", &["create_page"]),
        ],
        _ => &[],
    };
    pairs
        .iter()
        .map(|(capability, tools)| McpCapabilityRequirement {
            product_capability: CapabilityId((*capability).into()),
            required_tools: tools.iter().map(|x| (*x).into()).collect(),
            optional_resources: Vec::new(),
        })
        .collect()
}

pub fn map_error(error: &str) -> IntegrationCommandError {
    let e = error.to_ascii_lowercase();
    if e.contains("401") || e.contains("authentication") || e.contains("unauthorized") {
        IntegrationCommandError::AuthenticationRequired
    } else if e.contains("403") || e.contains("scope") || e.contains("permission") {
        IntegrationCommandError::PermissionDenied
    } else if e.contains("timeout")
        || e.contains("network")
        || e.contains("config")
        || e.contains("invalid")
    {
        IntegrationCommandError::HealthCheckFailed
    } else {
        IntegrationCommandError::RuntimeUnavailable
    }
}

pub fn normalize_mcp_issue(error: &str) -> IntegrationIssue {
    let e = error.to_ascii_lowercase();
    if e.contains("expired") {
        IntegrationIssue::AuthenticationExpired
    } else if e.contains("401") || e.contains("authentication") || e.contains("unauthorized") {
        IntegrationIssue::AuthenticationRequired
    } else if e.contains("403") || e.contains("scope") || e.contains("permission") {
        IntegrationIssue::PermissionDenied
    } else if e.contains("network") || e.contains("dns") || e.contains("connection refused") {
        IntegrationIssue::NetworkUnavailable
    } else if e.contains("timeout") || e.contains("503") {
        IntegrationIssue::ServiceUnavailable
    } else if e.contains("config") || e.contains("invalid") {
        IntegrationIssue::ConfigurationInvalid
    } else if e.contains("exit") || e.contains("closed") {
        IntegrationIssue::RuntimeUnavailable
    } else {
        IntegrationIssue::Unknown
    }
}

fn is_legacy_protocol_rejection(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("unsupported")
        || error.contains("method not found")
        || error.contains("protocol version")
}

fn snapshot_from_discovery(value: &serde_json::Value) -> McpCapabilitySnapshot {
    fn names(value: Option<&serde_json::Value>) -> Vec<String> {
        value
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.get("name").and_then(|v| v.as_str()).map(str::to_owned))
            .collect()
    }
    McpCapabilitySnapshot {
        tools: names(value.get("tools")),
        resources: names(value.get("resources")),
        prompts: names(value.get("prompts")),
    }
}

#[async_trait]
impl IntegrationRuntimePort for McpIntegrationRuntimeAdapter {
    async fn configure(
        &self,
        instance: &IntegrationInstance,
    ) -> Result<(), IntegrationCommandError> {
        let cfg = self.configuration(instance).await?;
        // Configuration changes invalidate capability observations before launch.
        self.configurations
            .lock()
            .await
            .insert(instance.id.clone(), cfg);
        self.start(&instance.id).await
    }

    async fn start(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        let cfg = self
            .configurations
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or(IntegrationCommandError::RuntimeUnavailable)?;
        let instance = IntegrationInstance {
            id: id.clone(),
            definition_id: cfg.descriptor.definition_id.clone(),
            display_name: String::new(),
            status: IntegrationStatus::Connecting,
            enabled: true,
            management: crate::integration_domain::IntegrationManagement::UserManaged,
            configured_capabilities: Vec::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let desired = Self::fingerprint(&instance, &cfg);
        if self
            .runtimes
            .lock()
            .await
            .get(id)
            .is_some_and(|runtime| runtime.handle.config_identity == desired)
        {
            return Ok(());
        }
        let new = tokio::time::timeout(Duration::from_secs(10), self.launch(&instance, &cfg))
            .await
            .map_err(|_| IntegrationCommandError::HealthCheckFailed)??;
        // Do not stop the previous healthy runtime until replacement succeeded.
        let old = self.runtimes.lock().await.insert(id.clone(), new);
        if let Some(mut old) = old {
            old.handle.process.shutdown().await;
        }
        Ok(())
    }

    async fn stop(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        let old = self.runtimes.lock().await.remove(id);
        if let Some(mut runtime) = old {
            runtime.handle.process.shutdown().await;
        }
        *self.generations.lock().await.entry(id.clone()).or_default() += 1;
        Ok(())
    }

    async fn health(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<IntegrationStatus, IntegrationCommandError> {
        let snapshot = tokio::time::timeout(Duration::from_secs(2), self.health_snapshot(id))
            .await
            .map_err(|_| IntegrationCommandError::HealthCheckFailed)?;
        Ok(snapshot.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curated_definitions_are_allowlisted() {
        assert_eq!(
            McpIntegrationRuntimeAdapter::resolve_descriptor(&IntegrationDefinitionId(
                "gmail".into()
            ))
            .unwrap()
            .package,
            "gmail-mcp"
        );
        assert_eq!(
            McpIntegrationRuntimeAdapter::resolve_descriptor(&IntegrationDefinitionId(
                "jira".into()
            ))
            .unwrap()
            .package,
            "jira-mcp"
        );
        assert_eq!(
            McpIntegrationRuntimeAdapter::resolve_descriptor(&IntegrationDefinitionId(
                "confluence".into()
            ))
            .unwrap()
            .package,
            "confluence-mcp"
        );
        assert!(
            McpIntegrationRuntimeAdapter::resolve_descriptor(&IntegrationDefinitionId("x".into()))
                .is_err()
        );
    }
    #[test]
    fn secrets_are_not_debuggable_or_serializable() {
        let secret = RuntimeSecret::new("secret-never-log");
        assert_eq!(secret.expose(), "secret-never-log");
    }
    #[test]
    fn requirements_are_per_product_capability() {
        assert_eq!(
            capability_requirements(&IntegrationDefinitionId("jira".into())).len(),
            3
        );
    }
    #[test]
    fn domain_and_application_do_not_import_mcp() {
        assert!(!include_str!("integration_domain.rs").contains("mcp_"));
        assert!(!include_str!("integration_service.rs").contains("mcp_"));
    }
}
