use anyhow::{bail, Result};
use serde::{de, Deserialize, Serialize};


#[derive(PartialEq)]
pub enum PredictionCommandVariant { // kinda shit name
    Start,
    Lock,
    Outcome,
    Cancel,
    Invalid,
}

impl From<&str> for PredictionCommandVariant {
    fn from(pred_variant: &str) -> Self {
        match pred_variant.to_uppercase().as_str() {
            "START" => PredictionCommandVariant::Start,
            "LOCK" => PredictionCommandVariant::Lock,
            "OUTCOME" => PredictionCommandVariant::Outcome,
            "CANCEL" => PredictionCommandVariant::Cancel,
            _ => {
                // log this as warning
                return PredictionCommandVariant::Invalid
            },
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct PredictionResponse {
    
}

#[derive(Serialize, Clone, Debug)]
pub struct EndPredictionData {
    pub broadcaster_id: String,
    pub id: String,
    pub status: String,
    
    #[serde(skip_serializing_if = "outcome_id_not_exists")]
    pub winning_outcome_id: Option<String>
}

fn outcome_id_not_exists(id: &Option<String>) -> bool {
    id.is_none()
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Prediction {
    pub name: String,
    pub auto_start: bool,
    pub data_for_twitch: CreatePredictionData,
    pub split_name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CreatePredictionData {
    pub title: String,
    pub outcomes: Vec<Outcome>,
    pub prediction_window: u16,
    pub broadcaster_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub enum PredictionStatus {
    Active,
    Locked,
    Canceled,
    Resolved {
        winning_outcome_id: Option<String>
    },
}

pub struct PredictionFromES {
    pub _type: String,
    pub status: String,
    pub outcomes: Vec<Outcome>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Outcome {
    pub title: String, // only serialize this field when sending to API / saving to file

    // Deserialize from API (filled in) or from a File / Eventsub (default values)
    #[serde(skip_serializing, default)]
    pub id: String,

    #[serde(skip_serializing, default)]
    pub users: u32,

    #[serde(skip_serializing, default)]
    pub channel_points: u32,

    #[serde(skip_serializing, default)]
    pub top_predictors: Option<Vec<Predictor>>, // TODO: check Option value when resolving, but no winners

    #[serde(skip_serializing, default)]
    pub color: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Predictor {
    pub user_id: String,
    pub user_name: String,
    pub user_login: String,
    pub channel_points_used: u32,
    pub channel_points_won: u32 // TODO: Can be null according to Twitch docs -> test this then make Option if fail
}

#[derive(Deserialize, Clone, Debug)]
pub struct PredictionFromTwitch {
    pub id: String,
    pub broadcaster_id: String,
    pub broadcaster_name: String,
    pub broadcaster_login: String,
    pub title: String,
    pub winning_outcome_id: Option<String>, // only when RESOLVED
    pub outcomes: Vec<Outcome>,
    pub prediction_window: u16,

    #[serde(deserialize_with = "status_deserializer")]
    pub status: PredictionStatus,

    // If Some, convert to some date type
    pub created_at: String,
    pub ended_at: Option<String>,
    pub locked_at: Option<String>,
}

fn status_deserializer<'de, D>(input: D) -> Result<PredictionStatus, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(input)?;
    let p: PredictionStatus = s.into();

    Ok(p)
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