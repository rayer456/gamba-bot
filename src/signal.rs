use crate::{command::Command, message::User, prediction::Prediction};

pub enum BotSignal {
    CreatePrediction {
        client_id: String,
        access_token: String,
        command: Command,
        prediction: Prediction,
    },
}

pub enum TwitchApiSignal {
    Unauthorized {
        command: Command,
        reason: String,
    },
    BadRequest (String),
    TooManyRequests,
    Unknown {
        status: u16,
        text: String,
    },

    PredictionCreated,

}