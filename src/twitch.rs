use std::{fs, time::Duration};

use reqwest::{header::AUTHORIZATION, Client};
use tokio::{spawn, sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender}};

use crate::{command::Command, prediction::{self, Prediction}, signal::{BotSignal, TwitchApiSignal}};

const PREDICTIONS_URL: &'static str = "https://api.twitch.tv/helix/predictions";

pub struct TwitchApiClient {
    client: Client,
    rx_from_bot: TokioReceiver<BotSignal>,
    tx_to_bot: TokioSender<TwitchApiSignal>,

}

impl TwitchApiClient {
    pub fn new(rx_from_bot: TokioReceiver<BotSignal>, tx_to_bot: TokioSender<TwitchApiSignal>) -> Self {
        TwitchApiClient {
            client: Client::new(),
            rx_from_bot,
            tx_to_bot,

        }
    }

    async fn read_channels(&mut self) {
        if let Ok(signal) = self.rx_from_bot.try_recv() {
            match signal {
                BotSignal::CreatePrediction { client_id, access_token, command, prediction} => self.create_prediction(client_id, access_token, command, prediction),
            };
        }
    }

    fn create_prediction(&mut self, client_id: String, access_token: String, command: Command, prediction: Prediction) {
        let client = self.client.clone();
        let tx_to_bot_c = self.tx_to_bot.clone();
        spawn(create_prediction(client, client_id, access_token, tx_to_bot_c, command, prediction));
    }

}

pub async fn main_loop(mut twitch_api_client: TwitchApiClient) {
    loop {
        twitch_api_client.read_channels().await;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

pub async fn create_prediction(
    api_client: Client, 
    client_id: String, 
    access_token: String, 
    tx_to_bot: TokioSender<TwitchApiSignal>,
    command: Command,
    mut prediction: Prediction) {


    prediction.data_for_twitch.broadcaster_id = "105842308".to_string();

    // TODO: Think of making simple response struct with basic shit like status text wrapped in a Result
    let response = api_client
        .post(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .header("client-id", &client_id)
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
                command: command,
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

