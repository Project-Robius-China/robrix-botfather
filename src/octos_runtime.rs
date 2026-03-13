use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use robrix_octos_channel::{
    BridgeEvent, CrewProfileOverride, CrewStreamRequest, CrewTransport, SseHttpTransport,
};
use url::Url;

use crate::runtime::{BotEvent, BotEventStream, BotRequest, BotRuntime, BotRuntimeError};
use crate::state::{RuntimeConfig, RuntimeKind, RuntimeProfile};

#[derive(Clone, Debug)]
pub struct OctosRuntimeAdapter {
    transport: SseHttpTransport,
    base_override: CrewProfileOverride,
}

impl OctosRuntimeAdapter {
    pub fn from_profile(profile: &RuntimeProfile) -> Result<Self, BotRuntimeError> {
        let RuntimeConfig::Crew {
            base_url,
            api_key_env,
            model,
            system_prompt,
        } = &profile.config
        else {
            return Err(BotRuntimeError::WrongRuntimeKind {
                runtime_profile_id: profile.id.clone(),
                expected: RuntimeKind::Crew,
            });
        };

        let mut transport = SseHttpTransport::new(Url::parse(base_url)?);
        if let Some(env_var) = api_key_env.as_deref()
            && let Ok(token) = std::env::var(env_var)
            && !token.is_empty()
        {
            transport = transport.with_auth_token(token);
        }

        Ok(Self {
            transport,
            base_override: CrewProfileOverride {
                model: model.clone(),
                system_prompt: system_prompt.clone(),
            },
        })
    }
}

#[async_trait]
impl BotRuntime for OctosRuntimeAdapter {
    async fn dispatch_stream(
        &self,
        request: BotRequest,
    ) -> Result<BotEventStream, BotRuntimeError> {
        self.transport
            .apply_profile_override(&CrewProfileOverride {
                model: request
                    .runtime_override
                    .model
                    .clone()
                    .or_else(|| self.base_override.model.clone()),
                system_prompt: request
                    .runtime_override
                    .system_prompt
                    .clone()
                    .or_else(|| self.base_override.system_prompt.clone()),
            })
            .await?;

        let stream = self
            .transport
            .submit_stream(CrewStreamRequest {
                session_id: request.session_id,
                message: request.message,
            })
            .await?;

        let mapped = try_stream! {
            futures_util::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                let event = event?;
                match event {
                    BridgeEvent::Thinking { iteration } => yield BotEvent::Thinking { iteration },
                    BridgeEvent::TextDelta { text } => yield BotEvent::TextDelta { text },
                    BridgeEvent::ToolStart { name } => yield BotEvent::ToolStart { name },
                    BridgeEvent::ToolEnd { name, success } => yield BotEvent::ToolEnd { name, success },
                    BridgeEvent::Response { iteration } => yield BotEvent::Response { iteration },
                    BridgeEvent::CostUpdate {
                        input_tokens,
                        output_tokens,
                        session_cost,
                    } => yield BotEvent::CostUpdate {
                        input_tokens,
                        output_tokens,
                        session_cost,
                    },
                    BridgeEvent::StreamEnd => yield BotEvent::StreamEnd,
                    BridgeEvent::Done { content, .. } => yield BotEvent::Done { content },
                    BridgeEvent::Error { message } => yield BotEvent::Error { message },
                    BridgeEvent::Raw { payload } => yield BotEvent::Raw { payload },
                }
            }
        };

        Ok(Box::pin(mapped))
    }

    async fn healthcheck(&self) -> Result<(), BotRuntimeError> {
        self.transport.healthcheck().await?;
        Ok(())
    }
}
