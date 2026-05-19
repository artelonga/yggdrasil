use axum::{Json, http::StatusCode, response::IntoResponse};
use yggdrasil_core::games::poker_game::{PokerSitError, PokerStandError, PokerTableError};
use yggdrasil_core::games::poker_lobby::LobbyError;
use yggdrasil_core::sementes::SementesError;

pub type Response = axum::response::Response;

pub fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"erro": "nao_autenticado"})),
    )
        .into_response()
}

pub fn lobby_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"erro": "mesa_nao_encontrada"})),
    )
        .into_response()
}

pub fn lobby_error(e: LobbyError) -> Response {
    let status = match e {
        LobbyError::InvalidSeat(_) => StatusCode::BAD_REQUEST,
        LobbyError::SeatTaken | LobbyError::AlreadySeated => StatusCode::CONFLICT,
        LobbyError::NotSeated => StatusCode::BAD_REQUEST,
    };
    (status, Json(serde_json::json!({"erro": e.to_string()}))).into_response()
}

pub fn sit_error(e: PokerSitError) -> Response {
    match e {
        PokerSitError::Lobby(le) => lobby_error(le),
        PokerSitError::Sementes(SementesError::SaldoInsuficiente) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(serde_json::json!({"erro": "Saldo insuficiente para sentar"})),
        )
            .into_response(),
        PokerSitError::Sementes(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"erro": e.to_string()})),
        )
            .into_response(),
    }
}

pub fn stand_error(e: PokerStandError) -> Response {
    match e {
        PokerStandError::Lobby(le) => lobby_error(le),
        PokerStandError::Sementes(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"erro": e.to_string()})),
        )
            .into_response(),
    }
}

pub fn table_error(e: PokerTableError) -> Response {
    let status = match e {
        PokerTableError::NaoEhSuaVez => StatusCode::CONFLICT,
        PokerTableError::AcaoInvalida => StatusCode::UNPROCESSABLE_ENTITY,
        PokerTableError::MesaSemJogadores => StatusCode::CONFLICT,
        PokerTableError::RoundEncerrado => StatusCode::CONFLICT,
    };
    (status, Json(serde_json::json!({"erro": e.to_string()}))).into_response()
}
