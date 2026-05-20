use serde::{Deserialize, Serialize};

/// Query string `?variant=<slug>` para rotas de início de jogo (YG-37).
/// Slug `None` ou desconhecido → root default.
#[derive(Deserialize, Default)]
pub struct VariantQuery {
    pub variant: Option<String>,
}

fn default_user_id() -> String {
    "anonymous".to_string()
}

#[derive(Deserialize)]
pub struct InputRequest {
    pub direction: String,
    #[serde(default = "default_user_id")]
    pub user_id: String,
}

#[derive(Serialize)]
pub struct StartResponse<S> {
    pub id: String,
    pub state: S,
    pub score: u32,
}

#[derive(Serialize)]
pub struct TickResponse<S> {
    pub action: String,
    pub state: S,
    pub score: u32,
}
