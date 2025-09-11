use anyhow::Result;
use chrono::{DateTime, Duration, FixedOffset};
use futures_util::{stream::{SplitSink, SplitStream}, StreamExt};
use serde::Serialize;
use serde_json::Value;
use tokio::{net::TcpStream, spawn, sync::mpsc::Sender};
use tungstenite::{client::IntoClientRequest, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio;
use crate::{prediction::Outcome, websocket::{Transport, WSResponse}};

const EVENTSUB_URL: &'static str = "wss://eventsub.wss.twitch.tv/ws";

#[derive(Debug)]
pub enum EventsubClientError {
    EventSubUrlError,
    AlreadyClosed,
    ConnectionClosed,
    PayloadSessionNotFound,
    SubscriptionNotFound,
    EventTypeNotSupported,
    ParameterNotFound(String),
    OutcomesParsingFail(String),

    Unknown,
}

impl From<tungstenite::Error> for EventsubClientError {
    fn from(err: tungstenite::Error) -> Self {
        match err {
            tungstenite::Error::AlreadyClosed => EventsubClientError::AlreadyClosed,
            tungstenite::Error::ConnectionClosed => EventsubClientError::ConnectionClosed,
            _ => EventsubClientError::Unknown,
        }
    }
}

pub enum WSMessageType {
    SessionWelcome,
    SessionKeepAlive,
    Notification,
    SessionReconnect,
    Revocation,
    Other,
}

impl From<&str> for WSMessageType {
    fn from(value: &str) -> Self {
        match value {
            "session_welcome" => WSMessageType::SessionWelcome,
            "session_keepalive" => WSMessageType::SessionKeepAlive,
            "notification" => WSMessageType::Notification,
            "session_reconnect" => WSMessageType::SessionReconnect,
            "revocation" => WSMessageType::Revocation,
            _ => return WSMessageType::Other,
        }
    }
}

pub enum WSAction {
    SessionWelcome {
        session_id: String,
    },
    SessionKeepAlive,
    Notification {
        event_type: EventType,
    },
    SessionReconnect,
    Revocation,
}

pub enum EventType {
    ChannelPredictionBegin {
        locks_at: String,
    },
    ChannelPredictionLock {
        outcomes: Vec<Outcome>,
    },
    ChannelPredictionEnd {
        winning_id: String,
        status: String,
        outcomes: Vec<Outcome>,
    },
}

//
pub enum SubEvent {
    ChannelPredictionBegin,
    ChannelPredictionLock,
    ChannelPredictionEnd,
}

impl TryFrom<&str> for SubEvent {
    type Error = EventsubClientError;

    fn try_from(value: &str) -> Result<Self, EventsubClientError> {
        match value {
            "channel.prediction.begin" => Ok(Self::ChannelPredictionBegin),
            "channel.prediction.lock" => Ok(Self::ChannelPredictionLock),
            "channel.prediction.end" => Ok(Self::ChannelPredictionEnd),
            _ => Err(EventsubClientError::EventTypeNotSupported),
        }
    }
}

impl SubEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChannelPredictionBegin => "channel.prediction.begin",
            Self::ChannelPredictionLock => "channel.prediction.lock",
            Self::ChannelPredictionEnd => "channel.prediction.end",
        }
    }
}

// https://dev.twitch.tv/docs/api/reference/#create-eventsub-subscription
#[derive(Serialize)]
pub struct SubToEventData {
    #[serde(rename(serialize = "type"))]
    pub _type: String,
    pub version: String,
    pub condition: Value, 
    pub transport: Transport,
}



pub struct EventsubClient {
    pub sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    pub stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    sender: Sender<Result<WSAction, EventsubClientError>>,
}

impl EventsubClient {
    pub async fn create_and_try_connect(sender: Sender<Result<WSAction, EventsubClientError>>) -> Result<Self, EventsubClientError> {
        let request = EVENTSUB_URL.into_client_request()?;
        let (ws_stream, response) = tokio_tungstenite::connect_async(request).await?;
        let (sink, mut stream) = ws_stream.split();

        
        Ok(EventsubClient {
            sink,
            stream,
            sender,
        })
    }

    // TODO: change the Ok type to some Enum to do with message types
    pub async fn read_stream(&mut self) {
        // Iterate through websocket stream for messages

        while let Some(next) = self.stream.next().await {
            let _ = match next {
                Ok(msg) => {
                    let Ok(msg_str) = msg.to_text() else {
                        continue;
                    };
                    println!("DEBUGGING: {msg_str}");
                    let Ok(ws_response) = serde_json::from_str::<WSResponse>(msg_str) else {
                        if msg_str == "connection unused" {
                            self.sender.send(Err(EventsubClientError::ConnectionClosed)).await.ok();
                        }
                        println!("failed to parse response to WSResponse struct");
                        continue;
                    };
                    self.handle_ws_response(ws_response).await;
                    
                },
                Err(e) => {
                    // TODO: send error through channel
                    self.sender.send(Err(e.into())).await.ok();
                    continue;
                }
            };
        }
        println!("End reading stream");
    }

    async fn handle_ws_response(&mut self, ws_response: WSResponse) {
        let res = match ws_response.metadata.message_type.as_str().into() {
            WSMessageType::SessionWelcome => self.on_session_welcome(ws_response).await,
            WSMessageType::SessionKeepAlive => self.on_session_keepalive(),
            WSMessageType::Notification => self.on_notification(ws_response).await,
            WSMessageType::SessionReconnect => self.on_session_reconnect(),
            WSMessageType::Revocation => self.on_revocation(),
            WSMessageType::Other => {
                println!("ERROR");
                Ok(())
            },
        };

        if let Err(e) = res {
            println!("{:?}", e);
        }
    }

    fn handle_ws_error(&self, error: EventsubClientError) {
        match error {
            EventsubClientError::ConnectionClosed => println!("connection was closed"),
            e => println!("unknown error: {:?}", e),
        }
    }

    async fn on_session_welcome(&self, ws_response: WSResponse) -> Result<(), EventsubClientError> {
        println!("Session welcome received");

        let session_id = ws_response.payload.session.ok_or(EventsubClientError::PayloadSessionNotFound)?.id;
        self.sender.send(Ok(WSAction::SessionWelcome { session_id })).await.ok();

        Ok(())
    }

    fn on_session_keepalive(&mut self, ) -> Result<(), EventsubClientError> {
        println!("Session keepalive received");

        // log it and continue

        Ok(())
    }

    async fn on_notification(&mut self, ws_response: WSResponse) -> Result<(), EventsubClientError> {
        println!("Notification received");

        // Create different EventType enum struct based on event type returned
        let event_data = ws_response.payload.event.ok_or(EventsubClientError::ParameterNotFound("Parameter 'event' not found.".to_string()))?;
        let event_type = match ws_response.payload.subscription.ok_or(EventsubClientError::SubscriptionNotFound)?._type.as_str() {
            "channel.prediction.begin" => {
                let locks_at = event_data
                    .get("locks_at")
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'locks_at' was not found in event response.".to_string()))?
                    .as_str()
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'locks_at' is not of type string in event response.".to_string()))?
                    .to_string();

                EventType::ChannelPredictionBegin { locks_at }
            },
            "channel.prediction.lock" => {
                let outcomes_value = event_data
                    .get("outcomes")
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'outcomes' not found.".to_string()))?;
                let outcomes = serde_json::from_value::<Vec<Outcome>>(outcomes_value.to_owned()).map_err(|e| EventsubClientError::OutcomesParsingFail(e.to_string()))?;

                EventType::ChannelPredictionLock { outcomes }
            },
            "channel.prediction.end" => {
                let outcomes_value = event_data
                    .get("outcomes")
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'outcomes' not found.".to_string()))?;
                let outcomes = serde_json::from_value::<Vec<Outcome>>(outcomes_value.to_owned()).map_err(|e| EventsubClientError::OutcomesParsingFail(e.to_string()))?;
                let winning_id = event_data
                    .get("winning_outcome_id")
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'winning_outcome_id' not found.".to_string()))?
                    .as_str()
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'winning_outcome_id' is not of type string in event response.".to_string()))?
                    .to_string();
        
                let status = event_data
                    .get("status")
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'status' not found.".to_string()))?
                    .as_str()
                    .ok_or(EventsubClientError::ParameterNotFound("Parameter 'status' is not of type string in event response.".to_string()))?
                    .to_string();

                EventType::ChannelPredictionEnd { winning_id, status, outcomes }
            },
            _ => return Err(EventsubClientError::EventTypeNotSupported),
        };

        self.sender.send(Ok(WSAction::Notification { event_type: event_type })).await.ok();

        Ok(())
    }

    fn on_session_reconnect(&mut self, ) -> Result<(), EventsubClientError> {
        println!("Session reconnect received");

        // reconnect somehow

        Ok(())
    }

    fn on_revocation(&mut self, ) -> Result<(), EventsubClientError> {
        println!("Revocation received");

        // log the reason and try to fix it

        Ok(())
    }
}

fn convert_message_to_string(message: Message) -> String {
    message.to_text().unwrap_or("").to_string()
}

pub fn run_eventsub_client(sender: Sender<Result<WSAction, EventsubClientError>>) {
    spawn(async move{
        let eventsub_client_res = EventsubClient::create_and_try_connect(sender.clone()).await;
        let Ok(mut eventsub_client) = eventsub_client_res else {
            println!("Failed to create eventsub client.");
            sender.send(Err(EventsubClientError::ConnectionClosed)).await.ok();
            return;
        };

        println!("starting to read stream");

        eventsub_client.read_stream().await;

        println!("reading stream done");
    });
}


                // match chrono::DateTime::parse_from_str(format!("{date} +0000").as_str(), "%Y-%m-%dT%H:%M:%SZ %z") {
                //         Ok(datetime) => {
                //             let date_comp = date_component::calculate(&datetime.to_utc(), &Utc::now());

// Returns a list of strings to be send in chat
pub fn get_prediction_reminder_time(locks_at: String) -> Result<DateTime<FixedOffset>, chrono::ParseError>{
    // locks_at example: 2020-07-15T17:21:03.17106713Z
    match DateTime::parse_from_str(format!("{locks_at} +0000").as_str(), "%Y-%m-%dT%H:%M:%S.%fZ %z") {
        Ok(date) => return Ok(date - Duration::seconds(31)),
        Err(e) => return Err(e)
    };
}


#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};

    use super::*;

    #[test]
    fn test_get_prediction_reminder_time() {
        let locks_at = "2020-07-15T17:21:03.17106713Z".to_string();
        let res = get_prediction_reminder_time(locks_at);
        let naive_datetime = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2020, 07, 15).unwrap(),
            NaiveTime::from_hms_nano_opt(17, 20, 32, 17106713).unwrap(), //31 seconds earlier
        );

        assert_eq!(
            res.unwrap().to_utc(), 
            DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc)
        );

    }
}