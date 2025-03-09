use std::{fs, time::Duration};

use reqwest::{header::AUTHORIZATION, Client};
use serde_json::Value;
use tokio::{spawn, sync::mpsc::Receiver as TokioReceiver};

use crate::{prediction::{self, Prediction}, signal::BotSignal};

const PREDICTIONS_URL: &'static str = "https://api.twitch.tv/helix/predictions";

pub struct TwitchApiClient {
    client: Client,
    rx_from_bot: TokioReceiver<BotSignal>,

}

impl TwitchApiClient {
    pub fn new(rx_from_bot: TokioReceiver<BotSignal>) -> Self {
        TwitchApiClient {
            client: Client::new(),
            rx_from_bot,

        }
    }

    async fn read_channels(&mut self) {
        if let Ok(signal) = self.rx_from_bot.try_recv() {
            match signal {
                BotSignal::CREATE_PREDICTION { client_id, access_token } => self.create_prediction(client_id, access_token),
            };
        }
    }

    fn create_prediction(&mut self, client_id: String, access_token: String) {
        let client = self.client.clone();
        spawn(create_prediction(client, client_id, access_token));
    }

}

pub async fn main_loop(mut twitch_api_client: TwitchApiClient) {
    loop {
        twitch_api_client.read_channels().await;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

pub async fn create_prediction(api_client: Client, client_id: String, access_token: String) {
    let Ok(contents) = fs::read_to_string("predictions.json") else {
        println!("file not found");
        return;
    };

    tokio::time::sleep(Duration::from_secs(5)).await;

    let predictions: Vec<Prediction> = serde_json::from_str(&contents).unwrap();
    let mut chosen = predictions.iter().nth(0).unwrap().to_owned();
    chosen.data_for_twitch.broadcaster_id = "105842308".to_string();

    println!("{}", serde_json::to_string(&chosen).unwrap());

    let response = api_client
        .post(PREDICTIONS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .header("client-id", &client_id)
        .json(&chosen.data_for_twitch)
        .send().await;

    let res = response.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    
    match status {
        400 => {
            println!("400: Failed to create prediction: {text}");
        }
        401 => {
            // Send through tx to bot
            /* return Err(AppError::InvalidTokenError {
                cmd: "!clip".to_string(),
                arguments: vec![],
                requested_by: None,
            }) */

            println!("401: Failed to create prediction: {text}");
        }
        200 => {
            println!("Created prediction successfully");
        }
        other => /* return Err(AppError::OtherError(format!("{}: {}", other, res.text()?))) */(),
    };
}

