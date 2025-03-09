use crate::{command::Command, message::User};

pub enum BotSignal {
    CreatePrediction {
        client_id: String,
        access_token: String,
        command: Command,
    },
}

pub enum TwitchApiSignal {
    Unauthorized {
        cmd: String,
        arguments: Vec<String>,
        requested_by: Option<User>,
        reason: String,
    },
    BadRequest (String),
    TooManyRequests,
    Unknown,

    PredictionCreated,

}