use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use robrix_openclaw_channel::{
    OpenClawEvent, OpenClawStreamRequest, OpenClawTransport, OpenClawWsTransport,
};

use crate::runtime::{BotEvent, BotEventStream, BotRequest, BotRuntime, BotRuntimeError};
use crate::state::{RuntimeConfig, RuntimeKind, RuntimeProfile};

#[derive(Clone, Debug)]
pub struct OpenClawRuntimeAdapter {
    transport: OpenClawWsTransport,
    agent_id: String,
}

impl OpenClawRuntimeAdapter {
    pub fn from_profile(profile: &RuntimeProfile) -> Result<Self, BotRuntimeError> {
        let RuntimeConfig::OpenClaw(config) = &profile.config else {
            return Err(BotRuntimeError::WrongRuntimeKind {
                runtime_profile_id: profile.id.clone(),
                expected: RuntimeKind::OpenClaw,
            });
        };

        let mut transport = OpenClawWsTransport::new(&config.gateway_url)?;
        if let Some(env_var) = config.auth_token_env.as_deref()
            && let Ok(token) = std::env::var(env_var)
            && !token.is_empty()
        {
            transport = transport.with_auth_token(token);
        }

        Ok(Self {
            transport,
            agent_id: config.agent_id.clone(),
        })
    }
}

#[async_trait]
impl BotRuntime for OpenClawRuntimeAdapter {
    async fn dispatch_stream(
        &self,
        request: BotRequest,
    ) -> Result<BotEventStream, BotRuntimeError> {
        let stream = self
            .transport
            .submit_stream(OpenClawStreamRequest {
                session_id: request.session_id,
                message: request.message,
                agent_id: request
                    .runtime_override
                    .agent_id
                    .clone()
                    .unwrap_or_else(|| self.agent_id.clone()),
            })
            .await?;

        let mapped = try_stream! {
            futures_util::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                let event = event?;
                match event {
                    OpenClawEvent::TextDelta { text } => yield BotEvent::TextDelta { text },
                    OpenClawEvent::Done { content } => yield BotEvent::Done { content },
                    OpenClawEvent::Error { message } => yield BotEvent::Error { message },
                    OpenClawEvent::Raw { payload } => yield BotEvent::Raw { payload },
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
