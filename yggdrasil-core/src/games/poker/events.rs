use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TableEvent {
    Seated {
        table_id: String,
        user_id: String,
        seat: usize,
    },
    HandStarted {
        table_id: String,
        player_count: usize,
    },
    ActionTaken {
        table_id: String,
        user_id: String,
        action: String,
    },
    HandEnded {
        table_id: String,
        winner_message: Option<String>,
    },
}
