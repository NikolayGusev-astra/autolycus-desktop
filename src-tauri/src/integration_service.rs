//! Application service for integration configuration.  This module deliberately
//! depends only on the integration domain ports and types.

use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::capability_router::{CapabilityIntegrationSource, CapabilityRoutingError};
use crate::integration_domain::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct IntegrationActor {
    pub is_admin: bool,
}

#[derive(Clone)]
pub struct ConfigureIntegrationRequest {
    pub definition_id: IntegrationDefinitionId,
    pub instance_id: Option<IntegrationInstanceId>,
    pub display_name: String,
    pub fields: Vec<ConfiguredFieldValue>,
    pub management: Option<IntegrationManagement>,
    pub enabled_capabilities: Vec<CapabilityId>,
}

impl fmt::Debug for ConfigureIntegrationRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigureIntegrationRequest")
            .field("definition_id", &self.definition_id)
            .field("instance_id", &self.instance_id)
            .field("display_name", &self.display_name)
            .field("fields", &self.fields)
            .field("management", &self.management)
            .field("enabled_capabilities", &self.enabled_capabilities)
            .finish()
    }
}

#[derive(Clone)]
pub struct ConfiguredFieldValue {
    pub key: String,
    pub value: String,
}

impl fmt::Debug for ConfiguredFieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfiguredFieldValue")
            .field("key", &self.key)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct IntegrationService {
    catalog: Arc<dyn IntegrationCatalogRepository>,
    instances: Arc<dyn IntegrationInstanceRepository>,
    secrets: Arc<dyn IntegrationSecretStore>,
    runtime: Arc<dyn IntegrationRuntimePort>,
    instance_locks: Arc<Mutex<HashMap<IntegrationInstanceId, Arc<Mutex<()>>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IntegrationRecoveryReport {
    pub loaded: usize,
    pub re_enabled: usize,
    pub failed: usize,
}

impl IntegrationService {
    pub fn new(
        catalog: Arc<dyn IntegrationCatalogRepository>,
        instances: Arc<dyn IntegrationInstanceRepository>,
        secrets: Arc<dyn IntegrationSecretStore>,
        runtime: Arc<dyn IntegrationRuntimePort>,
    ) -> Self {
        Self {
            catalog,
            instances,
            secrets,
            runtime,
            instance_locks: Arc::default(),
        }
    }

    pub async fn list_available_integrations(
        &self,
    ) -> Result<Vec<IntegrationDefinition>, IntegrationCommandError> {
        self.catalog.list_definitions().await
    }

    pub async fn list_configured_integrations(
        &self,
    ) -> Result<Vec<IntegrationInstance>, IntegrationCommandError> {
        self.instances.list().await
    }

    pub async fn get_integration(
        &self,
        _actor: &IntegrationActor,
        instance_id: &IntegrationInstanceId,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        let instance = self.instances.get(instance_id).await?;
        self.admin_dto(instance).await
    }

    pub async fn get_user_integration(
        &self,
        instance_id: &IntegrationInstanceId,
    ) -> Result<UserIntegrationDto, IntegrationCommandError> {
        let instance = self.instances.get(instance_id).await?;
        let definition = self.catalog.get_definition(&instance.definition_id).await?;
        Ok(UserIntegrationDto {
            id: instance.id,
            definition_id: instance.definition_id,
            display_name: instance.display_name,
            description: definition.description,
            category: definition.category,
            status: instance.status.as_user_status().into(),
            enabled: instance.enabled,
            capabilities: definition
                .capabilities
                .into_iter()
                .filter(|capability| instance.configured_capabilities.contains(&capability.id))
                .map(|capability| UserCapabilityDto {
                    id: capability.id,
                    display_name: capability.display_name,
                    description: capability.description,
                    access: capability.access,
                })
                .collect(),
        })
    }

    pub async fn configure_integration(
        &self,
        actor: &IntegrationActor,
        request: ConfigureIntegrationRequest,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        let definition = self.catalog.get_definition(&request.definition_id).await?;
        self.validate_request(&definition, &request)?;
        let existing = match &request.instance_id {
            Some(id) => Some(self.instances.get(id).await?),
            None => self.instances.list().await?.into_iter().find(|item| {
                item.definition_id == request.definition_id
                    && item.display_name == request.display_name
            }),
        };
        if let Some(ref item) = existing {
            self.ensure_mutable(actor, item)?;
        }
        let id = existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| IntegrationInstanceId(Uuid::new_v4()));
        let _guard = self.lock(&id).await;
        let previous = existing.clone();
        let secret_presence = self.secret_presence(&definition, &id).await?;
        let now = Utc::now();
        let mut instance = previous.clone().unwrap_or(IntegrationInstance {
            version: INTEGRATION_INSTANCE_SCHEMA_VERSION,
            id: id.clone(),
            definition_id: definition.id.clone(),
            display_name: request.display_name.clone(),
            status: IntegrationStatus::Configuring,
            enabled: true,
            management: request
                .management
                .clone()
                .unwrap_or(IntegrationManagement::UserManaged),
            configured_capabilities: request.enabled_capabilities.clone(),
            created_at: now,
            updated_at: now,
        });
        instance.definition_id = definition.id.clone();
        instance.display_name = request.display_name;
        instance.management = request.management.unwrap_or(instance.management);
        if matches!(
            instance.management,
            IntegrationManagement::AdministratorManaged
        ) && !actor.is_admin
        {
            return Err(IntegrationCommandError::PermissionDenied);
        }
        instance.configured_capabilities = request.enabled_capabilities;
        instance.status = IntegrationStatus::Configuring;
        instance.updated_at = now;

        let result = async {
            for field in request.fields.iter().filter(|value| {
                definition
                    .setup_schema
                    .iter()
                    .any(|schema| schema.key == value.key && schema.secret)
            }) {
                self.secrets
                    .set_secret(&id, &field.key, &field.value)
                    .await?;
            }
            if previous.is_some() {
                self.instances.save(instance.clone()).await?;
            } else {
                self.instances.create(instance.clone()).await?;
            }
            self.runtime.configure(&instance).await?;
            let mut final_instance = instance.clone();
            final_instance.status = self.health_status(&id).await;
            final_instance.updated_at = Utc::now();
            self.instances.save(final_instance.clone()).await?;
            Ok::<_, IntegrationCommandError>(final_instance)
        }
        .await;
        match result {
            Ok(instance) => {
                self.event(&definition.id, &id, "configure", "ok");
                self.admin_dto(instance).await
            }
            Err(error) => {
                self.restore(&id, previous, &secret_presence).await;
                self.event(&definition.id, &id, "configure", "error");
                Err(error)
            }
        }
    }

    pub async fn enable_integration(
        &self,
        actor: &IntegrationActor,
        id: &IntegrationInstanceId,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        let _guard = self.lock(id).await;
        let mut instance = self.instances.get(id).await?;
        self.ensure_mutable(actor, &instance)?;
        if !instance.enabled {
            self.runtime.start(id).await?;
            instance.enabled = true;
            instance.status = self.health_status(id).await;
            instance.updated_at = Utc::now();
            self.instances.save(instance.clone()).await?;
        }
        self.event(&instance.definition_id, id, "enable", "ok");
        self.admin_dto(instance).await
    }

    pub async fn disable_integration(
        &self,
        actor: &IntegrationActor,
        id: &IntegrationInstanceId,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        let _guard = self.lock(id).await;
        let mut instance = self.instances.get(id).await?;
        self.ensure_mutable(actor, &instance)?;
        if instance.enabled {
            self.runtime.stop(id).await?;
            instance.enabled = false;
            instance.status = IntegrationStatus::Disabled;
            instance.updated_at = Utc::now();
            self.instances.save(instance.clone()).await?;
        }
        self.event(&instance.definition_id, id, "disable", "ok");
        self.admin_dto(instance).await
    }

    pub async fn test_integration(
        &self,
        actor: &IntegrationActor,
        id: &IntegrationInstanceId,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        self.refresh_integration_status(actor, id).await
    }

    pub async fn refresh_integration_status(
        &self,
        actor: &IntegrationActor,
        id: &IntegrationInstanceId,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        let _guard = self.lock(id).await;
        let mut instance = self.instances.get(id).await?;
        self.ensure_mutable(actor, &instance)?;
        if instance.enabled {
            instance.status = self.health_status(id).await;
            instance.updated_at = Utc::now();
            self.instances.save(instance.clone()).await?;
        }
        self.event(&instance.definition_id, id, "refresh_status", "ok");
        self.admin_dto(instance).await
    }

    pub async fn remove_integration(
        &self,
        actor: &IntegrationActor,
        id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError> {
        let _guard = self.lock(id).await;
        let instance = self.instances.get(id).await?;
        self.ensure_mutable(actor, &instance)?;
        self.runtime.stop(id).await?;
        self.secrets.delete_instance_secrets(id).await?;
        self.instances.delete(id).await?;
        self.event(&instance.definition_id, id, "remove", "ok");
        Ok(())
    }

    pub async fn recover_instances(
        &self,
    ) -> Result<IntegrationRecoveryReport, IntegrationCommandError> {
        let instances = self.instances.list().await?;
        let mut report = IntegrationRecoveryReport {
            loaded: instances.len(),
            re_enabled: 0,
            failed: 0,
        };
        for mut instance in instances.into_iter().filter(|item| item.enabled) {
            let _guard = self.lock(&instance.id).await;
            // Runtime adapters keep only live process state in memory. Rebuild
            // that state from the persisted instance before reconnecting it.
            let startup = self.runtime.configure(&instance).await;
            let startup = match startup {
                Ok(()) => self.runtime.start(&instance.id).await,
                Err(error) => Err(error),
            };
            match startup {
                Ok(()) => {
                    instance.status = self.health_status(&instance.id).await;
                    instance.updated_at = Utc::now();
                    if self.instances.save(instance).await.is_ok() {
                        report.re_enabled += 1;
                    } else {
                        report.failed += 1;
                    }
                }
                Err(_) => {
                    instance.status = IntegrationStatus::Degraded {
                        reason: IntegrationIssue::RuntimeUnavailable,
                    };
                    instance.updated_at = Utc::now();
                    let _ = self.instances.save(instance).await;
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }

    async fn lock(&self, id: &IntegrationInstanceId) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.instance_locks.lock().await;
            locks
                .entry(id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }

    fn ensure_mutable(
        &self,
        actor: &IntegrationActor,
        item: &IntegrationInstance,
    ) -> Result<(), IntegrationCommandError> {
        if matches!(item.management, IntegrationManagement::AdministratorManaged) && !actor.is_admin
        {
            Err(IntegrationCommandError::PermissionDenied)
        } else {
            Ok(())
        }
    }

    fn validate_request(
        &self,
        definition: &IntegrationDefinition,
        request: &ConfigureIntegrationRequest,
    ) -> Result<(), IntegrationCommandError> {
        let mut errors = Vec::new();
        let supplied: HashMap<&str, &str> = request
            .fields
            .iter()
            .map(|field| (field.key.as_str(), field.value.as_str()))
            .collect();
        for field in &definition.setup_schema {
            let value = supplied
                .get(field.key.as_str())
                .copied()
                .or(field.default_value.as_deref());
            if field.required && value.is_none_or(str::is_empty) {
                errors.push(FieldError {
                    key: field.key.clone(),
                    message: "required".into(),
                });
                continue;
            }
            if let Some(value) = value {
                if !valid_field(field, value) {
                    errors.push(FieldError {
                        key: field.key.clone(),
                        message: "invalid value".into(),
                    });
                }
            }
        }
        for field in &request.fields {
            if !definition
                .setup_schema
                .iter()
                .any(|schema| schema.key == field.key)
            {
                errors.push(FieldError {
                    key: field.key.clone(),
                    message: "unknown field".into(),
                });
            }
        }
        for capability in &request.enabled_capabilities {
            if !definition
                .capabilities
                .iter()
                .any(|allowed| allowed.id == *capability)
            {
                errors.push(FieldError {
                    key: capability.0.clone(),
                    message: "unsupported capability".into(),
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(IntegrationCommandError::ConfigurationInvalid { fields: errors })
        }
    }

    async fn secret_presence(
        &self,
        definition: &IntegrationDefinition,
        id: &IntegrationInstanceId,
    ) -> Result<HashMap<String, bool>, IntegrationCommandError> {
        let mut presence = HashMap::new();
        for field in definition.setup_schema.iter().filter(|field| field.secret) {
            presence.insert(
                field.key.clone(),
                self.secrets.has_secret(id, &field.key).await?,
            );
        }
        Ok(presence)
    }

    async fn restore(
        &self,
        id: &IntegrationInstanceId,
        previous: Option<IntegrationInstance>,
        old_secrets: &HashMap<String, bool>,
    ) {
        // Ports intentionally never reveal secret values; restoration can preserve
        // presence only.  Rollback errors are intentionally ignored.
        match previous {
            None => {
                let _ = self.runtime.stop(id).await;
                let _ = self.secrets.delete_instance_secrets(id).await;
                let _ = self.instances.delete(id).await;
            }
            Some(previous) => {
                let _ = self.instances.save(previous).await;
                if old_secrets.values().all(|present| !present) {
                    let _ = self.secrets.delete_instance_secrets(id).await;
                }
            }
        }
    }

    async fn health_status(&self, id: &IntegrationInstanceId) -> IntegrationStatus {
        match self.runtime.health(id).await {
            Ok(status) => normalize_status(status),
            Err(_) => IntegrationStatus::Degraded {
                reason: IntegrationIssue::HealthCheckFailed,
            },
        }
    }

    async fn admin_dto(
        &self,
        instance: IntegrationInstance,
    ) -> Result<AdminIntegrationDto, IntegrationCommandError> {
        let definition = self.catalog.get_definition(&instance.definition_id).await?;
        let mut configured_fields = Vec::with_capacity(definition.setup_schema.len());
        for field in &definition.setup_schema {
            configured_fields.push(ConfiguredFieldDto {
                key: field.key.clone(),
                configured: if field.secret {
                    self.secrets.has_secret(&instance.id, &field.key).await?
                } else {
                    false
                },
                value: None,
            });
        }
        Ok(AdminIntegrationDto {
            id: instance.id,
            definition_id: instance.definition_id,
            display_name: instance.display_name,
            status: instance.status,
            management: instance.management,
            setup_schema: definition.setup_schema,
            configured_fields,
            diagnostics: None,
        })
    }

    fn event(
        &self,
        definition_id: &IntegrationDefinitionId,
        instance_id: &IntegrationInstanceId,
        operation: &'static str,
        result: &'static str,
    ) {
        info!(%definition_id, %instance_id, operation, result, "integration operation");
    }
}

#[async_trait::async_trait]
impl CapabilityIntegrationSource for IntegrationService {
    async fn list_instances(&self) -> Result<Vec<IntegrationInstance>, CapabilityRoutingError> {
        self.list_configured_integrations()
            .await
            .map_err(|_| CapabilityRoutingError::InstanceUnavailable)
    }
}

fn normalize_status(status: IntegrationStatus) -> IntegrationStatus {
    match status {
        IntegrationStatus::Configuring
        | IntegrationStatus::Connecting
        | IntegrationStatus::NotConfigured => IntegrationStatus::Degraded {
            reason: IntegrationIssue::HealthCheckFailed,
        },
        other => other,
    }
}

fn valid_field(field: &SetupField, value: &str) -> bool {
    let typed = match &field.field_type {
        SetupFieldType::Number => value.parse::<f64>().is_ok(),
        SetupFieldType::Boolean => matches!(value, "true" | "false"),
        SetupFieldType::Url => value.starts_with("http://") || value.starts_with("https://"),
        SetupFieldType::Select { options } => options.iter().any(|option| option.value == value),
        _ => true,
    };
    typed
        && field.validation.as_ref().is_none_or(|validation| {
            validation
                .min_length
                .is_none_or(|minimum| value.len() >= minimum)
                && validation
                    .max_length
                    .is_none_or(|maximum| value.len() <= maximum)
                && validation.pattern.as_ref().is_none_or(|pattern| {
                    regex::Regex::new(pattern).is_ok_and(|expression| expression.is_match(value))
                })
        })
}

/// JSON-backed production repository. Each instance is isolated so a damaged
/// file cannot hide or overwrite another configured integration.
pub struct FileInstanceRepository {
    instances_dir: PathBuf,
}

impl FileInstanceRepository {
    pub fn new(hermes_home: impl AsRef<Path>) -> Self {
        Self {
            instances_dir: hermes_home.as_ref().join("integrations").join("instances"),
        }
    }

    fn path_for(&self, id: &IntegrationInstanceId) -> PathBuf {
        self.instances_dir.join(format!("{id}.json"))
    }

    fn persistence_error(error: impl fmt::Display) -> IntegrationCommandError {
        IntegrationCommandError::Persistence {
            message: error.to_string(),
        }
    }

    fn write(&self, instance: &IntegrationInstance) -> Result<(), IntegrationCommandError> {
        fs::create_dir_all(&self.instances_dir).map_err(Self::persistence_error)?;
        let json = serde_json::to_vec_pretty(instance).map_err(Self::persistence_error)?;
        fs::write(self.path_for(&instance.id), json).map_err(Self::persistence_error)
    }

    fn read(&self, path: &Path) -> Result<IntegrationInstance, IntegrationCommandError> {
        let contents = fs::read(path).map_err(Self::persistence_error)?;
        let instance: IntegrationInstance =
            serde_json::from_slice(&contents).map_err(Self::persistence_error)?;
        if instance.version != INTEGRATION_INSTANCE_SCHEMA_VERSION {
            return Err(IntegrationCommandError::UnsupportedSchemaVersion {
                version: instance.version,
            });
        }
        Ok(instance)
    }
}

#[async_trait::async_trait]
impl IntegrationInstanceRepository for FileInstanceRepository {
    async fn create(&self, instance: IntegrationInstance) -> Result<(), IntegrationCommandError> {
        if self.path_for(&instance.id).exists() {
            return Err(IntegrationCommandError::AlreadyConfigured);
        }
        self.write(&instance)
    }

    async fn get(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<IntegrationInstance, IntegrationCommandError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(IntegrationCommandError::InstanceNotFound);
        }
        self.read(&path)
    }

    async fn list(&self) -> Result<Vec<IntegrationInstance>, IntegrationCommandError> {
        if !self.instances_dir.exists() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&self.instances_dir).map_err(Self::persistence_error)?;
        let mut instances = Vec::new();
        for entry in entries {
            let path = entry.map_err(Self::persistence_error)?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                instances.push(self.read(&path)?);
            }
        }
        instances.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
        Ok(instances)
    }

    async fn save(&self, instance: IntegrationInstance) -> Result<(), IntegrationCommandError> {
        self.write(&instance)
    }

    async fn delete(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(IntegrationCommandError::InstanceNotFound);
        }
        fs::remove_file(path).map_err(Self::persistence_error)
    }
}

/// Base64 is obfuscation, not encryption. It keeps credentials out of normal
/// text files until the application has a platform key management layer.
pub struct FileSecretStore {
    secrets_dir: PathBuf,
}

impl FileSecretStore {
    pub fn new(hermes_home: impl AsRef<Path>) -> Self {
        Self {
            secrets_dir: hermes_home.as_ref().join("integrations").join("secrets"),
        }
    }

    fn key_path(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
    ) -> Result<PathBuf, IntegrationCommandError> {
        if key.is_empty() || Path::new(key).components().count() != 1 {
            return Err(IntegrationCommandError::Persistence {
                message: "invalid secret key".into(),
            });
        }
        Ok(self.secrets_dir.join(id.to_string()).join(key))
    }
}

#[async_trait::async_trait]
impl IntegrationSecretStore for FileSecretStore {
    async fn set_secret(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
        value: &str,
    ) -> Result<(), IntegrationCommandError> {
        let path = self.key_path(id, key)?;
        let parent = path.parent().expect("secret path has a parent");
        fs::create_dir_all(parent).map_err(FileInstanceRepository::persistence_error)?;
        fs::write(path, BASE64.encode(value)).map_err(FileInstanceRepository::persistence_error)
    }

    async fn has_secret(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
    ) -> Result<bool, IntegrationCommandError> {
        let path = self.key_path(id, key)?;
        if !path.exists() {
            return Ok(false);
        }
        let value = fs::read_to_string(path).map_err(FileInstanceRepository::persistence_error)?;
        BASE64
            .decode(value)
            .map_err(FileInstanceRepository::persistence_error)?;
        Ok(true)
    }

    async fn delete_instance_secrets(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError> {
        let path = self.secrets_dir.join(id.to_string());
        if path.exists() {
            fs::remove_dir_all(path).map_err(FileInstanceRepository::persistence_error)?;
        }
        Ok(())
    }
}

const INTEGRATION_SECRET_SERVICE: &str = "autolycus-integrations";
const SECRET_INDEX_KEY: &str = "__autolycus_secret_keys";
const MACHINE_KEY_SALT: &[u8] = b"autolycus-integrations-encrypted-file-store-v1";

/// Minimal keyring boundary. Keeping it separate from `keyring::Entry` lets the
/// secret store be tested without creating credentials in the user's profile.
trait CredentialBackend: Send + Sync {
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

struct SystemCredentialBackend;

impl CredentialBackend for SystemCredentialBackend {
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(INTEGRATION_SECRET_SERVICE, account)
            .map_err(|error| error.to_string())?
            .set_password(value)
            .map_err(|error| error.to_string())
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(INTEGRATION_SECRET_SERVICE, account)
            .map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(INTEGRATION_SECRET_SERVICE, account)
            .map_err(|error| error.to_string())?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Encrypted, machine-bound fallback for hosts without a usable OS keyring.
/// This is deliberately distinct from the legacy base64 `FileSecretStore`.
struct EncryptedFileSecretStore {
    secrets_dir: PathBuf,
    key: [u8; 32],
}

impl EncryptedFileSecretStore {
    fn new(hermes_home: impl AsRef<Path>) -> Self {
        let mut digest = Sha256::new();
        digest.update(machine_identifier().as_bytes());
        digest.update(MACHINE_KEY_SALT);
        Self {
            secrets_dir: hermes_home
                .as_ref()
                .join("integrations")
                .join("encrypted-secrets"),
            key: digest.finalize().into(),
        }
    }

    fn key_path(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
    ) -> Result<PathBuf, IntegrationCommandError> {
        validate_secret_key(key)?;
        Ok(self
            .secrets_dir
            .join(id.to_string())
            .join(format!("{key}.enc")))
    }

    fn set(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
        value: &str,
    ) -> Result<(), IntegrationCommandError> {
        let path = self.key_path(id, key)?;
        let parent = path.parent().expect("secret path has a parent");
        fs::create_dir_all(parent).map_err(FileInstanceRepository::persistence_error)?;
        let nonce_source = Uuid::new_v4();
        let nonce_bytes = &nonce_source.as_bytes()[..12];
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("AES-256 key length is fixed");
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(nonce_bytes), value.as_bytes())
            .map_err(|_| IntegrationCommandError::SecretStoreUnavailable)?;
        let mut stored = nonce_bytes.to_vec();
        stored.extend(ciphertext);
        fs::write(path, stored).map_err(FileInstanceRepository::persistence_error)
    }

    fn has(&self, id: &IntegrationInstanceId, key: &str) -> Result<bool, IntegrationCommandError> {
        let path = self.key_path(id, key)?;
        if !path.exists() {
            return Ok(false);
        }
        let stored = fs::read(path).map_err(FileInstanceRepository::persistence_error)?;
        if stored.len() <= 12 {
            return Err(IntegrationCommandError::SecretStoreUnavailable);
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("AES-256 key length is fixed");
        cipher
            .decrypt(Nonce::from_slice(&stored[..12]), &stored[12..])
            .map_err(|_| IntegrationCommandError::SecretStoreUnavailable)?;
        Ok(true)
    }

    fn delete_instance(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        let path = self.secrets_dir.join(id.to_string());
        if path.exists() {
            fs::remove_dir_all(path).map_err(FileInstanceRepository::persistence_error)?;
        }
        Ok(())
    }
}

fn machine_identifier() -> String {
    #[cfg(windows)]
    if let Ok(hklm) = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
    {
        if let Ok(value) = hklm.get_value::<String, _>("MachineGuid") {
            return value;
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(value) = fs::read_to_string("/etc/machine-id") {
        return value.trim().to_owned();
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-machine".into())
}

fn validate_secret_key(key: &str) -> Result<(), IntegrationCommandError> {
    if key.is_empty() || Path::new(key).components().count() != 1 {
        return Err(IntegrationCommandError::Persistence {
            message: "invalid secret key".into(),
        });
    }
    Ok(())
}

/// Stores integration secrets in the platform credential manager. When that
/// service is unavailable (notably headless Linux), it uses AES-256-GCM files
/// derived from the machine identifier and application salt instead.
pub struct OsSecretStore {
    legacy: FileSecretStore,
    encrypted_fallback: EncryptedFileSecretStore,
    credentials: Arc<dyn CredentialBackend>,
    use_fallback: AtomicBool,
    migration_lock: Mutex<()>,
}

impl OsSecretStore {
    pub fn new(hermes_home: impl AsRef<Path>) -> Self {
        Self::with_backend(hermes_home, Arc::new(SystemCredentialBackend))
    }

    fn with_backend(
        hermes_home: impl AsRef<Path>,
        credentials: Arc<dyn CredentialBackend>,
    ) -> Self {
        Self {
            legacy: FileSecretStore::new(&hermes_home),
            encrypted_fallback: EncryptedFileSecretStore::new(hermes_home),
            credentials,
            use_fallback: AtomicBool::new(false),
            migration_lock: Mutex::new(()),
        }
    }

    fn account(id: &IntegrationInstanceId, key: &str) -> String {
        format!("{id}:{key}")
    }

    fn fallback(&self, error: &str) {
        if !self.use_fallback.swap(true, Ordering::AcqRel) {
            warn!(
                error,
                "OS keyring unavailable; using encrypted integration secret files"
            );
        }
    }

    fn keys(&self, id: &IntegrationInstanceId) -> Result<Vec<String>, String> {
        match self.credentials.get(&Self::account(id, SECRET_INDEX_KEY))? {
            Some(value) => serde_json::from_str(&value).map_err(|error| error.to_string()),
            None => Ok(Vec::new()),
        }
    }

    fn store_keyring(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
        value: &str,
    ) -> Result<(), String> {
        self.credentials.set(&Self::account(id, key), value)?;
        let mut keys = self.keys(id)?;
        if !keys.iter().any(|item| item == key) {
            keys.push(key.to_owned());
            self.credentials.set(
                &Self::account(id, SECRET_INDEX_KEY),
                &serde_json::to_string(&keys).expect("secret keys serialize"),
            )?;
        }
        Ok(())
    }

    async fn migrate_legacy(&self, id: &IntegrationInstanceId) {
        let _guard = self.migration_lock.lock().await;
        let directory = self.legacy.secrets_dir.join(id.to_string());
        let Ok(entries) = fs::read_dir(&directory) else {
            return;
        };
        let mut migrated = 0usize;
        let mut failures = 0usize;
        for entry in entries.flatten() {
            let Some(key) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let result = fs::read_to_string(entry.path())
                .map_err(|error| error.to_string())
                .and_then(|encoded| BASE64.decode(encoded).map_err(|error| error.to_string()))
                .and_then(|value| String::from_utf8(value).map_err(|error| error.to_string()))
                .and_then(|value| {
                    if self.use_fallback.load(Ordering::Acquire) {
                        self.encrypted_fallback
                            .set(id, &key, &value)
                            .map_err(|error| format!("{error:?}"))
                    } else {
                        self.store_keyring(id, &key, &value).or_else(|error| {
                            self.fallback(&error);
                            self.encrypted_fallback
                                .set(id, &key, &value)
                                .map_err(|fallback| format!("{fallback:?}"))
                        })
                    }
                });
            match result {
                Ok(()) => {
                    if fs::remove_file(entry.path()).is_ok() {
                        migrated += 1
                    } else {
                        failures += 1
                    }
                }
                Err(_) => failures += 1,
            }
        }
        if migrated > 0 || failures > 0 {
            info!(%id, migrated, failures, "migrated legacy integration secrets");
        }
        if fs::read_dir(&directory).is_ok_and(|mut entries| entries.next().is_none()) {
            let _ = fs::remove_dir(&directory);
        }
    }
}

#[async_trait::async_trait]
impl IntegrationSecretStore for OsSecretStore {
    async fn set_secret(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
        value: &str,
    ) -> Result<(), IntegrationCommandError> {
        validate_secret_key(key)?;
        self.migrate_legacy(id).await;
        if self.use_fallback.load(Ordering::Acquire) {
            return self.encrypted_fallback.set(id, key, value);
        }
        if let Err(error) = self.store_keyring(id, key, value) {
            self.fallback(&error);
            self.encrypted_fallback.set(id, key, value)?;
        }
        Ok(())
    }

    async fn has_secret(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
    ) -> Result<bool, IntegrationCommandError> {
        validate_secret_key(key)?;
        self.migrate_legacy(id).await;
        if self.use_fallback.load(Ordering::Acquire) {
            return self.encrypted_fallback.has(id, key);
        }
        match self.credentials.get(&Self::account(id, key)) {
            Ok(value) => Ok(value.is_some()),
            Err(error) => {
                self.fallback(&error);
                self.encrypted_fallback.has(id, key)
            }
        }
    }

    async fn delete_instance_secrets(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError> {
        self.migrate_legacy(id).await;
        if self.use_fallback.load(Ordering::Acquire) {
            return self.encrypted_fallback.delete_instance(id);
        }
        match self.keys(id) {
            Ok(keys) => {
                for key in keys {
                    if let Err(error) = self.credentials.delete(&Self::account(id, &key)) {
                        self.fallback(&error);
                        return self.encrypted_fallback.delete_instance(id);
                    }
                }
                if let Err(error) = self
                    .credentials
                    .delete(&Self::account(id, SECRET_INDEX_KEY))
                {
                    self.fallback(&error);
                    return self.encrypted_fallback.delete_instance(id);
                }
                Ok(())
            }
            Err(error) => {
                self.fallback(&error);
                self.encrypted_fallback.delete_instance(id)
            }
        }
    }
}

/// In-memory ports for service tests and consumers that need a deterministic fake.
/// `events` records only operation names and ids; it never records secret values.
#[derive(Default)]
pub struct FakeCatalogRepository {
    definitions: Vec<IntegrationDefinition>,
}

impl FakeCatalogRepository {
    pub fn new() -> Self {
        Self {
            definitions: curated_integration_catalog(),
        }
    }
}

#[async_trait::async_trait]
impl IntegrationCatalogRepository for FakeCatalogRepository {
    async fn list_definitions(
        &self,
    ) -> Result<Vec<IntegrationDefinition>, IntegrationCommandError> {
        Ok(self.definitions.clone())
    }
    async fn get_definition(
        &self,
        id: &IntegrationDefinitionId,
    ) -> Result<IntegrationDefinition, IntegrationCommandError> {
        self.definitions
            .iter()
            .find(|definition| definition.id == *id)
            .cloned()
            .ok_or(IntegrationCommandError::DefinitionNotFound)
    }
}

#[derive(Default)]
pub struct FakeInstanceRepository {
    instances: Mutex<HashMap<IntegrationInstanceId, IntegrationInstance>>,
    pub events: Arc<Mutex<Vec<String>>>,
    failure: Mutex<Option<IntegrationCommandError>>,
}

/// Desktop startup uses the deterministic in-memory implementation until a
/// persistent repository is introduced.
pub type InMemoryIntegrationInstanceRepository = FakeInstanceRepository;

impl FakeInstanceRepository {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_events(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            ..Self::default()
        }
    }
    pub async fn fail_next(&self, error: IntegrationCommandError) {
        *self.failure.lock().await = Some(error);
    }
    async fn fail(&self) -> Result<(), IntegrationCommandError> {
        self.failure.lock().await.take().map_or(Ok(()), Err)
    }
}

#[async_trait::async_trait]
impl IntegrationInstanceRepository for FakeInstanceRepository {
    async fn create(&self, item: IntegrationInstance) -> Result<(), IntegrationCommandError> {
        self.fail().await?;
        self.events
            .lock()
            .await
            .push(format!("instance.create:{}", item.id));
        self.instances.lock().await.insert(item.id.clone(), item);
        Ok(())
    }
    async fn get(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<IntegrationInstance, IntegrationCommandError> {
        self.instances
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or(IntegrationCommandError::InstanceNotFound)
    }
    async fn list(&self) -> Result<Vec<IntegrationInstance>, IntegrationCommandError> {
        Ok(self.instances.lock().await.values().cloned().collect())
    }
    async fn save(&self, item: IntegrationInstance) -> Result<(), IntegrationCommandError> {
        self.fail().await?;
        self.events
            .lock()
            .await
            .push(format!("instance.save:{}", item.id));
        self.instances.lock().await.insert(item.id.clone(), item);
        Ok(())
    }
    async fn delete(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        self.fail().await?;
        self.events
            .lock()
            .await
            .push(format!("instance.delete:{id}"));
        self.instances.lock().await.remove(id);
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeSecretStore {
    secrets: Mutex<HashMap<(IntegrationInstanceId, String), String>>,
    pub events: Arc<Mutex<Vec<String>>>,
    failure: Mutex<Option<IntegrationCommandError>>,
}

impl FakeSecretStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_events(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            ..Self::default()
        }
    }
    pub async fn fail_next(&self, error: IntegrationCommandError) {
        *self.failure.lock().await = Some(error);
    }
    async fn fail(&self) -> Result<(), IntegrationCommandError> {
        self.failure.lock().await.take().map_or(Ok(()), Err)
    }
}

#[async_trait::async_trait]
impl IntegrationSecretStore for FakeSecretStore {
    async fn set_secret(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
        value: &str,
    ) -> Result<(), IntegrationCommandError> {
        self.fail().await?;
        self.events
            .lock()
            .await
            .push(format!("secret.set:{id}:{key}"));
        self.secrets
            .lock()
            .await
            .insert((id.clone(), key.into()), value.into());
        Ok(())
    }
    async fn has_secret(
        &self,
        id: &IntegrationInstanceId,
        key: &str,
    ) -> Result<bool, IntegrationCommandError> {
        Ok(self
            .secrets
            .lock()
            .await
            .contains_key(&(id.clone(), key.into())))
    }
    async fn delete_instance_secrets(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError> {
        self.fail().await?;
        self.events.lock().await.push(format!("secret.delete:{id}"));
        self.secrets
            .lock()
            .await
            .retain(|(stored_id, _), _| stored_id != id);
        Ok(())
    }
}

pub struct FakeRuntimePort {
    pub events: Arc<Mutex<Vec<String>>>,
    failures: Mutex<HashMap<String, IntegrationCommandError>>,
    health: Mutex<IntegrationStatus>,
}

impl Default for FakeRuntimePort {
    fn default() -> Self {
        Self {
            events: Arc::default(),
            failures: Mutex::new(HashMap::new()),
            health: Mutex::new(IntegrationStatus::Ready),
        }
    }
}

impl FakeRuntimePort {
    pub fn new() -> Self {
        Self {
            health: Mutex::new(IntegrationStatus::Ready),
            ..Self::default()
        }
    }
    pub fn with_events(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            ..Self::default()
        }
    }
    pub async fn fail_at(&self, operation: &str, error: IntegrationCommandError) {
        self.failures.lock().await.insert(operation.into(), error);
    }
    pub async fn set_health(&self, status: IntegrationStatus) {
        *self.health.lock().await = status;
    }
    async fn call(
        &self,
        operation: &str,
        id: &IntegrationInstanceId,
    ) -> Result<(), IntegrationCommandError> {
        self.events
            .lock()
            .await
            .push(format!("runtime.{operation}:{id}"));
        self.failures
            .lock()
            .await
            .remove(operation)
            .map_or(Ok(()), Err)
    }
}

#[async_trait::async_trait]
impl IntegrationRuntimePort for FakeRuntimePort {
    async fn configure(
        &self,
        instance: &IntegrationInstance,
    ) -> Result<(), IntegrationCommandError> {
        self.call("configure", &instance.id).await
    }
    async fn start(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        self.call("start", id).await
    }
    async fn stop(&self, id: &IntegrationInstanceId) -> Result<(), IntegrationCommandError> {
        self.call("stop", id).await
    }
    async fn health(
        &self,
        id: &IntegrationInstanceId,
    ) -> Result<IntegrationStatus, IntegrationCommandError> {
        self.call("health", id).await?;
        Ok(self.health.lock().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type ServiceFixture = (
        IntegrationService,
        Arc<FakeInstanceRepository>,
        Arc<FakeSecretStore>,
        Arc<FakeRuntimePort>,
        Arc<Mutex<Vec<String>>>,
    );

    fn request(name: &str) -> ConfigureIntegrationRequest {
        ConfigureIntegrationRequest {
            definition_id: IntegrationDefinitionId("gmail".into()),
            instance_id: None,
            display_name: name.into(),
            fields: vec![
                ConfiguredFieldValue {
                    key: "email".into(),
                    value: "user@example.test".into(),
                },
                ConfiguredFieldValue {
                    key: "app_password".into(),
                    value: "secret-never-log".into(),
                },
            ],
            management: None,
            enabled_capabilities: vec![CapabilityId("email.read".into())],
        }
    }
    fn service() -> ServiceFixture {
        let events = Arc::new(Mutex::new(Vec::new()));
        let instances = Arc::new(FakeInstanceRepository::with_events(events.clone()));
        let secrets = Arc::new(FakeSecretStore::with_events(events.clone()));
        let runtime = Arc::new(FakeRuntimePort::with_events(events.clone()));
        (
            IntegrationService::new(
                Arc::new(FakeCatalogRepository::new()),
                instances.clone(),
                secrets.clone(),
                runtime.clone(),
            ),
            instances,
            secrets,
            runtime,
            events,
        )
    }
    fn user() -> IntegrationActor {
        IntegrationActor { is_admin: false }
    }

    fn persistence_test_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("steersman-integration-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn persisted_instance() -> IntegrationInstance {
        IntegrationInstance {
            version: INTEGRATION_INSTANCE_SCHEMA_VERSION,
            id: IntegrationInstanceId(Uuid::new_v4()),
            definition_id: IntegrationDefinitionId("gmail".into()),
            display_name: "Persisted Gmail".into(),
            status: IntegrationStatus::Ready,
            enabled: true,
            management: IntegrationManagement::UserManaged,
            configured_capabilities: vec![CapabilityId("email.read".into())],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn list_available_and_unknown_definition() {
        let (service, ..) = service();
        assert_eq!(
            service.list_available_integrations().await.unwrap().len(),
            3
        );
        let error = service
            .configure_integration(
                &user(),
                ConfigureIntegrationRequest {
                    definition_id: IntegrationDefinitionId("missing".into()),
                    ..request("missing")
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error, IntegrationCommandError::DefinitionNotFound);
    }
    #[tokio::test]
    async fn successful_configure_orders_secret_persist_runtime_health() {
        let (service, instances, _, _, events) = service();
        let dto = service
            .configure_integration(&user(), request("one"))
            .await
            .unwrap();
        assert_eq!(dto.status, IntegrationStatus::Ready);
        assert_eq!(instances.list().await.unwrap().len(), 1);
        let events = events.lock().await;
        assert!(events[0].starts_with("secret.set:"));
        assert!(events[1].starts_with("instance.create:"));
        assert!(events[2].starts_with("runtime.configure:"));
        assert!(events[3].starts_with("runtime.health:"));
    }
    #[tokio::test]
    async fn invalid_setup_and_capability_do_not_write() {
        let (service, instances, _, _, events) = service();
        let mut bad = request("bad");
        bad.fields.clear();
        assert!(matches!(
            service.configure_integration(&user(), bad).await,
            Err(IntegrationCommandError::ConfigurationInvalid { .. })
        ));
        let mut cap = request("cap");
        cap.enabled_capabilities = vec![CapabilityId("no".into())];
        assert!(matches!(
            service.configure_integration(&user(), cap).await,
            Err(IntegrationCommandError::ConfigurationInvalid { .. })
        ));
        assert!(instances.list().await.unwrap().is_empty());
        assert!(events.lock().await.is_empty());
    }
    #[tokio::test]
    async fn runtime_failure_rolls_back_new_instance_and_keeps_original_error() {
        let (service, instances, secrets, runtime, events) = service();
        runtime
            .fail_at("configure", IntegrationCommandError::RuntimeUnavailable)
            .await;
        assert_eq!(
            service
                .configure_integration(&user(), request("one"))
                .await
                .unwrap_err(),
            IntegrationCommandError::RuntimeUnavailable
        );
        assert!(instances.list().await.unwrap().is_empty());
        assert!(!secrets
            .has_secret(&IntegrationInstanceId(Uuid::nil()), "app_password")
            .await
            .unwrap());
        let events = events.lock().await;
        assert!(events
            .iter()
            .any(|event| event.starts_with("runtime.stop:")));
        assert!(events
            .iter()
            .any(|event| event.starts_with("secret.delete:")));
    }
    #[tokio::test]
    async fn secrets_are_absent_from_dtos_and_debug() {
        let (service, _, _, _, _) = service();
        let dto = service
            .configure_integration(&user(), request("one"))
            .await
            .unwrap();
        assert!(dto
            .configured_fields
            .iter()
            .all(|field| field.value.is_none()));
        assert!(!format!("{:?}", request("one")).contains("secret-never-log"));
    }
    #[tokio::test]
    async fn enable_disable_are_idempotent_and_disable_keeps_credentials() {
        let (service, _, secrets, runtime, events) = service();
        let dto = service
            .configure_integration(&user(), request("one"))
            .await
            .unwrap();
        service.disable_integration(&user(), &dto.id).await.unwrap();
        service.disable_integration(&user(), &dto.id).await.unwrap();
        assert!(secrets.has_secret(&dto.id, "app_password").await.unwrap());
        service.enable_integration(&user(), &dto.id).await.unwrap();
        service.enable_integration(&user(), &dto.id).await.unwrap();
        let events = events.lock().await;
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("runtime.stop:"))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("runtime.start:"))
                .count(),
            1
        );
        assert!(!events
            .iter()
            .any(|event| event.starts_with("secret.delete:")));
        drop(runtime);
    }
    #[tokio::test]
    async fn remove_orders_stop_secrets_instance_and_preserves_on_failures() {
        let (service, instances, secrets, runtime, events) = service();
        let dto = service
            .configure_integration(&user(), request("one"))
            .await
            .unwrap();
        service.remove_integration(&user(), &dto.id).await.unwrap();
        let events = events.lock().await;
        let tail = &events[events.len() - 3..];
        assert!(tail[0].starts_with("runtime.stop:"));
        assert!(tail[1].starts_with("secret.delete:"));
        assert!(tail[2].starts_with("instance.delete:"));
        drop(events);
        let dto = service
            .configure_integration(&user(), request("two"))
            .await
            .unwrap();
        runtime
            .fail_at("stop", IntegrationCommandError::RuntimeUnavailable)
            .await;
        assert!(service.remove_integration(&user(), &dto.id).await.is_err());
        assert!(instances.get(&dto.id).await.is_ok());
        assert!(secrets.has_secret(&dto.id, "app_password").await.unwrap());
    }
    #[tokio::test]
    async fn administrator_managed_rejects_user_and_allows_admin() {
        let (service, _, _, _, _) = service();
        let mut request = request("admin");
        request.management = Some(IntegrationManagement::AdministratorManaged);
        assert_eq!(
            service
                .configure_integration(&user(), request.clone())
                .await
                .unwrap_err(),
            IntegrationCommandError::PermissionDenied
        );
        let dto = service
            .configure_integration(&IntegrationActor { is_admin: true }, request)
            .await
            .unwrap();
        assert_eq!(dto.management, IntegrationManagement::AdministratorManaged);
    }
    #[tokio::test]
    async fn health_failure_is_normalized_and_instances_are_isolated() {
        let (service, instances, _, runtime, _) = service();
        let one = service
            .configure_integration(&user(), request("one"))
            .await
            .unwrap();
        let two = service
            .configure_integration(&user(), request("two"))
            .await
            .unwrap();
        runtime
            .fail_at("health", IntegrationCommandError::HealthCheckFailed)
            .await;
        let status = service
            .refresh_integration_status(&user(), &one.id)
            .await
            .unwrap()
            .status;
        assert_eq!(
            status,
            IntegrationStatus::Degraded {
                reason: IntegrationIssue::HealthCheckFailed
            }
        );
        assert_eq!(instances.get(&two.id).await.unwrap().display_name, "two");
    }
    #[tokio::test]
    async fn disabled_remains_configured_failed_test_does_not_remove_and_removal_retry_is_safe() {
        let (service, instances, _, runtime, _) = service();
        let dto = service
            .configure_integration(&user(), request("one"))
            .await
            .unwrap();
        service.disable_integration(&user(), &dto.id).await.unwrap();
        assert_eq!(
            instances.get(&dto.id).await.unwrap().status,
            IntegrationStatus::Disabled
        );
        runtime
            .fail_at("health", IntegrationCommandError::HealthCheckFailed)
            .await;
        service.test_integration(&user(), &dto.id).await.unwrap();
        assert!(instances.get(&dto.id).await.is_ok());
        service.remove_integration(&user(), &dto.id).await.unwrap();
        assert_eq!(
            service
                .remove_integration(&user(), &dto.id)
                .await
                .unwrap_err(),
            IntegrationCommandError::InstanceNotFound
        );
    }

    #[tokio::test]
    async fn file_instance_repository_persists_the_full_lifecycle() {
        let home = persistence_test_dir();
        let repository = FileInstanceRepository::new(&home);
        let mut instance = persisted_instance();
        repository.create(instance.clone()).await.unwrap();
        assert_eq!(repository.get(&instance.id).await.unwrap(), instance);
        assert_eq!(repository.list().await.unwrap(), vec![instance.clone()]);
        instance.display_name = "Updated Gmail".into();
        repository.save(instance.clone()).await.unwrap();
        assert_eq!(repository.get(&instance.id).await.unwrap(), instance);
        repository.delete(&instance.id).await.unwrap();
        assert!(repository.list().await.unwrap().is_empty());
        fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn file_secret_store_sets_checks_and_deletes_secrets() {
        let home = persistence_test_dir();
        let store = FileSecretStore::new(&home);
        let id = IntegrationInstanceId(Uuid::new_v4());
        store.set_secret(&id, "token", "top-secret").await.unwrap();
        assert!(store.has_secret(&id, "token").await.unwrap());
        let stored = fs::read_to_string(
            home.join("integrations/secrets")
                .join(id.to_string())
                .join("token"),
        )
        .unwrap();
        assert_ne!(stored, "top-secret");
        store.delete_instance_secrets(&id).await.unwrap();
        assert!(!store.has_secret(&id, "token").await.unwrap());
        fs::remove_dir_all(home).unwrap();
    }

    #[derive(Default)]
    struct MockCredentialBackend {
        values: std::sync::Mutex<HashMap<String, String>>,
    }

    impl CredentialBackend for MockCredentialBackend {
        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(account.into(), value.into());
            Ok(())
        }

        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(account).cloned())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[tokio::test]
    async fn os_secret_store_round_trips_and_deletes_mocked_keyring_secrets() {
        let home = persistence_test_dir();
        let store = OsSecretStore::with_backend(&home, Arc::new(MockCredentialBackend::default()));
        let id = IntegrationInstanceId(Uuid::new_v4());
        store.set_secret(&id, "token", "top-secret").await.unwrap();
        assert!(store.has_secret(&id, "token").await.unwrap());
        store.delete_instance_secrets(&id).await.unwrap();
        assert!(!store.has_secret(&id, "token").await.unwrap());
        fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn os_secret_store_reports_missing_mocked_keyring_secret() {
        let home = persistence_test_dir();
        let store = OsSecretStore::with_backend(&home, Arc::new(MockCredentialBackend::default()));
        assert!(!store
            .has_secret(&IntegrationInstanceId(Uuid::new_v4()), "token")
            .await
            .unwrap());
        fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn os_secret_store_migrates_base64_files_to_mocked_keyring() {
        let home = persistence_test_dir();
        let legacy = FileSecretStore::new(&home);
        let id = IntegrationInstanceId(Uuid::new_v4());
        legacy.set_secret(&id, "token", "old-secret").await.unwrap();
        let legacy_path = home
            .join("integrations/secrets")
            .join(id.to_string())
            .join("token");
        let store = OsSecretStore::with_backend(&home, Arc::new(MockCredentialBackend::default()));
        assert!(store.has_secret(&id, "token").await.unwrap());
        assert!(
            !legacy_path.exists(),
            "legacy secret should be removed after migration"
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn startup_recovery_loads_and_reenables_enabled_instances() {
        let home = persistence_test_dir();
        let repository = Arc::new(FileInstanceRepository::new(&home));
        let instance = persisted_instance();
        repository.create(instance.clone()).await.unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let runtime = Arc::new(FakeRuntimePort::with_events(events.clone()));
        let service = IntegrationService::new(
            Arc::new(FakeCatalogRepository::new()),
            repository.clone(),
            Arc::new(FileSecretStore::new(&home)),
            runtime,
        );
        let report = service.recover_instances().await.unwrap();
        assert_eq!(
            report,
            IntegrationRecoveryReport {
                loaded: 1,
                re_enabled: 1,
                failed: 0
            }
        );
        assert_eq!(
            repository.get(&instance.id).await.unwrap().status,
            IntegrationStatus::Ready
        );
        assert!(events
            .lock()
            .await
            .iter()
            .any(|event| event.starts_with("runtime.start:")));
        fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn corrupted_instance_file_returns_a_typed_persistence_error() {
        let home = persistence_test_dir();
        let repository = FileInstanceRepository::new(&home);
        let dir = home.join("integrations/instances");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("corrupted.json"), b"not json").unwrap();
        assert!(matches!(
            repository.list().await,
            Err(IntegrationCommandError::Persistence { .. })
        ));
        fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn instance_schema_migrates_missing_version_and_rejects_unknown_versions() {
        let home = persistence_test_dir();
        let repository = FileInstanceRepository::new(&home);
        let instance = persisted_instance();
        let dir = home.join("integrations/instances");
        fs::create_dir_all(&dir).unwrap();
        let mut legacy = serde_json::to_value(&instance).unwrap();
        legacy.as_object_mut().unwrap().remove("version");
        fs::write(
            dir.join(format!("{}.json", instance.id)),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        assert_eq!(
            repository.get(&instance.id).await.unwrap().version,
            INTEGRATION_INSTANCE_SCHEMA_VERSION
        );
        legacy["version"] = serde_json::json!(99);
        fs::write(
            dir.join(format!("{}.json", instance.id)),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        assert_eq!(
            repository.get(&instance.id).await.unwrap_err(),
            IntegrationCommandError::UnsupportedSchemaVersion { version: 99 }
        );
        fs::remove_dir_all(home).unwrap();
    }
}
