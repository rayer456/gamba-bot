use crate::{command::Command, message::User, prediction::Prediction, twitch::TwitchCommonParameters};

pub enum BotSignal {
    CreatePrediction {
        common_paras: TwitchCommonParameters,
        command: Command,
        prediction: Prediction,
    },
    EndPrediction {
        common_paras: TwitchCommonParameters,
        command: Command,
        status: PredictionStatus,
        id: String,
    }
}

pub enum PredictionStatus {
    Resolved {
        winning_outcome_id: String
    },
    Canceled,
    Locked,
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