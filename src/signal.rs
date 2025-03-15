use serde::Deserialize;

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

// TODO: MOVE TO PREDICTION.RS
#[derive(Clone, Debug, Deserialize)]
pub enum PredictionStatus {
    Active,
    Locked,
    Canceled,
    Resolved {
        winning_outcome_id: Option<String>
    },
}


impl From<PredictionStatus> for String {
    fn from(value: PredictionStatus) -> Self {
        match value {
            PredictionStatus::Active => "ACTIVE".to_string(),
            PredictionStatus::Locked => "LOCKED".to_string(),
            PredictionStatus::Canceled => "CANCELED".to_string(),
            PredictionStatus::Resolved {..} => "RESOLVED".to_string(),
        }
    }
}

impl From<String> for PredictionStatus {
    fn from(value: String) -> Self {
        match value.to_uppercase().as_str() {
            "ACTIVE" => PredictionStatus::Active,
            "LOCKED" => PredictionStatus::Locked,
            "CANCELED" => PredictionStatus::Canceled,
            "RESOLVED" => PredictionStatus::Resolved { winning_outcome_id: None },
            _ => panic!("Twitch sent some wrong shit as predictionStatus"),
        }
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