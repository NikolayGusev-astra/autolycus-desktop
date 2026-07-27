//! Session Registry — maps product/UI conversation IDs to Hermes live/durable
//! session IDs.
//!
//! Replaces the single global `session_id: Mutex<Option<String>>` on `WsState`
//! with a typed, multi-session registry. The product layer uses
//! [`ConversationId`] exclusively; the transport layer resolves it to a live
//! session ID for `prompt.submit` and routes inbound events back to the right
//! conversation.
//!
//! Key invariants (Phase 1C.2):
//! - `prompt.submit` uses only the live session ID.
//! - resume happens by durable `stored_session_id`.
//! - `session.info.stored_session_id` never overwrites the live ID.
//! - after reconnect, a new live ID atomically replaces the old mapping.
//! - events are routed by the external `params.session_id` (live ID).
//! - unknown live-session events are logged but not assigned to a random
//!   conversation.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use tokio::sync::Mutex;

/// Product/UI conversation identity. Opaque to the transport layer — the
/// frontend generates it and the registry maps it to Hermes session IDs.
/// A newtype (not a bare String) so live and durable IDs cannot be confused
/// with conversation IDs at the type level.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationId(pub String);

/// Hermes profile identifier (multi-profile support). Sessions are scoped to a
/// profile; durable identity is the (profile, stored_session_id) pair, not the
/// stored ID alone — the same stored ID can exist under different profiles.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ProfileId(pub String);

/// Runtime identity: Local Hermes, Remote Hermes, or SSH-tunneled Hermes.
/// Bindings are scoped to a specific runtime so Local/Remote/SSH sessions
/// don't mix in the registry.
pub type RemoteInstanceId = String;
pub type SshTunnelId = String;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum RuntimeKey {
    Local,
    Remote(RemoteInstanceId),
    Ssh(SshTunnelId),
}

impl fmt::Debug for RuntimeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => f.write_str("Local"),
            Self::Remote(_) => f.write_str("Remote(<redacted>)"),
            Self::Ssh(_) => f.write_str("Ssh(<redacted>)"),
        }
    }
}

impl ProfileId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn empty() -> Self {
        Self(String::new())
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Durable (cross-reconnect) reference to a Hermes session. The full identity
/// for resume is the pair (profile, stored_session_id): two profiles can have
/// the same stored session ID. Resume and persistence must key on both.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DurableSessionRef {
    pub profile: ProfileId,
    pub stored_session_id: String,
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl ConversationId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Lifecycle of a conversation binding relative to the current connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Live: has a valid live session ID on the current connection.
    Active,
    /// Disconnected or waiting for reconnect/resume. Durable ID retained.
    Suspended,
    /// Reconnect in progress: a session.resume call is outstanding for this
    /// binding's stored_session_id. Carries the connection generation of the
    /// attempt so reader cleanup can identify which Resuming bindings belong
    /// to a dead connection.
    Resuming { attempt_generation: u64 },
    /// The last session.resume attempt failed. The durable ID is retained so
    /// the user can retry manually; the binding is NOT usable for prompts.
    ResumeFailed,
}

/// One conversation's binding to Hermes session IDs.
///
/// `live_session_id` is the ephemeral ID valid for the current connection
/// generation; it changes on reconnect. `stored_session_id` is the durable ID
/// used to resume across reconnects.
#[derive(Debug, Clone)]
pub struct SessionBinding {
    pub conversation_id: ConversationId,
    pub live_session_id: Option<String>,
    pub stored_session_id: Option<String>,
    pub profile: ProfileId,
    pub state: SessionState,
    /// The connection generation this binding's live ID belongs to. A
    /// generation mismatch means the live ID is stale and must be resumed
    /// before use (Phase 1C.3 reconnect reconciliation).
    pub connection_generation: u64,
    /// The runtime this binding belongs to (Local/Remote/SSH).
    pub runtime_key: RuntimeKey,
}

/// Registry mapping conversations ↔ Hermes session IDs, indexed both ways for
/// O(1) event routing (by live session ID) and prompt resolution (by
/// conversation ID).
///
/// All access goes through a single internal `Mutex` — the registry is
/// contention-light (one lock per chat turn or event, not per token).
pub struct SessionRegistry {
    inner: Mutex<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    by_conversation: HashMap<(RuntimeKey, ConversationId), SessionBinding>,
    /// Reverse index: live session ID → conversation ID, for event routing.
    by_live_session: HashMap<(RuntimeKey, String), ConversationId>,
}

impl SessionRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(RegistryInner::default()),
        })
    }

    /// Register or update a conversation's binding. Atomically maintains both
    /// indexes: removes any old live-session reverse mapping, inserts the new
    /// one. Used after `session.create`/`session.resume` returns a fresh live ID.
    pub async fn set_live(
        &self,
        conversation_id: ConversationId,
        live_session_id: String,
        stored_session_id: Option<String>,
        profile: ProfileId,
        connection_generation: u64,
        runtime_key: RuntimeKey,
    ) {
        let mut inner = self.inner.lock().await;
        // Remove the old reverse mapping if the conversation already had a
        // different live ID (reconnect replaces it atomically). Clone the old
        // live ID out of the immutable borrow before mutating the reverse map.
        let old_live_to_remove = inner
            .by_conversation
            .get(&(runtime_key.clone(), conversation_id.clone()))
            .and_then(|old| old.live_session_id.clone())
            .filter(|old_live| old_live != &live_session_id);
        if let Some(old_live) = old_live_to_remove {
            inner
                .by_live_session
                .remove(&(runtime_key.clone(), old_live));
        }
        inner.by_live_session.insert(
            (runtime_key.clone(), live_session_id.clone()),
            conversation_id.clone(),
        );
        inner.by_conversation.insert(
            (runtime_key.clone(), conversation_id.clone()),
            SessionBinding {
                conversation_id,
                live_session_id: Some(live_session_id),
                stored_session_id,
                profile,
                state: SessionState::Active,
                connection_generation,
                runtime_key,
            },
        );
    }

    /// Record a durable stored_session_id for an existing conversation without
    /// touching the live ID. `session.info.stored_session_id` must never
    /// overwrite the live ID (it may lag behind after a reconnect).
    pub async fn set_stored(
        &self,
        conversation_id: &ConversationId,
        runtime_key: RuntimeKey,
        stored_session_id: String,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner
            .by_conversation
            .get_mut(&(runtime_key, conversation_id.clone()))
        {
            b.stored_session_id = Some(stored_session_id);
            true
        } else {
            false
        }
    }

    /// Look up the live session ID for a conversation (for `prompt.submit`).
    /// Returns None if the conversation is unknown or has no live ID (must
    /// create/resume first).
    pub async fn get_live(
        &self,
        conversation_id: &ConversationId,
        runtime_key: RuntimeKey,
    ) -> Option<String> {
        let inner = self.inner.lock().await;
        inner
            .by_conversation
            .get(&(runtime_key, conversation_id.clone()))
            .and_then(|b| b.live_session_id.clone())
    }

    /// Get the full binding for a conversation.
    pub async fn get(
        &self,
        conversation_id: &ConversationId,
        runtime_key: RuntimeKey,
    ) -> Option<SessionBinding> {
        let inner = self.inner.lock().await;
        inner
            .by_conversation
            .get(&(runtime_key, conversation_id.clone()))
            .cloned()
    }

    /// Route an inbound event: given the external live session ID from
    /// `params.session_id`, find the owning conversation. Returns None for
    /// unknown sessions (logged by caller, not assigned to a random convo).
    pub async fn route_event(
        &self,
        live_session_id: &str,
        runtime_key: RuntimeKey,
    ) -> Option<ConversationId> {
        let inner = self.inner.lock().await;
        inner
            .by_live_session
            .get(&(runtime_key, live_session_id.to_owned()))
            .cloned()
    }

    /// Transition all bindings to Suspended (on disconnect). Durable IDs are
    /// retained for resume; only the live IDs are considered stale.
    pub async fn suspend_all(&self, runtime_key: RuntimeKey) {
        let mut inner = self.inner.lock().await;
        for ((binding_runtime_key, _), b) in inner.by_conversation.iter_mut() {
            if binding_runtime_key == &runtime_key {
                b.state = SessionState::Suspended;
            }
        }
        // Live IDs for this runtime are now stale; clear the reverse index
        // so stale events don't route to a suspended conversation.
        let to_remove: Vec<(RuntimeKey, String)> = inner
            .by_live_session
            .iter()
            .filter(|((binding_runtime_key, _), _)| binding_runtime_key == &runtime_key)
            .map(|(live, _)| live.clone())
            .collect();
        for live in to_remove {
            inner.by_live_session.remove(&live);
        }
    }

    /// Mark all bindings as Suspended if their generation predates the current
    /// connection generation. Used after reconnect before reconciliation.
    /// Clears their live IDs (stale) so events from a dead connection don't
    /// route, while preserving durable stored IDs for resume.
    pub async fn mark_stale_for_generation(
        &self,
        current_generation: u64,
        runtime_key: RuntimeKey,
    ) {
        let mut inner = self.inner.lock().await;
        // Collect the stale live IDs first to avoid borrowing inner twice.
        let mut stale_lives: Vec<(RuntimeKey, String)> = Vec::new();
        for ((binding_runtime_key, _), b) in inner.by_conversation.iter_mut() {
            if binding_runtime_key == &runtime_key && b.connection_generation < current_generation {
                b.state = SessionState::Suspended;
                if let Some(live) = b.live_session_id.take() {
                    stale_lives.push((runtime_key.clone(), live));
                }
            }
        }
        for live in stale_lives {
            inner.by_live_session.remove(&live);
        }
    }

    /// Suspend exactly the bindings belonging to `dead_generation`.
    /// Matches: Active with connection_generation == dead_generation, AND
    /// Resuming with attempt_generation == dead_generation (a resume RPC was
    /// in flight when the socket died). Both return to Suspended so the next
    /// reconnect retries them. Without the Resuming match, a disconnect during
    /// reconciliation would permanently strand the binding.
    pub async fn suspend_generation(&self, dead_generation: u64, runtime_key: RuntimeKey) {
        let mut inner = self.inner.lock().await;
        let mut stale_lives: Vec<(RuntimeKey, String)> = Vec::new();
        for ((binding_runtime_key, _), b) in inner.by_conversation.iter_mut() {
            if binding_runtime_key != &runtime_key {
                continue;
            }
            let should_suspend = match &b.state {
                SessionState::Active => b.connection_generation == dead_generation,
                SessionState::Resuming { attempt_generation } => {
                    *attempt_generation == dead_generation
                }
                _ => false,
            };
            if should_suspend {
                b.state = SessionState::Suspended;
                if let Some(live) = b.live_session_id.take() {
                    stale_lives.push((runtime_key.clone(), live));
                }
            }
        }
        for live in stale_lives {
            inner.by_live_session.remove(&live);
        }
    }

    /// Return all bindings that are Suspended AND have a durable stored_session_id,
    /// so the reconciliation loop can resume them. The returned bindings are
    /// transitioned to Resuming { attempt_generation } atomically (single lock),
    /// preventing duplicate resume attempts if two tasks race. The attempt_generation
    /// lets reader cleanup identify which Resuming bindings belong to a dead
    /// connection. Returns the full DurableSessionRef (profile + stored_session_id).
    pub async fn take_suspended_for_resume(
        &self,
        attempt_generation: u64,
        runtime_key: RuntimeKey,
    ) -> Vec<(ConversationId, DurableSessionRef)> {
        let mut inner = self.inner.lock().await;
        let mut out = Vec::new();
        for ((binding_runtime_key, _), b) in inner.by_conversation.iter_mut() {
            if binding_runtime_key == &runtime_key && b.state == SessionState::Suspended {
                if let Some(stored) = b.stored_session_id.clone() {
                    if !stored.is_empty() {
                        b.state = SessionState::Resuming { attempt_generation };
                        out.push((
                            b.conversation_id.clone(),
                            DurableSessionRef {
                                profile: b.profile.clone(),
                                stored_session_id: stored,
                            },
                        ));
                    }
                }
            }
        }
        out
    }

    /// Mark a conversation's resume as failed (keeps durable ID for manual retry).
    pub async fn mark_resume_failed(
        &self,
        conversation_id: &ConversationId,
        runtime_key: RuntimeKey,
    ) {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner
            .by_conversation
            .get_mut(&(runtime_key, conversation_id.clone()))
        {
            b.state = SessionState::ResumeFailed;
        }
    }

    /// Return a binding from Resuming back to Suspended. Used when a resume RPC
    /// was interrupted by a network error (ConnectionLost/RpcTimeout) rather
    /// than a genuine backend rejection — the session may still be resumable on
    /// the next reconnect, so it must NOT be permanently marked ResumeFailed.
    pub async fn return_to_suspended(
        &self,
        conversation_id: &ConversationId,
        runtime_key: RuntimeKey,
    ) {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner
            .by_conversation
            .get_mut(&(runtime_key, conversation_id.clone()))
        {
            if matches!(b.state, SessionState::Resuming { .. }) {
                b.state = SessionState::Suspended;
            }
        }
    }

    /// Number of registered conversations (diagnostics/testing).
    pub async fn len(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.by_conversation.len()
    }

    /// Remove a conversation entirely (e.g. user closes it).
    pub async fn remove(
        &self,
        conversation_id: &ConversationId,
        runtime_key: RuntimeKey,
    ) -> Option<SessionBinding> {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner
            .by_conversation
            .remove(&(runtime_key.clone(), conversation_id.clone()))
        {
            if let Some(live) = &b.live_session_id {
                inner
                    .by_live_session
                    .remove(&(runtime_key.clone(), live.clone()));
            }
            Some(b)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Arc<SessionRegistry> {
        SessionRegistry::new()
    }

    #[test]
    fn runtime_key_debug_redacts_instance_ids() {
        let remote = format!("{:?}", RuntimeKey::Remote("remote-secret".into()));
        let ssh = format!("{:?}", RuntimeKey::Ssh("ssh-secret".into()));

        assert_eq!(remote, "Remote(<redacted>)");
        assert_eq!(ssh, "Ssh(<redacted>)");
    }

    /// Test 6: Live and durable IDs do not mix. set_stored does not overwrite
    /// the live ID; get_live returns the live ID, not the stored one.
    #[tokio::test]
    async fn live_and_stored_ids_do_not_mix() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        r.set_live(
            conv.clone(),
            "live-aaa".into(),
            None,
            ProfileId::empty(),
            1,
            RuntimeKey::Local,
        )
        .await;
        // Backend reports a stored_session_id via session.info — must NOT
        // replace the live ID used for prompt.submit.
        let updated = r
            .set_stored(&conv, RuntimeKey::Local, "stored-bbb".into())
            .await;
        assert!(
            updated,
            "set_stored should succeed for existing conversation"
        );
        let live = r.get_live(&conv, RuntimeKey::Local).await.unwrap();
        assert_eq!(
            live, "live-aaa",
            "live ID must not be overwritten by stored ID"
        );
        let binding = r.get(&conv, RuntimeKey::Local).await.unwrap();
        assert_eq!(binding.stored_session_id.as_deref(), Some("stored-bbb"));
        assert_eq!(binding.live_session_id.as_deref(), Some("live-aaa"));
    }

    /// Test 7: Two parallel conversations route events independently by live
    /// session ID.
    #[tokio::test]
    async fn parallel_conversations_route_independently() {
        let r = reg();
        let conv_a = ConversationId::new("conv-a");
        let conv_b = ConversationId::new("conv-b");
        r.set_live(
            conv_a.clone(),
            "live-a".into(),
            None,
            ProfileId::empty(),
            1,
            RuntimeKey::Local,
        )
        .await;
        r.set_live(
            conv_b.clone(),
            "live-b".into(),
            None,
            ProfileId::empty(),
            1,
            RuntimeKey::Local,
        )
        .await;

        assert_eq!(
            r.route_event("live-a", RuntimeKey::Local).await,
            Some(conv_a)
        );
        assert_eq!(
            r.route_event("live-b", RuntimeKey::Local).await,
            Some(conv_b)
        );
        assert_eq!(r.route_event("live-unknown", RuntimeKey::Local).await, None);
    }

    /// Test 8: After a reconnect (new live ID), set_live atomically replaces
    /// the old mapping and updates the reverse index. The old live ID no
    /// longer routes.
    #[tokio::test]
    async fn reconnect_replaces_only_live_id() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        r.set_live(
            conv.clone(),
            "live-old".into(),
            Some("stored-1".into()),
            ProfileId::empty(),
            1,
            RuntimeKey::Local,
        )
        .await;
        // Reconnect: new live ID, same stored/durable ID.
        r.set_live(
            conv.clone(),
            "live-new".into(),
            Some("stored-1".into()),
            ProfileId::empty(),
            2,
            RuntimeKey::Local,
        )
        .await;

        // Old live ID no longer routes.
        assert_eq!(r.route_event("live-old", RuntimeKey::Local).await, None);
        // New live ID routes correctly.
        assert_eq!(
            r.route_event("live-new", RuntimeKey::Local).await,
            Some(conv.clone())
        );
        // Durable stored ID is preserved.
        let binding = r.get(&conv, RuntimeKey::Local).await.unwrap();
        assert_eq!(binding.stored_session_id.as_deref(), Some("stored-1"));
        assert_eq!(binding.connection_generation, 2);
    }

    /// Test 9: A stale generation marks bindings Suspended and clears their
    /// live IDs, so events from a dead connection don't route.
    #[tokio::test]
    async fn stale_generation_clears_live_routing() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        r.set_live(
            conv.clone(),
            "live-1".into(),
            Some("stored-1".into()),
            ProfileId::empty(),
            1,
            RuntimeKey::Local,
        )
        .await;
        // Simulate reconnect to generation 2, before reconciliation.
        r.mark_stale_for_generation(2, RuntimeKey::Local).await;

        let binding = r.get(&conv, RuntimeKey::Local).await.unwrap();
        assert_eq!(binding.state, SessionState::Suspended);
        assert_eq!(
            binding.live_session_id, None,
            "live ID must be cleared for stale gen"
        );
        assert_eq!(
            binding.stored_session_id.as_deref(),
            Some("stored-1"),
            "durable ID must be retained"
        );
        // Stale live ID no longer routes.
        assert_eq!(r.route_event("live-1", RuntimeKey::Local).await, None);
    }

    /// Test 10: An unknown live session event is not assigned to any
    /// conversation — route_event returns None.
    #[tokio::test]
    async fn unknown_live_event_not_assigned() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        r.set_live(
            conv,
            "live-known".into(),
            None,
            ProfileId::empty(),
            1,
            RuntimeKey::Local,
        )
        .await;
        // An event for a session nobody owns.
        assert_eq!(r.route_event("live-orphan", RuntimeKey::Local).await, None);
    }

    #[tokio::test]
    async fn runtimes_isolate_identical_conversation_and_live_ids() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        let remote = RuntimeKey::Remote("test-instance".into());
        for runtime_key in [RuntimeKey::Local, remote.clone()] {
            r.set_live(
                conv.clone(),
                "live-1".into(),
                Some(format!("stored-{runtime_key:?}")),
                ProfileId::empty(),
                1,
                runtime_key,
            )
            .await;
        }

        assert_eq!(r.len().await, 2);
        assert_eq!(
            r.route_event("live-1", RuntimeKey::Local).await,
            Some(conv.clone())
        );
        assert_eq!(
            r.route_event("live-1", remote.clone()).await,
            Some(conv.clone())
        );

        r.suspend_generation(1, RuntimeKey::Local).await;
        assert_eq!(r.route_event("live-1", RuntimeKey::Local).await, None);
        assert_eq!(
            r.route_event("live-1", remote.clone()).await,
            Some(conv.clone())
        );
        assert_eq!(
            r.take_suspended_for_resume(2, RuntimeKey::Local)
                .await
                .len(),
            1
        );
        assert!(r.take_suspended_for_resume(2, remote).await.is_empty());
    }
}
