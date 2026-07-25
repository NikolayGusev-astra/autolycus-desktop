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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Live: has a valid live session ID on the current connection.
    Active,
    /// Disconnected or waiting for reconnect/resume. Durable ID retained.
    Suspended,
    /// Reconnect in progress: a session.resume call is outstanding for this
    /// binding's stored_session_id.
    Resuming,
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
    /// Profile scope for this binding's durable ID. Resume must key on
    /// (profile, stored_session_id), not stored_session_id alone.
    pub profile: ProfileId,
    pub state: SessionState,
    /// The connection generation this binding's live ID belongs to. A
    /// generation mismatch means the live ID is stale and must be resumed
    /// before use (Phase 1C.3 reconnect reconciliation).
    pub connection_generation: u64,
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
    by_conversation: HashMap<ConversationId, SessionBinding>,
    /// Reverse index: live session ID → conversation ID, for event routing.
    by_live_session: HashMap<String, ConversationId>,
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
    ) {
        let mut inner = self.inner.lock().await;
        // Remove the old reverse mapping if the conversation already had a
        // different live ID (reconnect replaces it atomically). Clone the old
        // live ID out of the immutable borrow before mutating the reverse map.
        let old_live_to_remove = inner
            .by_conversation
            .get(&conversation_id)
            .and_then(|old| old.live_session_id.clone())
            .filter(|old_live| old_live != &live_session_id);
        if let Some(old_live) = old_live_to_remove {
            inner.by_live_session.remove(&old_live);
        }
        inner
            .by_live_session
            .insert(live_session_id.clone(), conversation_id.clone());
        inner.by_conversation.insert(
            conversation_id.clone(),
            SessionBinding {
                conversation_id,
                live_session_id: Some(live_session_id),
                stored_session_id,
                profile,
                state: SessionState::Active,
                connection_generation,
            },
        );
    }

    /// Record a durable stored_session_id for an existing conversation without
    /// touching the live ID. `session.info.stored_session_id` must never
    /// overwrite the live ID (it may lag behind after a reconnect).
    pub async fn set_stored(
        &self,
        conversation_id: &ConversationId,
        stored_session_id: String,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner.by_conversation.get_mut(conversation_id) {
            b.stored_session_id = Some(stored_session_id);
            true
        } else {
            false
        }
    }

    /// Look up the live session ID for a conversation (for `prompt.submit`).
    /// Returns None if the conversation is unknown or has no live ID (must
    /// create/resume first).
    pub async fn get_live(&self, conversation_id: &ConversationId) -> Option<String> {
        let inner = self.inner.lock().await;
        inner
            .by_conversation
            .get(conversation_id)
            .and_then(|b| b.live_session_id.clone())
    }

    /// Get the full binding for a conversation.
    pub async fn get(&self, conversation_id: &ConversationId) -> Option<SessionBinding> {
        let inner = self.inner.lock().await;
        inner.by_conversation.get(conversation_id).cloned()
    }

    /// Route an inbound event: given the external live session ID from
    /// `params.session_id`, find the owning conversation. Returns None for
    /// unknown sessions (logged by caller, not assigned to a random convo).
    pub async fn route_event(&self, live_session_id: &str) -> Option<ConversationId> {
        let inner = self.inner.lock().await;
        inner.by_live_session.get(live_session_id).cloned()
    }

    /// Transition all bindings to Suspended (on disconnect). Durable IDs are
    /// retained for resume; only the live IDs are considered stale.
    pub async fn suspend_all(&self) {
        let mut inner = self.inner.lock().await;
        for b in inner.by_conversation.values_mut() {
            b.state = SessionState::Suspended;
        }
        // Live IDs are now stale; clear the reverse index so stale events
        // don't route to a suspended conversation.
        inner.by_live_session.clear();
    }

    /// Mark all bindings as Suspended if their generation predates the current
    /// connection generation. Used after reconnect before reconciliation.
    /// Clears their live IDs (stale) so events from a dead connection don't
    /// route, while preserving durable stored IDs for resume.
    pub async fn mark_stale_for_generation(&self, current_generation: u64) {
        let mut inner = self.inner.lock().await;
        // Collect the stale live IDs first to avoid borrowing inner twice.
        let mut stale_lives: Vec<String> = Vec::new();
        for b in inner.by_conversation.values_mut() {
            if b.connection_generation < current_generation {
                b.state = SessionState::Suspended;
                if let Some(live) = b.live_session_id.take() {
                    stale_lives.push(live);
                }
            }
        }
        for live in stale_lives {
            inner.by_live_session.remove(&live);
        }
    }

    /// Suspend exactly the bindings belonging to `dead_generation` (== match).
    /// This is what the reader task calls on confirmed disconnect: the dead
    /// socket's bindings are Suspended, their live IDs cleared (stale), and
    /// durable IDs retained for resume. Bindings from OTHER generations
    /// (already resumed on a newer connection) are left untouched.
    pub async fn suspend_generation(&self, dead_generation: u64) {
        let mut inner = self.inner.lock().await;
        let mut stale_lives: Vec<String> = Vec::new();
        for b in inner.by_conversation.values_mut() {
            if b.connection_generation == dead_generation && b.state == SessionState::Active {
                b.state = SessionState::Suspended;
                if let Some(live) = b.live_session_id.take() {
                    stale_lives.push(live);
                }
            }
        }
        for live in stale_lives {
            inner.by_live_session.remove(&live);
        }
    }

    /// Return all bindings that are Suspended AND have a durable stored_session_id,
    /// so the reconciliation loop can resume them. The returned bindings are
    /// transitioned to Resuming atomically (single lock), preventing duplicate
    /// resume attempts if two tasks race. Returns the full DurableSessionRef
    /// (profile + stored_session_id) so resume can key on the pair.
    pub async fn take_suspended_for_resume(&self) -> Vec<(ConversationId, DurableSessionRef)> {
        let mut inner = self.inner.lock().await;
        let mut out = Vec::new();
        for b in inner.by_conversation.values_mut() {
            if b.state == SessionState::Suspended {
                if let Some(stored) = b.stored_session_id.clone() {
                    if !stored.is_empty() {
                        b.state = SessionState::Resuming;
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
    pub async fn mark_resume_failed(&self, conversation_id: &ConversationId) {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner.by_conversation.get_mut(conversation_id) {
            b.state = SessionState::ResumeFailed;
        }
    }

    /// Number of registered conversations (diagnostics/testing).
    pub async fn len(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.by_conversation.len()
    }

    /// Remove a conversation entirely (e.g. user closes it).
    pub async fn remove(&self, conversation_id: &ConversationId) -> Option<SessionBinding> {
        let mut inner = self.inner.lock().await;
        if let Some(b) = inner.by_conversation.remove(conversation_id) {
            if let Some(live) = &b.live_session_id {
                inner.by_live_session.remove(live);
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

    /// Test 6: Live and durable IDs do not mix. set_stored does not overwrite
    /// the live ID; get_live returns the live ID, not the stored one.
    #[tokio::test]
    async fn live_and_stored_ids_do_not_mix() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        r.set_live(conv.clone(), "live-aaa".into(), None, ProfileId::empty(), 1)
            .await;
        // Backend reports a stored_session_id via session.info — must NOT
        // replace the live ID used for prompt.submit.
        let updated = r.set_stored(&conv, "stored-bbb".into()).await;
        assert!(
            updated,
            "set_stored should succeed for existing conversation"
        );
        let live = r.get_live(&conv).await.unwrap();
        assert_eq!(
            live, "live-aaa",
            "live ID must not be overwritten by stored ID"
        );
        let binding = r.get(&conv).await.unwrap();
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
        r.set_live(conv_a.clone(), "live-a".into(), None, ProfileId::empty(), 1)
            .await;
        r.set_live(conv_b.clone(), "live-b".into(), None, ProfileId::empty(), 1)
            .await;

        assert_eq!(r.route_event("live-a").await, Some(conv_a));
        assert_eq!(r.route_event("live-b").await, Some(conv_b));
        assert_eq!(r.route_event("live-unknown").await, None);
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
        )
        .await;
        // Reconnect: new live ID, same stored/durable ID.
        r.set_live(
            conv.clone(),
            "live-new".into(),
            Some("stored-1".into()),
            ProfileId::empty(),
            2,
        )
        .await;

        // Old live ID no longer routes.
        assert_eq!(r.route_event("live-old").await, None);
        // New live ID routes correctly.
        assert_eq!(r.route_event("live-new").await, Some(conv.clone()));
        // Durable stored ID is preserved.
        let binding = r.get(&conv).await.unwrap();
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
        )
        .await;
        // Simulate reconnect to generation 2, before reconciliation.
        r.mark_stale_for_generation(2).await;

        let binding = r.get(&conv).await.unwrap();
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
        assert_eq!(r.route_event("live-1").await, None);
    }

    /// Test 10: An unknown live session event is not assigned to any
    /// conversation — route_event returns None.
    #[tokio::test]
    async fn unknown_live_event_not_assigned() {
        let r = reg();
        let conv = ConversationId::new("conv-1");
        r.set_live(conv, "live-known".into(), None, ProfileId::empty(), 1)
            .await;
        // An event for a session nobody owns.
        assert_eq!(r.route_event("live-orphan").await, None);
    }
}
