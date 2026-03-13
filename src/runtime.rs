use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use serde_json::Value;

#[cfg(feature = "crew")]
use crate::crew_runtime::CrewRuntimeAdapter;
#[cfg(feature = "openclaw")]
use crate::openclaw_runtime::OpenClawRuntimeAdapter;
use crate::state::{BotRuntimeOverride, DeliveryTarget, RuntimeKind, RuntimeProfile};

pub type BotEventStream = Pin<Box<dyn Stream<Item = Result<BotEvent, BotRuntimeError>> + Send>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotRequest {
    pub room_id: String,
    pub thread_root_event_id: Option<String>,
    pub reply_root_event_id: Option<String>,
    pub bot_id: String,
    pub session_id: String,
    pub message: String,
    pub delivery_target: DeliveryTarget,
    pub runtime_override: BotRuntimeOverride,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BotEvent {
    Thinking {
        iteration: u32,
    },
    TextDelta {
        text: String,
    },
    ToolStart {
        name: String,
    },
    ToolEnd {
        name: String,
        success: bool,
    },
    Response {
        iteration: u32,
    },
    CostUpdate {
        input_tokens: u32,
        output_tokens: u32,
        session_cost: Option<f64>,
    },
    StreamEnd,
    Done {
        content: String,
    },
    Error {
        message: String,
    },
    Raw {
        payload: Value,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BotRuntimeError {
    #[error("runtime profile '{runtime_profile_id}' is not a {expected:?} runtime")]
    WrongRuntimeKind {
        runtime_profile_id: String,
        expected: RuntimeKind,
    },
    #[error("runtime feature {0:?} is not enabled in this build")]
    RuntimeFeatureDisabled(RuntimeKind),
    #[error("runtime url '{0}' is invalid")]
    InvalidUrl(String),
    #[cfg(feature = "openclaw")]
    #[error("openclaw runtime error")]
    OpenClaw(#[from] robrix_openclaw_channel::OpenClawTransportError),
    #[error("runtime url parse failed")]
    Url(#[from] url::ParseError),
    #[error("failed to serialize runtime message")]
    Serialization(#[from] serde_json::Error),
    #[cfg(feature = "crew")]
    #[error("crew runtime error")]
    Crew(#[from] robrix_crew_channel::BridgeError),
}

#[async_trait]
pub trait BotRuntime: Send + Sync {
    async fn dispatch_stream(&self, request: BotRequest)
    -> Result<BotEventStream, BotRuntimeError>;
    async fn healthcheck(&self) -> Result<(), BotRuntimeError>;
}

#[derive(Clone, Debug)]
pub enum RuntimeAdapter {
    #[cfg(feature = "crew")]
    Crew(CrewRuntimeAdapter),
    #[cfg(feature = "openclaw")]
    OpenClaw(OpenClawRuntimeAdapter),
}

impl RuntimeAdapter {
    pub fn from_profile(profile: &RuntimeProfile) -> Result<Self, BotRuntimeError> {
        match profile.kind() {
            RuntimeKind::Crew => {
                #[cfg(feature = "crew")]
                {
                    return Ok(Self::Crew(CrewRuntimeAdapter::from_profile(profile)?));
                }
                #[cfg(not(feature = "crew"))]
                {
                    return Err(BotRuntimeError::RuntimeFeatureDisabled(RuntimeKind::Crew));
                }
            }
            RuntimeKind::OpenClaw => {
                #[cfg(feature = "openclaw")]
                {
                    return Ok(Self::OpenClaw(OpenClawRuntimeAdapter::from_profile(
                        profile,
                    )?));
                }
                #[cfg(not(feature = "openclaw"))]
                {
                    return Err(BotRuntimeError::RuntimeFeatureDisabled(
                        RuntimeKind::OpenClaw,
                    ));
                }
            }
        }
    }
}

#[async_trait]
impl BotRuntime for RuntimeAdapter {
    async fn dispatch_stream(
        &self,
        request: BotRequest,
    ) -> Result<BotEventStream, BotRuntimeError> {
        match self {
            #[cfg(feature = "crew")]
            Self::Crew(adapter) => adapter.dispatch_stream(request).await,
            #[cfg(feature = "openclaw")]
            Self::OpenClaw(adapter) => adapter.dispatch_stream(request).await,
        }
    }

    async fn healthcheck(&self) -> Result<(), BotRuntimeError> {
        match self {
            #[cfg(feature = "crew")]
            Self::Crew(adapter) => adapter.healthcheck().await,
            #[cfg(feature = "openclaw")]
            Self::OpenClaw(adapter) => adapter.healthcheck().await,
        }
    }
}

pub fn runtime_feature_enabled(kind: RuntimeKind) -> bool {
    match kind {
        RuntimeKind::Crew => cfg!(feature = "crew"),
        RuntimeKind::OpenClaw => cfg!(feature = "openclaw"),
    }
}
