use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntegrationDefinitionId(pub String);

impl fmt::Display for IntegrationDefinitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntegrationInstanceId(pub Uuid);

impl fmt::Display for IntegrationInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(pub String);

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCategory {
    Communication,
    Knowledge,
    ProjectManagement,
    Files,
    Calendar,
    Development,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationDefinition {
    pub id: IntegrationDefinitionId,
    pub display_name: String,
    pub description: String,
    pub category: IntegrationCategory,
    pub icon: String,
    pub capabilities: Vec<IntegrationCapability>,
    pub setup_schema: Vec<SetupField>,
    pub availability: IntegrationAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationAvailability {
    Available,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationCapability {
    pub id: CapabilityId,
    pub display_name: String,
    pub description: String,
    pub access: CapabilityAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAccess {
    Read,
    Write,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupField {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub field_type: SetupFieldType,
    pub required: bool,
    pub secret: bool,
    pub default_value: Option<String>,
    pub validation: Option<FieldValidation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupFieldType {
    Text,
    Password,
    Url,
    Number,
    Boolean,
    Select { options: Vec<SelectOption> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldValidation {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationInstance {
    #[serde(default = "integration_instance_schema_version")]
    pub version: u32,
    pub id: IntegrationInstanceId,
    pub definition_id: IntegrationDefinitionId,
    pub display_name: String,
    pub status: IntegrationStatus,
    pub enabled: bool,
    pub management: IntegrationManagement,
    pub configured_capabilities: Vec<CapabilityId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const INTEGRATION_INSTANCE_SCHEMA_VERSION: u32 = 1;

pub const fn integration_instance_schema_version() -> u32 {
    INTEGRATION_INSTANCE_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationManagement {
    UserManaged,
    AdministratorManaged,
    SystemManaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStatus {
    NotConfigured,
    Configuring,
    Connecting,
    Ready,
    Degraded { reason: IntegrationIssue },
    NeedsAttention { reason: IntegrationIssue },
    Disabled,
    Unsupported { reason: String },
}

impl IntegrationStatus {
    pub fn as_user_status(&self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Configuring => "configuring",
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Degraded { .. } => "degraded",
            Self::NeedsAttention { .. } => "needs_attention",
            Self::Disabled => "disabled",
            Self::Unsupported { .. } => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationIssue {
    AuthenticationRequired,
    AuthenticationExpired,
    PermissionDenied,
    NetworkUnavailable,
    ServiceUnavailable,
    ConfigurationInvalid,
    RuntimeUnavailable,
    HealthCheckFailed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserIntegrationDto {
    pub id: IntegrationInstanceId,
    pub definition_id: IntegrationDefinitionId,
    pub display_name: String,
    pub description: String,
    pub category: IntegrationCategory,
    pub status: String,
    pub enabled: bool,
    pub capabilities: Vec<UserCapabilityDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserCapabilityDto {
    pub id: CapabilityId,
    pub display_name: String,
    pub description: String,
    pub access: CapabilityAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdminIntegrationDto {
    pub id: IntegrationInstanceId,
    pub definition_id: IntegrationDefinitionId,
    pub display_name: String,
    pub status: IntegrationStatus,
    pub management: IntegrationManagement,
    pub setup_schema: Vec<SetupField>,
    pub configured_fields: Vec<ConfiguredFieldDto>,
    pub diagnostics: Option<IntegrationDiagnosticsDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfiguredFieldDto {
    pub key: String,
    pub configured: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationDiagnosticsDto {
    pub last_health_check: Option<DateTime<Utc>>,
    pub error_count: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "details", rename_all = "snake_case")]
pub enum IntegrationCommandError {
    DefinitionNotFound,
    InstanceNotFound,
    AlreadyConfigured,
    ConfigurationInvalid { fields: Vec<FieldError> },
    AuthenticationRequired,
    PermissionDenied,
    RuntimeUnavailable,
    HealthCheckFailed,
    SecretStoreUnavailable,
    Persistence { message: String },
    UnsupportedSchemaVersion { version: u32 },
    Internal { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub key: String,
    pub message: String,
}

#[async_trait]
pub trait IntegrationCatalogRepository: Send + Sync {
    async fn list_definitions(&self)
        -> Result<Vec<IntegrationDefinition>, IntegrationCommandError>;
    async fn get_definition(
        &self,
        id: &IntegrationDefinitionId,
    ) -> Result<IntegrationDefinition, IntegrationCommandError>;
}

#[async_trait]
pub trait IntegrationInstanceRepository: Send + Sync {
    async fn create(&self, instance: IntegrationInstance) -> Result<(), IntegrationCommandError>;
    async fn get(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<IntegrationInstance, IntegrationCommandError>;
    async fn list(&self) -> Result<Vec<IntegrationInstance>, IntegrationCommandError>;
    async fn save(&self, instance: IntegrationInstance) -> Result<(), IntegrationCommandError>;
    async fn delete(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError>;
}

#[async_trait]
pub trait IntegrationSecretStore: Send + Sync {
    async fn set_secret(
        &self,
        instance_id: &IntegrationInstanceId,
        key: &str,
        value: &str,
    ) -> Result<(), IntegrationCommandError>;
    async fn has_secret(
        &self,
        instance_id: &IntegrationInstanceId,
        key: &str,
    ) -> Result<bool, IntegrationCommandError>;
    async fn delete_instance_secrets(
        &self,
        instance_id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError>;
}

#[async_trait]
pub trait IntegrationRuntimePort: Send + Sync {
    async fn configure(
        &self,
        instance: &IntegrationInstance,
    ) -> Result<(), IntegrationCommandError>;
    async fn start(
        &self,
        instance_id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError>;
    async fn stop(
        &self,
        instance_id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError>;
    async fn health(
        &self,
        instance_id: &IntegrationInstanceId,
    ) -> Result<IntegrationStatus, IntegrationCommandError>;
    /// Invokes a curated product capability. Implementations must not expose
    /// provider operation names or credentials outside the integration layer.
    async fn invoke_capability(
        &self,
        instance: &IntegrationInstance,
        capability: &CapabilityId,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, IntegrationCommandError> {
        let _ = (instance, capability, input);
        Err(IntegrationCommandError::RuntimeUnavailable)
    }

    async fn capability_available(
        &self,
        instance: &IntegrationInstance,
        capability: &CapabilityId,
    ) -> Result<bool, IntegrationCommandError> {
        let _ = (instance, capability);
        Ok(true)
    }
}

pub struct StaticIntegrationCatalogRepository;

#[async_trait]
impl IntegrationCatalogRepository for StaticIntegrationCatalogRepository {
    async fn list_definitions(
        &self,
    ) -> Result<Vec<IntegrationDefinition>, IntegrationCommandError> {
        Ok(curated_integration_catalog())
    }

    async fn get_definition(
        &self,
        id: &IntegrationDefinitionId,
    ) -> Result<IntegrationDefinition, IntegrationCommandError> {
        curated_integration_catalog()
            .into_iter()
            .find(|definition| definition.id == *id)
            .ok_or(IntegrationCommandError::DefinitionNotFound)
    }
}

pub fn curated_integration_catalog() -> Vec<IntegrationDefinition> {
    vec![
        definition(
            "gmail",
            "Gmail",
            "Connect Gmail to read and send email.",
            IntegrationCategory::Communication,
            "gmail",
            vec![
                capability(
                    "email.read",
                    "Read email",
                    "Read messages and labels.",
                    CapabilityAccess::Read,
                ),
                capability(
                    "email.send",
                    "Send email",
                    "Send email messages.",
                    CapabilityAccess::Write,
                ),
            ],
            vec![
                field("email", "Email address", false, false),
                field("app_password", "App password", true, true),
            ],
        ),
        definition(
            "jira",
            "Jira",
            "Connect Jira projects and issues.",
            IntegrationCategory::ProjectManagement,
            "jira",
            vec![
                capability(
                    "issue.read",
                    "Read issues",
                    "Read Jira issues.",
                    CapabilityAccess::Read,
                ),
                capability(
                    "issue.write",
                    "Update issues",
                    "Create and update Jira issues.",
                    CapabilityAccess::Write,
                ),
                capability(
                    "issue.transition",
                    "Transition issues",
                    "Change issue workflow status.",
                    CapabilityAccess::Execute,
                ),
            ],
            vec![
                field("base_url", "Jira URL", true, false),
                field("email", "Email address", true, false),
                field("api_token", "API token", true, true),
            ],
        ),
        definition(
            "confluence",
            "Confluence",
            "Connect Confluence knowledge spaces.",
            IntegrationCategory::Knowledge,
            "confluence",
            vec![
                capability(
                    "page.read",
                    "Read pages",
                    "Read Confluence pages.",
                    CapabilityAccess::Read,
                ),
                capability(
                    "page.write",
                    "Write pages",
                    "Create and update pages.",
                    CapabilityAccess::Write,
                ),
            ],
            vec![
                field("base_url", "Confluence URL", true, false),
                field("email", "Email address", true, false),
                field("api_token", "API token", true, true),
            ],
        ),
    ]
}

fn definition(
    id: &str,
    display_name: &str,
    description: &str,
    category: IntegrationCategory,
    icon: &str,
    capabilities: Vec<IntegrationCapability>,
    setup_schema: Vec<SetupField>,
) -> IntegrationDefinition {
    IntegrationDefinition {
        id: IntegrationDefinitionId(id.into()),
        display_name: display_name.into(),
        description: description.into(),
        category,
        icon: icon.into(),
        capabilities,
        setup_schema,
        availability: IntegrationAvailability::Available,
    }
}

fn capability(
    id: &str,
    display_name: &str,
    description: &str,
    access: CapabilityAccess,
) -> IntegrationCapability {
    IntegrationCapability {
        id: CapabilityId(id.into()),
        display_name: display_name.into(),
        description: description.into(),
        access,
    }
}

fn field(key: &str, label: &str, required: bool, secret: bool) -> SetupField {
    SetupField {
        key: key.into(),
        label: label.into(),
        description: None,
        field_type: if secret {
            SetupFieldType::Password
        } else {
            SetupFieldType::Text
        },
        required,
        secret,
        default_value: None,
        validation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(definition_id: &str) -> IntegrationInstance {
        IntegrationInstance {
            version: INTEGRATION_INSTANCE_SCHEMA_VERSION,
            id: IntegrationInstanceId(Uuid::new_v4()),
            definition_id: IntegrationDefinitionId(definition_id.into()),
            display_name: "Test".into(),
            status: IntegrationStatus::Ready,
            enabled: true,
            management: IntegrationManagement::UserManaged,
            configured_capabilities: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn definition_and_instance_have_different_identities() {
        let definition = IntegrationDefinitionId("gmail".into());
        let instance = instance("gmail");
        assert_eq!(instance.definition_id, definition);
        assert_ne!(instance.id.to_string(), definition.to_string());
    }

    #[test]
    fn instances_of_one_definition_do_not_conflict() {
        let first = instance("gmail");
        let second = instance("gmail");
        assert_eq!(first.definition_id, second.definition_id);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn user_dto_excludes_administrative_data() {
        let dto = UserIntegrationDto {
            id: IntegrationInstanceId(Uuid::new_v4()),
            definition_id: IntegrationDefinitionId("gmail".into()),
            display_name: "Work Gmail".into(),
            description: "Email".into(),
            category: IntegrationCategory::Communication,
            status: "ready".into(),
            enabled: true,
            capabilities: vec![],
        };
        let value = serde_json::to_value(dto).unwrap();
        assert!(value.get("setup_schema").is_none());
        assert!(value.get("diagnostics").is_none());
    }

    #[test]
    fn admin_dto_hides_secret_values() {
        let field = curated_integration_catalog()[0].setup_schema[1].clone();
        assert!(field.secret);
        let dto = AdminIntegrationDto {
            id: IntegrationInstanceId(Uuid::new_v4()),
            definition_id: IntegrationDefinitionId("gmail".into()),
            display_name: "Work Gmail".into(),
            status: IntegrationStatus::Ready,
            management: IntegrationManagement::UserManaged,
            setup_schema: vec![field],
            configured_fields: vec![ConfiguredFieldDto {
                key: "app_password".into(),
                configured: true,
                value: None,
            }],
            diagnostics: None,
        };
        let value = serde_json::to_value(dto).unwrap();
        assert!(value["configured_fields"][0]["value"].is_null());
    }

    #[tokio::test]
    async fn unknown_definition_returns_typed_error() {
        let error = StaticIntegrationCatalogRepository
            .get_definition(&IntegrationDefinitionId("missing".into()))
            .await
            .unwrap_err();
        assert_eq!(error, IntegrationCommandError::DefinitionNotFound);
    }

    #[test]
    fn entities_and_dtos_serialize_to_json() {
        let entity = curated_integration_catalog().remove(0);
        let dto = UserIntegrationDto {
            id: IntegrationInstanceId(Uuid::new_v4()),
            definition_id: entity.id.clone(),
            display_name: entity.display_name.clone(),
            description: entity.description.clone(),
            category: entity.category.clone(),
            status: "ready".into(),
            enabled: true,
            capabilities: vec![],
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&entity).unwrap())
                .unwrap()["id"],
            "gmail"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&dto).unwrap())
                .unwrap()["status"],
            "ready"
        );
    }

    #[test]
    fn statuses_cover_transitions() {
        let statuses = [
            IntegrationStatus::NotConfigured,
            IntegrationStatus::Configuring,
            IntegrationStatus::Connecting,
            IntegrationStatus::Ready,
            IntegrationStatus::Degraded {
                reason: IntegrationIssue::NetworkUnavailable,
            },
            IntegrationStatus::NeedsAttention {
                reason: IntegrationIssue::AuthenticationRequired,
            },
            IntegrationStatus::Disabled,
            IntegrationStatus::Unsupported {
                reason: "platform".into(),
            },
        ];
        assert_eq!(statuses[3].as_user_status(), "ready");
        assert_eq!(statuses[4].as_user_status(), "degraded");
    }
}
