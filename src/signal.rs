use crate::{command::Command, message::User, prediction::Prediction, twitch::TwitchCommonParameters};

pub enum BotSignal {
    CreatePrediction {
        common_paras: TwitchCommonParameters,
        command: Command,
        prediction: Prediction,
    },
    LockPrediction {
        common_paras: TwitchCommonParameters,
        command: Command,
    }
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
    PredictionLocked,
    GotLatestPrediction(),
}