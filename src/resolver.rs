use crate::runtime::runtime_feature_enabled;
use crate::state::{
    BotBinding, BotDefinition, BotRuntimeOverride, BotfatherState, DeliveryTarget, DispatchPolicy,
    PermissionPolicy, RuntimeKind, RuntimeProfile, SenderProfile, TriggerPolicy, Workspace,
};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("room '{0}' is not present in the current inventory snapshot")]
    UnknownRoom(String),
    #[error("bot '{0}' was referenced but not found")]
    UnknownBot(String),
    #[error("runtime profile '{0}' was referenced but not found")]
    UnknownRuntimeProfile(String),
    #[error("sender profile '{0}' was referenced but not found")]
    UnknownSenderProfile(String),
    #[error("workspace '{0}' was referenced but not found")]
    UnknownWorkspace(String),
    #[error("no enabled bot is configured for room '{0}'")]
    NoBotsConfigured(String),
    #[error("preferred bot '{bot_id}' is not available in room '{room_id}'")]
    PreferredBotNotAvailable { room_id: String, bot_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingSource {
    Default,
    Space { space_id: String },
    Room { room_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBotBinding {
    pub room_id: String,
    pub source: BindingSource,
    pub binding: Option<BotBinding>,
    pub bot: BotDefinition,
    pub runtime_profile: RuntimeProfile,
    pub sender_profile: SenderProfile,
    pub workspace: Option<Workspace>,
    pub trigger: TriggerPolicy,
    pub delivery: DeliveryTarget,
    pub permissions: PermissionPolicy,
    pub runtime_override: BotRuntimeOverride,
    pub dispatch_policy: DispatchPolicy,
}

impl ResolvedBotBinding {
    pub fn runtime_kind(&self) -> RuntimeKind {
        self.runtime_profile.kind()
    }

    pub fn effective_priority(&self) -> i32 {
        self.binding.as_ref().map_or(0, |binding| binding.priority) + self.bot.priority
    }
}

pub fn resolve_room_bots(
    state: &BotfatherState,
    room_id: &str,
) -> Result<Vec<ResolvedBotBinding>, ResolveError> {
    let room = state
        .inventory
        .rooms
        .get(room_id)
        .ok_or_else(|| ResolveError::UnknownRoom(room_id.to_string()))?;

    let mut resolved = Vec::new();

    for bot_id in &state.defaults.bot_ids {
        resolved.push(resolve_default_bot(state, room_id, bot_id)?);
    }

    for space_id in &room.space_ids {
        if let Some(bindings) = state.space_bindings.get(space_id) {
            for binding in bindings.iter().filter(|binding| binding.enabled) {
                upsert_resolved(
                    &mut resolved,
                    resolve_bound_bot(
                        state,
                        room_id,
                        Some(binding.clone()),
                        BindingSource::Space {
                            space_id: space_id.clone(),
                        },
                        &binding.bot_id,
                    )?,
                );
            }
        }
    }

    if let Some(bindings) = state.room_bindings.get(room_id) {
        for binding in bindings.iter().filter(|binding| binding.enabled) {
            upsert_resolved(
                &mut resolved,
                resolve_bound_bot(
                    state,
                    room_id,
                    Some(binding.clone()),
                    BindingSource::Room {
                        room_id: room_id.to_string(),
                    },
                    &binding.bot_id,
                )?,
            );
        }
    }

    resolved.retain(|resolved| {
        resolved.bot.enabled && runtime_feature_enabled(resolved.runtime_kind())
    });
    if resolved.is_empty() {
        return Err(ResolveError::NoBotsConfigured(room_id.to_string()));
    }

    resolved.sort_by(|lhs, rhs| {
        let source_cmp = source_rank(&rhs.source).cmp(&source_rank(&lhs.source));
        if !source_cmp.is_eq() {
            return source_cmp;
        }

        let same_default_source = matches!(lhs.source, BindingSource::Default)
            && matches!(rhs.source, BindingSource::Default);
        if same_default_source {
            runtime_priority(rhs.runtime_kind())
                .cmp(&runtime_priority(lhs.runtime_kind()))
                .then_with(|| rhs.effective_priority().cmp(&lhs.effective_priority()))
                .then_with(|| lhs.bot.id.cmp(&rhs.bot.id))
        } else {
            rhs.effective_priority()
                .cmp(&lhs.effective_priority())
                .then_with(|| {
                    runtime_priority(rhs.runtime_kind()).cmp(&runtime_priority(lhs.runtime_kind()))
                })
                .then_with(|| lhs.bot.id.cmp(&rhs.bot.id))
        }
    });

    Ok(resolved)
}

pub fn resolve_room_bot(
    state: &BotfatherState,
    room_id: &str,
    preferred_bot_id: Option<&str>,
) -> Result<ResolvedBotBinding, ResolveError> {
    let resolved = resolve_room_bots(state, room_id)?;
    if let Some(bot_id) = preferred_bot_id {
        return resolved
            .into_iter()
            .find(|resolved| resolved.bot.id == bot_id)
            .ok_or_else(|| ResolveError::PreferredBotNotAvailable {
                room_id: room_id.to_string(),
                bot_id: bot_id.to_string(),
            });
    }

    resolved
        .into_iter()
        .next()
        .ok_or_else(|| ResolveError::NoBotsConfigured(room_id.to_string()))
}

fn resolve_default_bot(
    state: &BotfatherState,
    room_id: &str,
    bot_id: &str,
) -> Result<ResolvedBotBinding, ResolveError> {
    resolve_bound_bot(state, room_id, None, BindingSource::Default, bot_id)
}

fn resolve_bound_bot(
    state: &BotfatherState,
    room_id: &str,
    binding: Option<BotBinding>,
    source: BindingSource,
    bot_id: &str,
) -> Result<ResolvedBotBinding, ResolveError> {
    let bot = state
        .bots
        .get(bot_id)
        .cloned()
        .ok_or_else(|| ResolveError::UnknownBot(bot_id.to_string()))?;
    let runtime_profile = state
        .runtime_profiles
        .get(&bot.runtime_profile_id)
        .cloned()
        .ok_or_else(|| ResolveError::UnknownRuntimeProfile(bot.runtime_profile_id.clone()))?;
    let sender_profile = resolve_sender_profile(state, &bot, binding.as_ref())?;
    let workspace = runtime_profile
        .workspace_id
        .as_ref()
        .map(|workspace_id| {
            state
                .workspaces
                .get(workspace_id)
                .cloned()
                .ok_or_else(|| ResolveError::UnknownWorkspace(workspace_id.clone()))
        })
        .transpose()?;

    let trigger = binding
        .as_ref()
        .and_then(|binding| binding.trigger.clone())
        .unwrap_or_else(|| bot.trigger.clone());
    let delivery = binding
        .as_ref()
        .and_then(|binding| binding.delivery)
        .unwrap_or(bot.default_delivery);
    let permissions = binding
        .as_ref()
        .and_then(|binding| binding.permissions.clone())
        .unwrap_or_else(|| bot.permissions.clone());
    let runtime_override = bot.runtime_override.clone();
    let dispatch_policy = bot
        .dispatch_policy_override
        .clone()
        .unwrap_or_else(|| runtime_profile.dispatch_policy.clone());

    Ok(ResolvedBotBinding {
        room_id: room_id.to_string(),
        source,
        binding,
        bot,
        runtime_profile,
        sender_profile,
        workspace,
        trigger,
        delivery,
        permissions,
        runtime_override,
        dispatch_policy,
    })
}

fn resolve_sender_profile(
    state: &BotfatherState,
    bot: &BotDefinition,
    binding: Option<&BotBinding>,
) -> Result<SenderProfile, ResolveError> {
    let sender_profile_id = binding
        .and_then(|binding| binding.sender_profile_id.as_ref())
        .or(bot.default_sender_profile_id.as_ref())
        .or(state.defaults.default_sender_profile_id.as_ref())
        .ok_or_else(|| ResolveError::UnknownSenderProfile("(none)".into()))?;
    let sender_profile = state
        .sender_profiles
        .get(sender_profile_id)
        .cloned()
        .ok_or_else(|| ResolveError::UnknownSenderProfile(sender_profile_id.clone()))?;
    Ok(sender_profile)
}

fn upsert_resolved(resolved: &mut Vec<ResolvedBotBinding>, next: ResolvedBotBinding) {
    if let Some(index) = resolved
        .iter()
        .position(|current| current.bot.id == next.bot.id)
    {
        resolved[index] = next;
    } else {
        resolved.push(next);
    }
}

fn runtime_priority(kind: RuntimeKind) -> i32 {
    match kind {
        RuntimeKind::Crew => 2,
        RuntimeKind::OpenClaw => 1,
    }
}

fn source_rank(source: &BindingSource) -> i32 {
    match source {
        BindingSource::Room { .. } => 3,
        BindingSource::Space { .. } => 2,
        BindingSource::Default => 1,
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(feature = "crew", feature = "openclaw"))]
    use std::collections::BTreeMap;
    #[cfg(all(feature = "crew", feature = "openclaw"))]
    use std::path::PathBuf;

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    use super::{BindingSource, resolve_room_bot, resolve_room_bots};
    #[cfg(all(feature = "crew", feature = "openclaw"))]
    use crate::state::{
        BotBinding, BotfatherDefaults, OpenClawRuntimeConfig, SenderProfile, SenderProfileKind,
        SenderSecurityLevel, TriggerMode, UserSnapshot, Workspace,
    };
    #[cfg(all(feature = "crew", feature = "openclaw"))]
    use crate::state::{
        BotDefinition, BotfatherState, DeliveryTarget, InventorySnapshot, PermissionPolicy,
        RoomInventory, RuntimeConfig, RuntimeProfile, TriggerPolicy,
    };

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    fn base_state() -> BotfatherState {
        let mut state = BotfatherState {
            user: UserSnapshot {
                matrix_user_id: Some("@user:example.org".into()),
                homeserver_url: Some("https://matrix.example.org".into()),
            },
            inventory: InventorySnapshot {
                rooms: BTreeMap::from([(
                    "!room:example.org".into(),
                    RoomInventory {
                        room_id: "!room:example.org".into(),
                        display_name: Some("Main".into()),
                        canonical_alias: None,
                        space_ids: vec!["!space:example.org".into()],
                        is_direct: false,
                        stale: false,
                    },
                )]),
                spaces: BTreeMap::new(),
            },
            ..Default::default()
        };
        state.workspaces.insert(
            "workspace".into(),
            Workspace {
                id: "workspace".into(),
                name: "Crew Workspace".into(),
                root_dir: PathBuf::from("/tmp/workspace"),
                data_dir: None,
                skills_dirs: Vec::new(),
                description: None,
            },
        );
        state.runtime_profiles.insert(
            "crew".into(),
            RuntimeProfile {
                id: "crew".into(),
                name: "Crew Runtime".into(),
                workspace_id: Some("workspace".into()),
                description: None,
                dispatch_policy: Default::default(),
                config: RuntimeConfig::Crew {
                    base_url: "http://127.0.0.1:8000".into(),
                    api_key_env: Some("CREW_API_TOKEN".into()),
                    model: None,
                    system_prompt: None,
                },
            },
        );
        state.runtime_profiles.insert(
            "openclaw".into(),
            RuntimeProfile {
                id: "openclaw".into(),
                name: "OpenClaw Runtime".into(),
                workspace_id: None,
                description: None,
                dispatch_policy: Default::default(),
                config: RuntimeConfig::OpenClaw(OpenClawRuntimeConfig {
                    gateway_url: "ws://127.0.0.1:24282/ws".into(),
                    auth_token_env: None,
                    agent_id: "main".into(),
                }),
            },
        );
        state.bots.insert(
            "crew-main".into(),
            BotDefinition {
                id: "crew-main".into(),
                name: "Crew Main".into(),
                runtime_profile_id: "crew".into(),
                default_sender_profile_id: Some("current-user".into()),
                priority: 0,
                enabled: true,
                trigger: TriggerPolicy {
                    mode: TriggerMode::Manual,
                    ..Default::default()
                },
                default_delivery: DeliveryTarget::CurrentRoom,
                permissions: PermissionPolicy::default(),
                runtime_override: Default::default(),
                dispatch_policy_override: None,
                description: None,
            },
        );
        state.bots.insert(
            "openclaw-main".into(),
            BotDefinition {
                id: "openclaw-main".into(),
                name: "OpenClaw Main".into(),
                runtime_profile_id: "openclaw".into(),
                default_sender_profile_id: Some("current-user".into()),
                priority: 100,
                enabled: true,
                trigger: TriggerPolicy {
                    mode: TriggerMode::Mention,
                    ..Default::default()
                },
                default_delivery: DeliveryTarget::CurrentThread,
                permissions: PermissionPolicy::default(),
                runtime_override: Default::default(),
                dispatch_policy_override: None,
                description: None,
            },
        );
        state.sender_profiles.insert(
            "current-user".into(),
            SenderProfile {
                id: "current-user".into(),
                name: "Current User".into(),
                enabled: true,
                kind: SenderProfileKind::CurrentUser,
                matrix_user_id: Some("@user:example.org".into()),
                homeserver_url: Some("https://matrix.example.org".into()),
                device_id: None,
                access_token_env: None,
                access_token: None,
                last_verified_at_millis: None,
                last_verification_error: None,
                security: SenderSecurityLevel::Standard,
                description: None,
            },
        );
        state
    }

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    #[test]
    fn crew_is_primary_when_both_defaults_are_enabled() {
        let mut state = base_state();
        state.defaults = BotfatherDefaults {
            bot_ids: vec!["openclaw-main".into(), "crew-main".into()],
            default_sender_profile_id: Some("current-user".into()),
            ..Default::default()
        };

        let resolved = resolve_room_bots(&state, "!room:example.org").unwrap();

        assert_eq!(resolved[0].bot.id, "crew-main");
        assert_eq!(resolved[1].bot.id, "openclaw-main");
    }

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    #[test]
    fn preferred_bot_can_override_primary_selection() {
        let mut state = base_state();
        state.defaults = BotfatherDefaults {
            bot_ids: vec!["openclaw-main".into(), "crew-main".into()],
            default_sender_profile_id: Some("current-user".into()),
            ..Default::default()
        };

        let resolved =
            resolve_room_bot(&state, "!room:example.org", Some("openclaw-main")).unwrap();

        assert_eq!(resolved.bot.id, "openclaw-main");
    }

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    #[test]
    fn room_binding_overrides_space_and_default_for_same_bot() {
        let mut state = base_state();
        state.defaults = BotfatherDefaults {
            bot_ids: vec!["crew-main".into()],
            default_sender_profile_id: Some("current-user".into()),
            ..Default::default()
        };
        state.space_bindings.insert(
            "!space:example.org".into(),
            vec![BotBinding {
                bot_id: "openclaw-main".into(),
                enabled: true,
                priority: 0,
                trigger: None,
                delivery: Some(DeliveryTarget::CurrentThread),
                permissions: None,
                sender_profile_id: None,
            }],
        );
        state.room_bindings.insert(
            "!room:example.org".into(),
            vec![BotBinding {
                bot_id: "openclaw-main".into(),
                enabled: true,
                priority: 42,
                trigger: None,
                delivery: Some(DeliveryTarget::CurrentRoom),
                permissions: None,
                sender_profile_id: None,
            }],
        );

        let resolved = resolve_room_bots(&state, "!room:example.org").unwrap();
        let openclaw = resolved
            .into_iter()
            .find(|resolved| resolved.bot.id == "openclaw-main")
            .unwrap();

        assert!(matches!(openclaw.source, BindingSource::Room { .. }));
        assert_eq!(openclaw.delivery, DeliveryTarget::CurrentRoom);
        assert_eq!(openclaw.binding.unwrap().priority, 42);
    }

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    #[test]
    fn room_binding_beats_default_runtime_priority() {
        let mut state = base_state();
        state.defaults = BotfatherDefaults {
            bot_ids: vec!["crew-main".into()],
            default_sender_profile_id: Some("current-user".into()),
            ..Default::default()
        };
        state.room_bindings.insert(
            "!room:example.org".into(),
            vec![BotBinding {
                bot_id: "openclaw-main".into(),
                enabled: true,
                priority: 0,
                trigger: None,
                delivery: None,
                permissions: None,
                sender_profile_id: None,
            }],
        );

        let resolved = resolve_room_bot(&state, "!room:example.org", None).unwrap();

        assert_eq!(resolved.bot.id, "openclaw-main");
        assert!(matches!(resolved.source, BindingSource::Room { .. }));
    }

    #[cfg(all(feature = "crew", feature = "openclaw"))]
    #[test]
    fn room_binding_can_override_sender_profile() {
        let mut state = base_state();
        state.defaults = BotfatherDefaults {
            bot_ids: vec!["crew-main".into()],
            default_sender_profile_id: Some("current-user".into()),
            ..Default::default()
        };
        state.sender_profiles.insert(
            "secure-room-bot".into(),
            SenderProfile {
                id: "secure-room-bot".into(),
                name: "Secure Room Bot".into(),
                enabled: true,
                kind: SenderProfileKind::MatrixBot,
                matrix_user_id: Some("@secure-bot:example.org".into()),
                homeserver_url: Some("https://matrix.example.org".into()),
                device_id: Some("SECUREBOT01".into()),
                access_token_env: Some("SECURE_ROOM_BOT_ACCESS_TOKEN".into()),
                access_token: None,
                last_verified_at_millis: None,
                last_verification_error: None,
                security: SenderSecurityLevel::Isolated,
                description: None,
            },
        );
        state.room_bindings.insert(
            "!room:example.org".into(),
            vec![BotBinding {
                bot_id: "crew-main".into(),
                enabled: true,
                priority: 0,
                trigger: None,
                delivery: None,
                permissions: None,
                sender_profile_id: Some("secure-room-bot".into()),
            }],
        );

        let resolved = resolve_room_bot(&state, "!room:example.org", None).unwrap();

        assert_eq!(resolved.bot.id, "crew-main");
        assert_eq!(resolved.sender_profile.id, "secure-room-bot");
        assert_eq!(
            resolved.sender_profile.security,
            SenderSecurityLevel::Isolated
        );
    }
}
