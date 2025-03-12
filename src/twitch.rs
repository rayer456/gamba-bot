use std::{fs, time::Duration};

use reqwest::{header::AUTHORIZATION, Client};
use tokio::{spawn, sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender}};

use crate::{command::Command, prediction::{self, Prediction}, signal::{BotSignal, TwitchApiSignal}};

const PREDICTIONS_URL: &'static str = "https://api.twitch.tv/helix/predictions";

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

    // TODO: signal could go away? Just call the desired method directly from bot
    pub fn send_signal(&mut self, bot_signal: BotSignal) {
        match signal {
            BotSignal::CreatePrediction { common_paras, command, prediction} => self.create_prediction(common_paras, command, prediction),
            BotSignal::LockPrediction { common_paras, command } => self.lock_prediction(common_paras, command)
        };
    }

    fn create_prediction(&mut self, common_paras: TwitchCommonParameters, command: Command, prediction: Prediction) {
        let client = self.client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        spawn(create_prediction(client, common_paras, tx_to_bot_c, command, prediction));
    }

    fn lock_prediction(&mut self, common_paras: TwitchCommonParameters, command: Command) {
        let client = self.client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        spawn(lock_prediction(client, common_paras, tx_to_bot_c, command));
    }

}

pub async fn lock_prediction(common_paras: TwitchCommonParameters, command: Command) {
    // Implement locking function
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

