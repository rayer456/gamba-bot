use anyhow::{bail, Result};
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

pub async fn get_predictions() -> Result<Vec<Prediction>> {
    let predictions_str = match std::fs::read_to_string("predictions.json") {
        Ok(p) => p,
        Err(e) => bail!("{e}:\nFile predictions.json not found, can't continue."),
    };

    let predictions: Vec<Prediction> = match serde_json::from_str(&predictions_str) {
        Ok(cmds) => cmds,
        Err(e) => bail!("Syntax of defined predictions in predictions.json is wrong.\nIn specific: {e}"),
    };

    Ok(predictions)
}

pub fn find_prediction_by_name<'a>(predictions: &'a Vec<Prediction>, name: &'a str) -> Option<&'a Prediction> {
    let prediction = predictions
        .iter()
        .find(|p| &*p.name == name);

    if prediction.is_none() {
        println!("WARNING: didn't find requested prediction '{name}' in list of loaded predictions.");
    }

    prediction
}

pub fn prediction_name_exists(predictions: &Vec<Prediction>, name: &str) -> bool {
    predictions
        .iter()
        .any(|pred| pred.name == name)
}

pub fn get_defined_predictions_as_str(predictions: &Vec<Prediction>) -> String {
    predictions
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}