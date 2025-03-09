pub enum BotSignal {
    CREATE_PREDICTION {
        client_id: String,
        access_token: String,
    },
}