use std::{collections::HashMap, fs, ops::Deref, time::Duration, vec};

use anyhow::{bail, Ok, Result};
use reqwest::{header::{AUTHORIZATION, CONTENT_TYPE}, Client};
use serde::Serialize;
use serde_json::Value;
use tokio::{spawn, sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender}};

use crate::{command::Command, eventsub::{EventType, SubEvent, SubToEventData}, prediction::{self, EndPredictionData, Prediction, PredictionFromTwitch, PredictionStatus}, signal::{BotSignal, TwitchApiSignal}, websocket::Transport};

const PREDICTIONS_URL: &'static str = "https://api.twitch.tv/helix/predictions";
const EVENTSUB_SUBSCRIPTION_URL: &'static str = "https://api.twitch.tv/helix/eventsub/subscriptions";

#[derive(Clone, Debug)]
pub struct TwitchCommonParameters {
    pub client_id: String,
    pub access_token: String,
    pub broadcaster_id: String,
}

impl TwitchCommonParameters {
    pub fn new(client_id: String, access_token: String, broadcaster_id: String) -> Self {
        TwitchCommonParameters { 
            client_id,
            access_token, 
            broadcaster_id,
        }
    }
}


pub async fn get_latest_prediction(
    api_client: Client,
    common_paras: TwitchCommonParameters,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    command: Command) -> Result<PredictionFromTwitch> {

    let mut params = HashMap::new();
    params.insert("broadcaster_id", common_paras.broadcaster_id.as_str());
    params.insert("first", "1");
    let response = api_client
        .get(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .query(&params)
        .send().await?;

    let status = response.status().as_u16();
    let text = response.text().await?;

    match status {
        400 => {
            let _ = tx_to_bot.send(TwitchApiSignal::BadRequest(text)).await;
        }
        401 => {
            println!("401: Failed to create prediction: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::Unauthorized {
                command,
                reason: text,
            }).await;
        }
        200 => {
            println!("Retrieved prediction successfully");

            let text_as_value = serde_json::from_str::<Value>(&text)?;
            
            let Some(data) = text_as_value.get("data") else {
                bail!("ERROR: No 'data' field found in API response.");
            };

            let pred_objects = serde_json::from_value::<Vec<PredictionFromTwitch>>(data.clone())?;

            let Some(pred) = pred_objects.first() else {
                bail!("prediction list is empty");
            };


            return Ok(pred.to_owned());
            // let _ = tx_to_bot.send(TwitchApiSignal::GotLatestPrediction).await; // Just use serde Value
        }
        429 => drop(tx_to_bot.send(TwitchApiSignal::TooManyRequests).await),
        _ => drop(tx_to_bot.send(TwitchApiSignal::Unknown {
            status,
            text,
        }).await),
    };

    bail!("Failed request via latest_prediction(), consult other logs for reason");
}

pub async fn end_prediction(
    api_client: Client, 
    common_paras: TwitchCommonParameters,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    command: Command,
    data: EndPredictionData) -> Result<()> {

    let response = api_client
        .patch(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .json(&data)
        .send().await?;

    let status = response.status().as_u16();
    let text = response.text().await?;
    
    match status {
        400 => bail!("Failed to end prediction: {text}"),
        401 => {
            println!("401: Failed to end prediction: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::Unauthorized {
                command,
                reason: text,
            }).await;
        }
        200 => {
            println!("Ended prediction successfully");
            let _ = tx_to_bot.send(TwitchApiSignal::PredictionLocked).await;
        }
        429 => drop(tx_to_bot.send(TwitchApiSignal::TooManyRequests).await),
        _ => drop(tx_to_bot.send(TwitchApiSignal::Unknown {
            status,
            text,
        }).await),
    };

    Ok(())
}


pub async fn create_prediction(
    api_client: Client, 
    common_paras: TwitchCommonParameters,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    command: Command,
    mut prediction: Prediction) -> Result<()> {

    prediction.data_for_twitch.broadcaster_id = common_paras.broadcaster_id;

    let response = api_client
        .post(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .json(&prediction.data_for_twitch)
        .send().await?;

    let status = response.status().as_u16();
    let text = response.text().await?;
    
    match status {
        400 => {
            // Should only fail if automod block or prediction already active
            // TODO: use automod to warn user when they create predictions with automod held terms
            // https://dev.twitch.tv/docs/api/reference/#check-automod-status

            println!("400: Failed to create prediction: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::BadRequest(text)).await;
        }
        401 => {
            println!("401: Failed to create prediction: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::Unauthorized {
                command,
                reason: text,
            }).await;
        }
        200 => {
            println!("Created prediction successfully");
            let _ = tx_to_bot.send(TwitchApiSignal::PredictionCreated).await;
        }
        429 => drop(tx_to_bot.send(TwitchApiSignal::TooManyRequests).await),
        _ => drop(tx_to_bot.send(TwitchApiSignal::Unknown {
            status,
            text,
        }).await),
    };

    Ok(())
}

// curl -X POST 'https://api.twitch.tv/helix/eventsub/subscriptions' \
// -H 'Authorization: Bearer 2gbdx6oar67tqtcmt49t3wpcgycthx' \
// -H 'Client-Id: wbmytr93xzw8zbg0p1izqyzzc5mbiz' \
// -H 'Content-Type: application/json' \
// -d '{"
//     type": "user.update",
//     "version": "1",
//     "condition": {
//         "user_id": "1234"
//     },
//     "transport": {
//         "method": "websocket",
//         "session_id": "AQoQexAWVYKSTIu4ec_2VAxyuhAB"
//     }
// }'



pub async fn sub_to_event(
    api_client: Client,
    common_paras: TwitchCommonParameters,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    data: SubToEventData) -> Result<()> {

    let response = api_client
        .post(EVENTSUB_SUBSCRIPTION_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .header(CONTENT_TYPE, "application/json")
        .json(&data)
        .send().await?;

    let status = response.status().as_u16();
    let text = response.text().await?;
    
    match status {
        202 => {
            println!("Subbed to event: {}", data._type);
            let _ = tx_to_bot.send(TwitchApiSignal::PredictionCreated).await;
        }
        400 => {
            println!("400: sub_to_event: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::BadRequest(text)).await;
        }
        401 => {
            println!("401: sub_to_event: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::UnauthorizedNoCommand { reason: text }).await;
        }
        429 => drop(tx_to_bot.send(TwitchApiSignal::TooManyRequests).await),
        _ => drop(tx_to_bot.send(TwitchApiSignal::Unknown {
            status,
            text,
        }).await),
    };

    Ok(())
}
