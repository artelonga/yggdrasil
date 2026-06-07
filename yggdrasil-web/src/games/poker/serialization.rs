pub fn waiting_state() -> serde_json::Value {
    serde_json::json!({
        "game_over": false,
        "round": "Aguardando",
        "community_cards": [],
        "pot": 0,
        "current_bet": 0,
        "current_actor": null,
        "players": [],
        "winner_message": null
    })
}
