use crate::resolver::{ResolveError, ResolvedBotBinding, resolve_room_bot, resolve_room_bots};
use crate::runtime::{BotRequest, BotRuntimeError, RuntimeAdapter};
use crate::state::{
    BotfatherState, InventorySnapshot, SessionKey, SessionRecord, SessionScopeKind, UserSnapshot,
};
use crate::store::{StateStore, StateStoreError};

#[derive(Debug, thiserror::Error)]
pub enum BotfatherManagerError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Store(#[from] StateStoreError),
    #[error(transparent)]
    Runtime(#[from] BotRuntimeError),
}

pub struct BotfatherManager {
    store: StateStore,
    state: BotfatherState,
}

impl BotfatherManager {
    pub fn from_parts(store: StateStore, state: BotfatherState) -> Self {
        Self { store, state }
    }

    pub fn state(&self) -> &BotfatherState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut BotfatherState {
        &mut self.state
    }

    pub fn store(&self) -> &StateStore {
        &self.store
    }

    pub fn refresh_inventory(&mut self, user: UserSnapshot, inventory: InventorySnapshot) {
        self.state.refresh_inventory(user, inventory);
    }

    pub fn save(&self) -> Result<(), StateStoreError> {
        self.store.save(&self.state)
    }

    pub fn resolve_room_bots(
        &self,
        room_id: &str,
    ) -> Result<Vec<ResolvedBotBinding>, ResolveError> {
        resolve_room_bots(&self.state, room_id)
    }

    pub fn resolve_room_bot(
        &self,
        room_id: &str,
        preferred_bot_id: Option<&str>,
    ) -> Result<ResolvedBotBinding, ResolveError> {
        resolve_room_bot(&self.state, room_id, preferred_bot_id)
    }

    pub fn runtime_for_resolved(
        &self,
        resolved: &ResolvedBotBinding,
    ) -> Result<RuntimeAdapter, BotRuntimeError> {
        RuntimeAdapter::from_profile(&resolved.runtime_profile)
    }

    pub fn prepare_dispatch(
        &mut self,
        room_id: &str,
        thread_root_event_id: Option<&str>,
        reply_root_event_id: Option<&str>,
        message: impl Into<String>,
        preferred_bot_id: Option<&str>,
    ) -> Result<(ResolvedBotBinding, RuntimeAdapter, BotRequest), BotfatherManagerError> {
        let resolved = self.resolve_room_bot(room_id, preferred_bot_id)?;
        let runtime = self.runtime_for_resolved(&resolved)?;
        let session_id =
            ensure_session_id(
                &mut self.state,
                room_id,
                thread_root_event_id,
                reply_root_event_id,
                &resolved,
            );
        let request = BotRequest {
            room_id: room_id.to_string(),
            thread_root_event_id: thread_root_event_id.map(ToOwned::to_owned),
            reply_root_event_id: reply_root_event_id.map(ToOwned::to_owned),
            bot_id: resolved.bot.id.clone(),
            session_id,
            message: message.into(),
            delivery_target: resolved.delivery,
            runtime_override: resolved.runtime_override.clone(),
        };

        Ok((resolved, runtime, request))
    }
}

fn ensure_session_id(
    state: &mut BotfatherState,
    room_id: &str,
    thread_root_event_id: Option<&str>,
    reply_root_event_id: Option<&str>,
    resolved: &ResolvedBotBinding,
) -> String {
    let (scope_kind, thread_root_event_id, reply_root_event_id) = if let Some(thread_root_event_id) =
        thread_root_event_id
    {
        (
            SessionScopeKind::Thread,
            Some(thread_root_event_id.to_string()),
            None,
        )
    } else if let Some(reply_root_event_id) = reply_root_event_id {
        (
            SessionScopeKind::ReplyRoot,
            None,
            Some(reply_root_event_id.to_string()),
        )
    } else {
        (SessionScopeKind::Room, None, None)
    };
    let key = SessionKey {
        room_id: room_id.to_string(),
        scope_kind,
        thread_root_event_id,
        reply_root_event_id,
        bot_id: resolved.bot.id.clone(),
    };
    let record = state
        .runtime
        .active_sessions
        .entry(key.clone())
        .or_insert_with(|| SessionRecord {
            key: key.clone(),
            runtime_profile_id: resolved.runtime_profile.id.clone(),
            session_id: make_session_id(&key),
        });

    if record.runtime_profile_id != resolved.runtime_profile.id {
        record.runtime_profile_id = resolved.runtime_profile.id.clone();
        record.session_id = make_session_id(&key);
    }

    record.session_id.clone()
}

fn make_session_id(key: &SessionKey) -> String {
    let scope = match key.scope_kind {
        SessionScopeKind::Room => "room:main".to_string(),
        SessionScopeKind::Thread => format!(
            "thread:{}",
            key.thread_root_event_id
                .as_deref()
                .unwrap_or("unknown-thread")
        ),
        SessionScopeKind::ReplyRoot => format!(
            "reply:{}",
            key.reply_root_event_id
                .as_deref()
                .unwrap_or("unknown-reply")
        ),
    };
    format!("robrix:{}:{}:{}", key.room_id, scope, key.bot_id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::BotfatherManager;
    use crate::state::{
        BotDefinition, BotfatherDefaults, BotfatherState, DeliveryTarget, InventorySnapshot,
        OpenClawRuntimeConfig, PermissionPolicy, RoomInventory, RuntimeConfig, RuntimeProfile,
        TriggerPolicy,
    };
    use crate::store::StateStore;

    fn build_manager() -> BotfatherManager {
        let mut state = BotfatherState {
            inventory: InventorySnapshot {
                rooms: BTreeMap::from([(
                    "!room:example.org".into(),
                    RoomInventory {
                        room_id: "!room:example.org".into(),
                        display_name: None,
                        canonical_alias: None,
                        space_ids: Vec::new(),
                        is_direct: false,
                        stale: false,
                    },
                )]),
                spaces: BTreeMap::new(),
            },
            defaults: BotfatherDefaults {
                bot_ids: vec![default_bot_id().into()],
                ..Default::default()
            },
            ..Default::default()
        };
        state.runtime_profiles.insert(
            default_runtime_id().into(),
            RuntimeProfile {
                id: default_runtime_id().into(),
                name: default_runtime_name().into(),
                workspace_id: None,
                description: None,
                dispatch_policy: Default::default(),
                config: default_runtime_config(),
            },
        );
        state.bots.insert(
            default_bot_id().into(),
            BotDefinition {
                id: default_bot_id().into(),
                name: default_runtime_name().into(),
                runtime_profile_id: default_runtime_id().into(),
                priority: 0,
                enabled: true,
                trigger: TriggerPolicy::default(),
                default_delivery: DeliveryTarget::CurrentRoom,
                permissions: PermissionPolicy::default(),
                runtime_override: Default::default(),
                dispatch_policy_override: None,
                description: None,
            },
        );

        BotfatherManager::from_parts(StateStore::new("/tmp/unused-botfather.json"), state)
    }

    fn default_runtime_id() -> &'static str {
        if cfg!(feature = "crew") {
            "crew-runtime"
        } else {
            "openclaw-runtime"
        }
    }

    fn default_bot_id() -> &'static str {
        if cfg!(feature = "crew") {
            "crew-main"
        } else {
            "openclaw-main"
        }
    }

    fn default_runtime_name() -> &'static str {
        if cfg!(feature = "crew") {
            "Crew"
        } else {
            "OpenClaw"
        }
    }

    fn default_runtime_config() -> RuntimeConfig {
        if cfg!(feature = "crew") {
            RuntimeConfig::Crew {
                base_url: "http://127.0.0.1:8000".into(),
                api_key_env: None,
                model: None,
                system_prompt: None,
            }
        } else {
            RuntimeConfig::OpenClaw(OpenClawRuntimeConfig {
                gateway_url: "ws://127.0.0.1:24282/ws".into(),
                auth_token_env: None,
                agent_id: "main".into(),
            })
        }
    }

    #[test]
    fn sessions_are_reused_for_same_room_scope() {
        let mut manager = build_manager();
        let (_, _, first) = manager
            .prepare_dispatch("!room:example.org", None, None, "hello", None)
            .unwrap();
        let (_, _, second) = manager
            .prepare_dispatch("!room:example.org", None, None, "hello again", None)
            .unwrap();

        assert_eq!(first.session_id, second.session_id);
    }

    #[test]
    fn thread_dispatch_uses_distinct_session_key() {
        let mut manager = build_manager();
        let (_, _, main) = manager
            .prepare_dispatch("!room:example.org", None, None, "hello", None)
            .unwrap();
        let (_, _, thread) = manager
            .prepare_dispatch(
                "!room:example.org",
                Some("$thread-root:example.org"),
                None,
                "reply",
                None,
            )
            .unwrap();

        assert_ne!(main.session_id, thread.session_id);
    }

    #[test]
    fn reply_root_dispatch_uses_distinct_session_key() {
        let mut manager = build_manager();
        let (_, _, main) = manager
            .prepare_dispatch("!room:example.org", None, None, "hello", None)
            .unwrap();
        let (_, _, reply) = manager
            .prepare_dispatch(
                "!room:example.org",
                None,
                Some("$reply-root:example.org"),
                "reply",
                None,
            )
            .unwrap();

        assert_ne!(main.session_id, reply.session_id);
        assert!(reply.session_id.contains("reply:$reply-root:example.org"));
    }
}
