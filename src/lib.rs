//! BotFather control-plane and runtime adapters for Robrix.
//!
//! This crate intentionally stays UI-agnostic. Robrix should only need to:
//! 1. keep the Matrix inventory snapshot in sync with the logged-in user,
//! 2. edit the bot/runtime policy section through its own UI,
//! 3. resolve a room into one or more bot runtimes before dispatching messages.

#[cfg(feature = "crew")]
pub mod crew_runtime;
pub mod manager;
#[cfg(feature = "openclaw")]
pub mod openclaw_runtime;
pub mod resolver;
pub mod runtime;
pub mod state;
pub mod store;

#[cfg(feature = "crew")]
pub use crew_runtime::CrewRuntimeAdapter;
pub use manager::{BotfatherManager, BotfatherManagerError};
#[cfg(feature = "openclaw")]
pub use openclaw_runtime::OpenClawRuntimeAdapter;
pub use resolver::{
    BindingSource, ResolveError, ResolvedBotBinding, resolve_room_bot, resolve_room_bots,
};
pub use runtime::{
    BotEvent, BotEventStream, BotRequest, BotRuntime, BotRuntimeError, RuntimeAdapter,
    runtime_feature_enabled,
};
pub use state::{
    BotBinding, BotDefinition, BotRuntimeOverride, BotfatherDefaults, BotfatherState,
    DeliveryTarget, DispatchPolicy, InventorySnapshot, OpenClawRuntimeConfig, PermissionPolicy,
    RoomInventory, RuntimeConfig, RuntimeKind, RuntimeProfile, RuntimeState, SessionKey,
    SessionRecord, SessionScopeKind, SpaceInventory, TriggerMode, TriggerPolicy, UserSnapshot,
    Workspace,
};
pub use store::{StateStore, StateStoreError};
