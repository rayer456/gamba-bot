use std::{collections::HashMap, fs, ops::Deref, time::Duration};

use anyhow::{bail, Result};
use reqwest::{header::{AUTHORIZATION, CONTENT_TYPE}, Client};
use serde_json::Value;
use tokio::{spawn, sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender}};

use crate::{command::Command, prediction::{self, EndPredictionData, Prediction, PredictionFromTwitch}, signal::{BotSignal, TwitchApiSignal}};
use crate::signal::PredictionStatus;

const PREDICTIONS_URL: &'static str = "https://api.twitch.tv/helix/predictions";

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

pub struct TwitchApiClient {
    client: Client,
    rx_from_bot: TokioReceiver<BotSignal>, // TODO: can probably remove this receiver, nothing to receive
    tx_to_bot: TokioSender<TwitchApiSignal>, // send to async task

}

impl TwitchApiClient {
    pub fn new(rx_from_bot: TokioReceiver<BotSignal>, tx_to_bot: TokioSender<TwitchApiSignal>) -> Self {
        TwitchApiClient {
            client: Client::new(),
            rx_from_bot,
            tx_to_bot,

        }
    }

    // // TODO: signal could go away? Just call the desired method directly from bot
    // pub fn send_signal(&mut self, bot_signal: BotSignal) {
    //     match signal {
    //         BotSignal::CreatePrediction { common_paras, command, prediction} => self.create_prediction(common_paras, command, prediction),
    //         BotSignal::EndPrediction { common_paras, command, status, id } => {
    //             self.lock_prediction(common_paras, command);
    //         }
    //     };
    // }

    pub fn create_prediction(&mut self, common_paras: TwitchCommonParameters, command: Command, prediction: Prediction) {
        let client = self.client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        spawn(create_prediction(client, common_paras, tx_to_bot_c, command, prediction));
    }

    pub fn end_prediction(&mut self, common_paras: TwitchCommonParameters, command: Command, id: String, status: PredictionStatus) {
        let client = self.client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();

        let winning_outcome_id = match &status {
            PredictionStatus::Resolved { winning_outcome_id } => winning_outcome_id.clone(),
            _ => None,
        };

        let data = EndPredictionData {
            broadcaster_id: common_paras.broadcaster_id.clone(),
            id,
            status: status.into(),
            winning_outcome_id,
        };

        spawn(end_prediction(client, common_paras, tx_to_bot_c, command, data));
    }

    pub async fn get_latest_prediction(&mut self, common_paras: TwitchCommonParameters, command: Command) -> Result<PredictionFromTwitch> {
        let client = self.client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        
        let handle = spawn(get_latest_prediction(client, common_paras, tx_to_bot_c, command));
        let res = handle.await?;


        res
    }

}

pub async fn get_latest_prediction(
    api_client: Client, 
    common_paras: TwitchCommonParameters,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    command: Command) -> Result<PredictionFromTwitch> {


    // TODO: Think of making simple response struct with basic shit like status text wrapped in a Result
    let mut params = HashMap::new();
    params.insert("broadcaster_id", common_paras.broadcaster_id.as_str());
    params.insert("first", "1");
    let response = api_client
        .get(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .query(&params)
        .send().await;

    let res = response.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    
    match status {
        400 => {
            println!("400: Failed to get prediction: {text}");
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
    data: EndPredictionData) {

    // let ser = &serde_json::to_string(&data).unwrap();
    // println!("{ser}");

    
    // TODO: Think of making simple response struct with basic shit like status text wrapped in a Result
    let response = api_client
        .patch(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .json(&data)
        .send().await;

    let res = response.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    
    match status {
        400 => {
            println!("400: Failed to lock prediction: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::BadRequest(text)).await;
        }
        401 => {
            println!("401: Failed to lock prediction: {text}");
            let _ = tx_to_bot.send(TwitchApiSignal::Unauthorized {
                command,
                reason: text,
            }).await;
        }
        200 => {
            println!("Locked prediction successfully");
            let _ = tx_to_bot.send(TwitchApiSignal::PredictionLocked).await;
        }
        429 => drop(tx_to_bot.send(TwitchApiSignal::TooManyRequests).await),
        _ => drop(tx_to_bot.send(TwitchApiSignal::Unknown {
            status,
            text,
        }).await),
    };
}

pub async fn create_prediction(
    api_client: Client, 
    common_paras: TwitchCommonParameters,
    tx_to_bot: TokioSender<TwitchApiSignal>,
    command: Command,
    mut prediction: Prediction) {

    prediction.data_for_twitch.broadcaster_id = common_paras.broadcaster_id;

    // TODO: Think of making simple response struct with basic shit like status text wrapped in a Result
    let response = api_client
        .post(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", common_paras.access_token))
        .header("client-id", common_paras.client_id)
        .json(&prediction.data_for_twitch)
        .send().await;

    let res = response.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    
    match status {
        400 => {
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
}

