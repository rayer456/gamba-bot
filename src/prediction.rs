use std::{cmp::Ordering, default};

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

// TODO
// impl Ord for Predictor {
//     fn cmp(&self, other: &Predictor) -> Ordering {
//         // Assume predictor is a winner and try to sort on won channel points
//         if let Some(a_channel_points_won) = self.channel_points_won {
//             if let Some(b_channel_points_won) = other.channel_points_won {
//                 // only return if both a and b have channel_points_won as Some
//                 ()
//             }
//             // sort on channel_points_used if b_won is None
//         }

//         // 
//     }
// }

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

pub fn get_prediction_end_vars(winning_id: String, outcomes: Vec<Outcome>) -> Result<(String, String)> {
    let mut winning_outcome: Option<Outcome> = None;
    let mut all_losers_vec: Vec<Vec<Predictor>> = vec![];
    for outcome in outcomes {
        if outcome.id == winning_id {
            winning_outcome = Some(outcome);
            continue;
        }

        let Some(top_predictors) = outcome.top_predictors else { bail!("ERROR: top predictors not found") };
        all_losers_vec.push(top_predictors);
    }

    let Some(winning_outcome) = winning_outcome else { bail!("ERROR: outcome with winning_id was not found") };
    let Some(mut top_predictors) = winning_outcome.top_predictors else { bail!("ERROR: top predictors not found") };

    // Sort winners just in case it doesn't do it automatically
    // TODO: rewrite this ordering
    top_predictors.sort_by(|a, b| {
        match a.channel_points_won.unwrap_or_else(|| 0).cmp(&b.channel_points_won.unwrap_or_else(|| 0)).reverse() {
            Ordering::Equal => a.user_name.cmp(&b.user_name),
            other => other,
        }
    });
    let top_predictors_vec = top_predictors.iter()
        .take(10) // shouldn't be more than 10 anyway
        .map(|w| format!("{} (+{})", w.user_name, w.channel_points_won.unwrap_or_else(|| 0)))
        .collect::<Vec<String>>();
    let top_predictors_str = top_predictors_vec.join(", ");

    let mut all_losers_flattened_vec = all_losers_vec.into_iter()
        .flatten()
        // .map(|l| format!("{}, {}", l.user_name, l.channel_points_used))
        .collect::<Vec<Predictor>>();

    // TODO: rewrite this ordering
    all_losers_flattened_vec.sort_by(|a, b| {
        match a.channel_points_used.cmp(&b.channel_points_used).reverse() {
            Ordering::Equal => a.user_name.cmp(&b.user_name),
            other => other,
        }
    });
    let top10_losers_vec = all_losers_flattened_vec.iter()
        .take(10)
        .map(|l| format!("{} (-{})", l.user_name, l.channel_points_used))
        .collect::<Vec<String>>();

    let top_losers_str = top10_losers_vec.join(", ");


    Ok((top_predictors_str, top_losers_str))
}


#[cfg(test)]
mod tests {
    use std::vec;

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
    fn test_get_prediction_end_vars() {
        // WINNING PREDICTORS
        let mut predictor_win2 = Predictor::default();
        predictor_win2.user_name = "xavier".to_string();
        predictor_win2.channel_points_used = 1000;
        predictor_win2.channel_points_won = Some(1950);

        let mut predictor_win1 = Predictor::default();
        predictor_win1.user_name = "rayer".to_string();
        predictor_win1.channel_points_used = 500;
        predictor_win1.channel_points_won = Some(1000);

        let mut predictor_win3 = Predictor::default();
        predictor_win3.user_name = "pisker".to_string();
        predictor_win3.channel_points_used = 2500;
        predictor_win3.channel_points_won = Some(3669);

        let predictors_win = vec![
            predictor_win1,
            predictor_win2,
            predictor_win3,
        ];

        // WINNING OUTCOME
        let mut outcome_win = Outcome::default();
        outcome_win.id = "winning_id123".to_string();
        outcome_win.top_predictors = Some(predictors_win);

        // LOSING PREDICTORS LOSING OUTCOME 1
        let mut predictor_lose1 = Predictor::default();
        predictor_lose1.user_name = "john".to_string();
        predictor_lose1.channel_points_used = 500;

        let mut predictor_lose2 = Predictor::default();
        predictor_lose2.user_name = "tim".to_string();
        predictor_lose2.channel_points_used = 1000;

        let mut predictor_lose3 = Predictor::default();
        predictor_lose3.user_name = "chiggs".to_string();
        predictor_lose3.channel_points_used = 2500;

        let mut predictor_lose4 = Predictor::default();
        predictor_lose4.user_name = "Trevis".to_string();
        predictor_lose4.channel_points_used = 5000;

        let mut predictor_lose5 = Predictor::default();
        predictor_lose5.user_name = "Franco".to_string();
        predictor_lose5.channel_points_used = 80;
        
        let predictors_lose1 = vec![
            predictor_lose1,
            predictor_lose2,
            predictor_lose3,
            predictor_lose4,
            predictor_lose5,
        ];

        // LOSING OUTCOME 1
        let mut outcome_lose1 = Outcome::default();
        outcome_lose1.id = "lolol123".to_string();
        outcome_lose1.top_predictors = Some(predictors_lose1);


        // LOSING PREDICTORS LOSING OUTCOME 2
        let mut predictor_lose1 = Predictor::default();
        predictor_lose1.user_name = "char1".to_string();
        predictor_lose1.channel_points_used = 3333;

        let mut predictor_lose2 = Predictor::default();
        predictor_lose2.user_name = "kush".to_string();
        predictor_lose2.channel_points_used = 1750;

        let mut predictor_lose3 = Predictor::default();
        predictor_lose3.user_name = "dev1".to_string();
        predictor_lose3.channel_points_used = 50;

        let mut predictor_lose4 = Predictor::default();
        predictor_lose4.user_name = "lamar".to_string();
        predictor_lose4.channel_points_used = 400;

        let mut predictor_lose5 = Predictor::default();
        predictor_lose5.user_name = "uglymf".to_string();
        predictor_lose5.channel_points_used = 95;
        
        let predictors_lose2 = vec![
            predictor_lose1,
            predictor_lose2,
            predictor_lose3,
            predictor_lose4,
            predictor_lose5,
        ];

        // LOSING OUTCOME 2
        let mut outcome_lose2 = Outcome::default();
        outcome_lose2.id = "lolol123".to_string();
        outcome_lose2.top_predictors = Some(predictors_lose2);

        // LOSING PREDICTORS LOSING OUTCOME 3
        let mut predictor_lose1 = Predictor::default();
        predictor_lose1.user_name = "apple".to_string();
        predictor_lose1.channel_points_used = 1999;

        let mut predictor_lose2 = Predictor::default();
        predictor_lose2.user_name = "bear".to_string();
        predictor_lose2.channel_points_used = 10008;

        let mut predictor_lose3 = Predictor::default();
        predictor_lose3.user_name = "witch".to_string();
        predictor_lose3.channel_points_used = 1222;

        let mut predictor_lose4 = Predictor::default();
        predictor_lose4.user_name = "cherrypicker".to_string();
        predictor_lose4.channel_points_used = 5;

        let mut predictor_lose5 = Predictor::default();
        predictor_lose5.user_name = "dryice456".to_string();
        predictor_lose5.channel_points_used = 80;
        
        let predictors_lose3 = vec![
            predictor_lose1,
            predictor_lose2,
            predictor_lose3,
            predictor_lose4,
            predictor_lose5,
        ];

        // LOSING OUTCOME 3
        let mut outcome_lose3 = Outcome::default();
        outcome_lose3.id = "lolol123".to_string();
        outcome_lose3.top_predictors = Some(predictors_lose3);

        // ALL OUTCOMES
        let outcomes = vec![
            outcome_win,
            outcome_lose1,
            outcome_lose2,
            outcome_lose3,
        ];

        if let Ok((top_predictors_str, top_losers_str)) = get_prediction_end_vars("winning_id123".to_string(), outcomes) {
            assert_eq!(top_predictors_str, "pisker (+3669), xavier (+1950), rayer (+1000)");
            assert_eq!(top_losers_str, "bear (-10008), Trevis (-5000), char1 (-3333), chiggs (-2500), apple (-1999), kush (-1750), witch (-1222), tim (-1000), john (-500), lamar (-400)")
        }
    }
}