use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const STATE_VERSION: u32 = 4;

pub type StateVersion = u32;
pub type WorkspaceId = String;
pub type RuntimeProfileId = String;
pub type BotId = String;
pub type RoomId = String;
pub type SpaceId = String;
pub type SenderProfileId = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotfatherState {
    #[serde(default = "default_state_version")]
    pub version: StateVersion,
    #[serde(default)]
    pub user: UserSnapshot,
    #[serde(default)]
    pub inventory: InventorySnapshot,
    #[serde(default)]
    pub workspaces: BTreeMap<WorkspaceId, Workspace>,
    #[serde(default)]
    pub runtime_profiles: BTreeMap<RuntimeProfileId, RuntimeProfile>,
    #[serde(default)]
    pub bots: BTreeMap<BotId, BotDefinition>,
    #[serde(default)]
    pub sender_profiles: BTreeMap<SenderProfileId, SenderProfile>,
    #[serde(default)]
    pub space_bindings: BTreeMap<SpaceId, Vec<BotBinding>>,
    #[serde(default)]
    pub room_bindings: BTreeMap<RoomId, Vec<BotBinding>>,
    #[serde(default)]
    pub defaults: BotfatherDefaults,
    #[serde(default)]
    pub runtime: RuntimeState,
}

impl Default for BotfatherState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            user: UserSnapshot::default(),
            inventory: InventorySnapshot::default(),
            workspaces: BTreeMap::new(),
            runtime_profiles: BTreeMap::new(),
            bots: BTreeMap::new(),
            sender_profiles: BTreeMap::new(),
            space_bindings: BTreeMap::new(),
            room_bindings: BTreeMap::new(),
            defaults: BotfatherDefaults::default(),
            runtime: RuntimeState::default(),
        }
    }
}

impl BotfatherState {
    pub fn refresh_inventory(&mut self, user: UserSnapshot, inventory: InventorySnapshot) {
        self.user = user;
        self.inventory = inventory;
    }

    pub fn normalize(&mut self) {
        if self.version == 0 || self.version < STATE_VERSION {
            self.version = STATE_VERSION;
        }

        self.runtime.active_sessions = self
            .runtime
            .active_sessions
            .values()
            .cloned()
            .map(|mut record| {
                record.key.normalize();
                (record.key.clone(), record)
            })
            .collect();
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserSnapshot {
    pub matrix_user_id: Option<String>,
    pub homeserver_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventorySnapshot {
    #[serde(default)]
    pub rooms: BTreeMap<RoomId, RoomInventory>,
    #[serde(default)]
    pub spaces: BTreeMap<SpaceId, SpaceInventory>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomInventory {
    pub room_id: RoomId,
    pub display_name: Option<String>,
    pub canonical_alias: Option<String>,
    #[serde(default)]
    pub space_ids: Vec<SpaceId>,
    #[serde(default)]
    pub is_direct: bool,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpaceInventory {
    pub space_id: SpaceId,
    pub display_name: Option<String>,
    pub canonical_alias: Option<String>,
    #[serde(default)]
    pub child_room_ids: Vec<RoomId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub root_dir: PathBuf,
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub skills_dirs: Vec<PathBuf>,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeKind {
    #[default]
    Crew,
    OpenClaw,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub id: RuntimeProfileId,
    pub name: String,
    pub workspace_id: Option<WorkspaceId>,
    pub description: Option<String>,
    #[serde(default)]
    pub dispatch_policy: DispatchPolicy,
    pub config: RuntimeConfig,
}

impl RuntimeProfile {
    pub fn kind(&self) -> RuntimeKind {
        self.config.kind()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RuntimeConfig {
    Crew {
        base_url: String,
        #[serde(default)]
        api_key_env: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        system_prompt: Option<String>,
    },
    OpenClaw(OpenClawRuntimeConfig),
}

impl RuntimeConfig {
    pub fn kind(&self) -> RuntimeKind {
        match self {
            Self::Crew { .. } => RuntimeKind::Crew,
            Self::OpenClaw(_) => RuntimeKind::OpenClaw,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenClawRuntimeConfig {
    pub gateway_url: String,
    pub auth_token_env: Option<String>,
    #[serde(default = "default_openclaw_agent_id")]
    pub agent_id: String,
}

impl Default for OpenClawRuntimeConfig {
    fn default() -> Self {
        Self {
            gateway_url: String::new(),
            auth_token_env: None,
            agent_id: default_openclaw_agent_id(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotDefinition {
    pub id: BotId,
    pub name: String,
    pub runtime_profile_id: RuntimeProfileId,
    #[serde(default)]
    pub default_sender_profile_id: Option<SenderProfileId>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub trigger: TriggerPolicy,
    #[serde(default)]
    pub default_delivery: DeliveryTarget,
    #[serde(default)]
    pub permissions: PermissionPolicy,
    #[serde(default)]
    pub runtime_override: BotRuntimeOverride,
    #[serde(default)]
    pub dispatch_policy_override: Option<DispatchPolicy>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotRuntimeOverride {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
}

impl BotRuntimeOverride {
    pub fn is_empty(&self) -> bool {
        self.model.is_none() && self.system_prompt.is_none() && self.agent_id.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchPolicy {
    #[serde(default = "default_max_parallel_per_room")]
    pub max_parallel_per_room: usize,
    #[serde(default = "default_max_parallel_per_runtime")]
    pub max_parallel_per_runtime: usize,
    #[serde(default = "default_queue_limit")]
    pub queue_limit: usize,
}

impl Default for DispatchPolicy {
    fn default() -> Self {
        Self {
            max_parallel_per_room: default_max_parallel_per_room(),
            max_parallel_per_runtime: default_max_parallel_per_runtime(),
            queue_limit: default_queue_limit(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotBinding {
    pub bot_id: BotId,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    pub trigger: Option<TriggerPolicy>,
    pub delivery: Option<DeliveryTarget>,
    pub permissions: Option<PermissionPolicy>,
    #[serde(default)]
    pub sender_profile_id: Option<SenderProfileId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotfatherDefaults {
    #[serde(default)]
    pub bot_ids: Vec<BotId>,
    #[serde(default)]
    pub default_sender_profile_id: Option<SenderProfileId>,
    #[serde(default)]
    pub room_stream_preview_enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SenderProfileKind {
    #[default]
    CurrentUser,
    MatrixBot,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SenderSecurityLevel {
    #[default]
    Standard,
    Elevated,
    Isolated,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SenderProfile {
    pub id: SenderProfileId,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kind: SenderProfileKind,
    #[serde(default)]
    pub matrix_user_id: Option<String>,
    #[serde(default)]
    pub homeserver_url: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub access_token_env: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub last_verified_at_millis: Option<u64>,
    #[serde(default)]
    pub last_verification_error: Option<String>,
    #[serde(default)]
    pub security: SenderSecurityLevel,
    pub description: Option<String>,
}

impl SenderProfile {
    pub fn uses_current_user(&self) -> bool {
        self.kind == SenderProfileKind::CurrentUser
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerMode {
    #[default]
    Manual,
    Mention,
    Command,
    Event,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TriggerPolicy {
    #[serde(default)]
    pub mode: TriggerMode,
    pub command_prefix: Option<String>,
    pub mention_name: Option<String>,
    #[serde(default)]
    pub reply_only: bool,
    #[serde(default)]
    pub thread_only: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryTarget {
    #[default]
    CurrentRoom,
    CurrentThread,
    ReplyToSource,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionPolicy {
    #[serde(default = "default_true")]
    pub can_post_to_room: bool,
    #[serde(default = "default_true")]
    pub can_post_to_thread: bool,
    #[serde(default)]
    pub can_react: bool,
    #[serde(default = "default_true")]
    pub can_use_tools: bool,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            can_post_to_room: true,
            can_post_to_thread: true,
            can_react: false,
            can_use_tools: true,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeState {
    #[serde(default, with = "session_map_serde")]
    pub active_sessions: BTreeMap<SessionKey, SessionRecord>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum SessionScopeKind {
    #[default]
    Room,
    Thread,
    ReplyRoot,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionKey {
    pub room_id: RoomId,
    #[serde(default)]
    pub scope_kind: SessionScopeKind,
    #[serde(default)]
    pub thread_root_event_id: Option<String>,
    #[serde(default)]
    pub reply_root_event_id: Option<String>,
    pub bot_id: BotId,
}

impl SessionKey {
    pub fn normalize(&mut self) {
        match self.scope_kind {
            SessionScopeKind::Thread => {
                if self.thread_root_event_id.is_none() {
                    self.scope_kind = if self.reply_root_event_id.is_some() {
                        SessionScopeKind::ReplyRoot
                    } else {
                        SessionScopeKind::Room
                    };
                }
                if self.scope_kind == SessionScopeKind::Thread {
                    self.reply_root_event_id = None;
                }
            }
            SessionScopeKind::ReplyRoot => {
                if self.reply_root_event_id.is_none() {
                    self.scope_kind = if self.thread_root_event_id.is_some() {
                        SessionScopeKind::Thread
                    } else {
                        SessionScopeKind::Room
                    };
                }
                if self.scope_kind == SessionScopeKind::ReplyRoot {
                    self.thread_root_event_id = None;
                }
            }
            SessionScopeKind::Room => {
                if self.thread_root_event_id.is_some() {
                    self.scope_kind = SessionScopeKind::Thread;
                    self.reply_root_event_id = None;
                } else if self.reply_root_event_id.is_some() {
                    self.scope_kind = SessionScopeKind::ReplyRoot;
                    self.thread_root_event_id = None;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub key: SessionKey,
    pub runtime_profile_id: RuntimeProfileId,
    pub session_id: String,
}

fn default_state_version() -> StateVersion {
    STATE_VERSION
}

fn default_true() -> bool {
    true
}

fn default_openclaw_agent_id() -> String {
    "main".to_string()
}

fn default_max_parallel_per_room() -> usize {
    1
}

fn default_max_parallel_per_runtime() -> usize {
    1
}

fn default_queue_limit() -> usize {
    8
}

mod session_map_serde {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{SessionKey, SessionRecord};

    pub fn serialize<S>(
        active_sessions: &BTreeMap<SessionKey, SessionRecord>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let records = active_sessions.values().collect::<Vec<_>>();
        records.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<SessionKey, SessionRecord>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum SessionMapRepresentation {
            Records(Vec<SessionRecord>),
            LegacyMap(BTreeMap<String, SessionRecord>),
        }

        let records = match SessionMapRepresentation::deserialize(deserializer)? {
            SessionMapRepresentation::Records(records) => records,
            SessionMapRepresentation::LegacyMap(records) => records.into_values().collect(),
        };
        Ok(records
            .into_iter()
            .map(|record| (record.key.clone(), record))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        BotfatherState, RuntimeState, STATE_VERSION, SessionKey, SessionRecord, SessionScopeKind,
    };

    #[test]
    fn runtime_state_serializes_sessions_as_records() {
        let key = SessionKey {
            room_id: "!room:example.org".into(),
            scope_kind: SessionScopeKind::Thread,
            thread_root_event_id: Some("$thread".into()),
            reply_root_event_id: None,
            bot_id: "default-crew-bot".into(),
        };
        let record = SessionRecord {
            key: key.clone(),
            runtime_profile_id: "default-crew-runtime".into(),
            session_id: "session-123".into(),
        };

        let state = BotfatherState {
            runtime: RuntimeState {
                active_sessions: BTreeMap::from([(key, record)]),
            },
            ..BotfatherState::default()
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("\"active_sessions\": ["));
        assert!(json.contains("\"session_id\": \"session-123\""));
        assert!(json.contains("\"scope_kind\": \"thread\""));
    }

    #[test]
    fn runtime_state_deserializes_sessions_from_records() {
        let json = r#"
        {
          "version": 2,
          "runtime": {
            "active_sessions": [
              {
                "key": {
                  "room_id": "!room:example.org",
                  "scope_kind": "room",
                  "thread_root_event_id": null,
                  "reply_root_event_id": null,
                  "bot_id": "default-openclaw-bot"
                },
                "runtime_profile_id": "default-openclaw-runtime",
                "session_id": "session-456"
              }
            ]
          }
        }
        "#;

        let mut state: BotfatherState = serde_json::from_str(json).unwrap();
        state.normalize();
        let record = state
            .runtime
            .active_sessions
            .get(&SessionKey {
                room_id: "!room:example.org".into(),
                scope_kind: SessionScopeKind::Room,
                thread_root_event_id: None,
                reply_root_event_id: None,
                bot_id: "default-openclaw-bot".into(),
            })
            .unwrap();

        assert_eq!(record.runtime_profile_id, "default-openclaw-runtime");
        assert_eq!(record.session_id, "session-456");
    }

    #[test]
    fn runtime_state_deserializes_legacy_empty_map() {
        let json = r#"
        {
          "version": 1,
          "runtime": {
            "active_sessions": {}
          }
        }
        "#;

        let mut state: BotfatherState = serde_json::from_str(json).unwrap();
        state.normalize();
        assert!(state.runtime.active_sessions.is_empty());
        assert_eq!(state.version, STATE_VERSION);
    }

    #[test]
    fn runtime_state_deserializes_legacy_map_records() {
        let json = r#"
        {
          "version": 1,
          "runtime": {
            "active_sessions": {
              "legacy-session-key": {
                "key": {
                  "room_id": "!room:example.org",
                  "thread_root_event_id": "$root",
                  "bot_id": "default-crew-bot"
                },
                "runtime_profile_id": "default-crew-runtime",
                "session_id": "session-789"
              }
            }
          }
        }
        "#;

        let mut state: BotfatherState = serde_json::from_str(json).unwrap();
        state.normalize();
        let record = state
            .runtime
            .active_sessions
            .get(&SessionKey {
                room_id: "!room:example.org".into(),
                scope_kind: SessionScopeKind::Thread,
                thread_root_event_id: Some("$root".into()),
                reply_root_event_id: None,
                bot_id: "default-crew-bot".into(),
            })
            .unwrap();

        assert_eq!(record.session_id, "session-789");
    }

    #[test]
    fn normalize_promotes_reply_scope_from_legacy_fields() {
        let mut state = BotfatherState {
            version: 1,
            runtime: RuntimeState {
                active_sessions: BTreeMap::from([(
                    SessionKey {
                        room_id: "!room:example.org".into(),
                        scope_kind: SessionScopeKind::Room,
                        thread_root_event_id: None,
                        reply_root_event_id: Some("$reply".into()),
                        bot_id: "crew".into(),
                    },
                    SessionRecord {
                        key: SessionKey {
                            room_id: "!room:example.org".into(),
                            scope_kind: SessionScopeKind::Room,
                            thread_root_event_id: None,
                            reply_root_event_id: Some("$reply".into()),
                            bot_id: "crew".into(),
                        },
                        runtime_profile_id: "crew-runtime".into(),
                        session_id: "session-111".into(),
                    },
                )]),
            },
            ..BotfatherState::default()
        };

        state.normalize();

        assert!(state.runtime.active_sessions.contains_key(&SessionKey {
            room_id: "!room:example.org".into(),
            scope_kind: SessionScopeKind::ReplyRoot,
            thread_root_event_id: None,
            reply_root_event_id: Some("$reply".into()),
            bot_id: "crew".into(),
        }));
        assert_eq!(state.version, STATE_VERSION);
    }
}
