use serde::{Deserialize, Serialize};

pub enum PredictionVariant { // PredictionCommandVariant
    Start,
    Lock,
    Outcome,
    Cancel,
    Invalid,
}

impl From<&str> for PredictionVariant {
    fn from(pred_variant: &str) -> Self {
        match pred_variant.to_uppercase().as_str() {
            "START" => PredictionVariant::Start,
            "LOCK" => PredictionVariant::Lock,
            "OUTCOME" => PredictionVariant::Outcome,
            "CANCEL" => PredictionVariant::Cancel,
            _ => {
                // log this
                return PredictionVariant::Invalid
            },
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Prediction {
    pub name: String,
    pub auto_start: bool,
    pub data_for_twitch: DataForTwitch,
    pub split_name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DataForTwitch {
    pub title: String,
    pub outcomes: Vec<Outcome>,
    pub prediction_window: u16,
    pub broadcaster_id: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Outcome {
    pub title: String
}