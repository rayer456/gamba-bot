use std::default;

use anyhow::{bail, Result};
use futures::stream::SplitStream;
use serde::{de, Deserialize, Serialize};


#[derive(PartialEq)]
pub enum PredictionCommandVariant {
    Start,
    Lock,
    Outcome,
    Cancel,
}

impl TryFrom<&str> for PredictionCommandVariant {
    type Error = String;
    fn try_from(input: &str) -> Result<Self, String> {
        match input {
            s if s.eq_ignore_ascii_case("start") => Ok(Self::Start),
            s if s.eq_ignore_ascii_case("lock") => Ok(Self::Lock),
            s if s.eq_ignore_ascii_case("outcome") => Ok(Self::Outcome),
            s if s.eq_ignore_ascii_case("cancel") => Ok(Self::Cancel),
            _ => Err("Invalid command variant.".to_string()),
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
    pub top_predictors: Option<Vec<Predictor>>, // TODO: Option probably isn't necessary

    #[serde(skip_serializing, default)]
    pub color: String,
}

impl Default for Outcome {
    fn default() -> Self {
        Outcome { 
            title: String::new(),
            id: String::new(),
            users: 0,
            channel_points: 0, 
            top_predictors: None, 
            color: String::new(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Predictor {
    pub user_id: String,
    pub user_name: String,
    pub user_login: String,
    pub channel_points_used: u32,
    pub channel_points_won: Option<u32>
}

impl Predictor {
    pub fn new_set_points(channel_points_used: u32, channel_points_won: Option<u32>) -> Self {
        let mut predictor = Self::default();
        predictor.channel_points_used = channel_points_used;
        predictor.channel_points_won = channel_points_won;

        predictor
    }
}

impl Default for Predictor {
    fn default() -> Self {
        Predictor { 
            user_id: String::new(),
            user_name: String::new(),
            user_login: String::new(),
            channel_points_used: 0,
            channel_points_won: None,
        }
    }
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

pub fn get_prediction_lock_vars_from_outcomes(outcomes: Vec<Outcome>) -> (String, u32, u32) {
    let mut total_points = 0;
    let mut total_users = 0;
    for outcome in &outcomes {
        total_points += outcome.channel_points;
        total_users += outcome.users;
    }

    // Avoid floating point math if no points bet
    if total_points == 0 {
        let split_str = vec!["0"; outcomes.len()].join("/");
        return (split_str, total_points, total_users);
    }

    let split_str = outcomes
        .iter()
        .map(|outcome| ((outcome.channel_points as f32 / total_points as f32) * 100.0).round().to_string())
        .collect::<Vec<_>>()
        .join("/");

    return (split_str, total_points, total_users)
}

pub fn get_prediction_end_vars(winning_id: String, status: String, outcomes: Vec<Outcome>) -> () {
    
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_prediction_lock_vars_from_outcomes() {
        let mut outcomes = vec![];
        
        let mut outcome = Outcome::default();
        outcome.users = 3;
        outcome.channel_points = 5211;

        let mut outcome_2 = Outcome::default();
        outcome_2.users = 5;
        outcome_2.channel_points = 3812;

        let mut outcome_3 = Outcome::default();
        outcome_3.users = 1;
        outcome_3.channel_points = 7080;

        outcomes.push(outcome.clone());
        outcomes.push(outcome_2.clone());
        outcomes.push(outcome_3.clone());

        // Test with 3 outcomes with points
        let (split_str, total_points, total_users) = get_prediction_lock_vars_from_outcomes(outcomes.clone());
        assert_eq!(split_str, "32/24/44");
        assert_eq!(total_points, 16103);
        assert_eq!(total_users, 9);

        // Test with 2 outcomes with points and 1 without points
        outcome_3.channel_points = 0;
        outcomes.remove(2);
        outcomes.push(outcome_3.clone());
        let (split_str, total_points, total_users) = get_prediction_lock_vars_from_outcomes(outcomes.clone());
        assert_eq!(split_str, "58/42/0");
        assert_eq!(total_points, 9023);
        assert_eq!(total_users, 9);

        // Test with no points bet, always possible, even if users did bet
        outcomes[0].channel_points = 0;
        outcomes[1].channel_points = 0;
        outcomes[2].channel_points = 0;
        let (split_str, total_points, total_users) = get_prediction_lock_vars_from_outcomes(outcomes.clone());
        assert_eq!(split_str, "0/0/0");
        assert_eq!(total_points, 0);
        assert_eq!(total_users, 9);
    }

    #[test]
    fn test_get_prediction_end_vars_from_outcomes() {

        let mut predictors_won: Vec<Predictor> = vec![];
        predictors_won.push(Predictor::new_set_points(432, None));

        let mut predictors_lost: Vec<Predictor> = vec![];

        todo!()
    }
}