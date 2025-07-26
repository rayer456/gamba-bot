use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize, Debug)]
pub struct WSResponse {
    pub metadata: Metadata,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Metadata {
    pub message_id: String,
    pub message_type: String,
    pub message_timestamp: String,

    // Only for notification and revocation messages
    // Serde will default to None (hopefully)
    pub subscription_type: Option<String>,
    pub subscription_version: Option<String>,

}

#[derive(Clone, Deserialize, Debug)]
pub struct Payload {
    // Only for welcome message
    pub session: Option<Session>,

    // Only for notification message
    pub subscription: Option<Subscription>,
    pub event: Option<Value>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Session {
    pub id: String,
    pub status: String,
    pub keepalive_timeout_seconds: isize,
    pub reconnect_url: Option<String>,
    pub connected_at: String,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Subscription {
    pub id: String,
    pub status: String,
    #[serde(rename(deserialize = "type"))]
    pub _type: String,
    pub version: String,
    pub cost: i32,
    pub condition: Value, // TODO: work out further
    pub transport: Transport,
    pub created_at: String,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Transport {
    pub method: String,
    pub session_id: String,
}

// Close frames/messages??? defined in twitch docs