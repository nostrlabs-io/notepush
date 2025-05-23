use crate::notification_manager::NotificationManager;
use futures::sink::SinkExt;
use futures::StreamExt;
use hyper::upgrade::Upgraded;
use hyper_tungstenite::{HyperWebsocket, WebSocketStream};
use hyper_util::rt::TokioIo;
use nostr::util::JsonUtil;
use nostr::{ClientMessage, RelayMessage};
use serde_json::Value;
use std::fmt::{self, Debug};
use std::str::FromStr;
use std::sync::Arc;
use tungstenite::{Error, Message};

const MAX_CONSECUTIVE_ERRORS: u32 = 10;

pub struct RelayConnection {
    notification_manager: Arc<NotificationManager>,
}

impl RelayConnection {
    // MARK: - Initializers

    pub async fn new(
        notification_manager: Arc<NotificationManager>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        log::info!("Accepted websocket connection");
        Ok(RelayConnection {
            notification_manager,
        })
    }

    pub async fn run(
        websocket: HyperWebsocket,
        notification_manager: Arc<NotificationManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut connection = RelayConnection::new(notification_manager).await?;
        connection.run_loop(websocket).await
    }

    // MARK: - Connection Runtime management

    pub async fn run_loop(
        &mut self,
        websocket: HyperWebsocket,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut consecutive_errors = 0;
        log::debug!("Starting run loop for connection with {:?}", websocket);
        let mut websocket_stream = websocket.await?;
        while let Some(raw_message) = websocket_stream.next().await {
            match self
                .run_loop_iteration_if_raw_message_is_ok(raw_message, &mut websocket_stream)
                .await
            {
                Ok(_) => {
                    consecutive_errors = 0;
                }
                Err(e) => {
                    log::error!("Error in websocket connection: {:?}", e);
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        log::error!("Too many consecutive errors, closing connection");
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn run_loop_iteration_if_raw_message_is_ok(
        &mut self,
        raw_message: Result<Message, Error>,
        stream: &mut WebSocketStream<TokioIo<Upgraded>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let raw_message = raw_message?;
        self.run_loop_iteration(raw_message, stream).await
    }

    pub async fn run_loop_iteration(
        &mut self,
        raw_message: Message,
        stream: &mut WebSocketStream<TokioIo<Upgraded>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if raw_message.is_text() {
            let message: ClientMessage =
                ClientMessage::from_value(Value::from_str(raw_message.to_text()?)?)?;
            let response = self.handle_client_message(message).await?;
            stream
                .send(tungstenite::Message::text(response.try_as_json()?))
                .await?;
        }
        Ok(())
    }

    // MARK: - Message handling

    async fn handle_client_message(
        &self,
        message: ClientMessage,
    ) -> Result<RelayMessage, Box<dyn std::error::Error>> {
        match message {
            ClientMessage::Event(event) => {
                log::info!("Received event with id: {:?}", event.id.to_hex());
                log::debug!("Event received: {:?}", event);
                self.notification_manager
                    .event_saver
                    .save_if_needed(&event)
                    .await?;
                self.notification_manager
                    .send_notifications_if_needed(&event)
                    .await?;
                let notice_message = "blocked: This relay does not store events".to_string();
                let response = RelayMessage::Ok {
                    event_id: event.id,
                    status: false,
                    message: notice_message,
                };
                Ok(response)
            }
            _ => {
                log::info!("Received unsupported Nostr client message");
                log::debug!("Unsupported Nostr client message: {:?}", message);
                let notice_message = "Unsupported message.".to_string();
                let response = RelayMessage::Notice {
                    message: notice_message,
                };
                Ok(response)
            }
        }
    }
}

impl Debug for RelayConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelayConnection with websocket")
    }
}
