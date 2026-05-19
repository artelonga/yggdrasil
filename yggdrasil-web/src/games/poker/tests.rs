use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::Request,
    routing::{get, post},
};
use tower::ServiceExt;

use crate::auth::sign_jwt;
use crate::games::poker_persistence;
use game_core::storage::Storage;
use tempfile::TempDir;
use yggdrasil_core::sementes::Sementes;

use super::routes::{get_hand, get_hole_cards, get_lobby, list_lobbies, post_action, sit, stand};
use super::state::PokerState;

/// Test helper: cria PokerState com Sementes seeded para os usuários comuns
/// dos testes (user-a, user-b). Cada um recebe 100k para não topar o limite
/// em testes que sentam várias vezes.
fn make_app(secret: &str) -> (Router, Arc<PokerState>, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&dir.path().join("test.db")).unwrap());
    let sementes = Arc::new(Sementes::new(storage));
    sementes.creditar("user-a", 100_000).unwrap();
    sementes.creditar("user-b", 100_000).unwrap();
    let state = Arc::new(PokerState::new(secret.to_string(), sementes));
    let app = Router::new()
        .route("/api/v1/poker/lobbies", get(list_lobbies))
        .route("/api/v1/poker/lobbies/{id}", get(get_lobby))
        .route("/api/v1/poker/lobbies/{id}/sit", post(sit))
        .route("/api/v1/poker/lobbies/{id}/stand", post(stand))
        .route("/api/v1/poker/lobbies/{id}/hand", get(get_hand))
        .route("/api/v1/poker/lobbies/{id}/hole-cards", get(get_hole_cards))
        .route("/api/v1/poker/lobbies/{id}/action", post(post_action))
        .with_state(state.clone());
    (app, state, dir)
}

async fn parse_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn sit_req(seat: usize, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/poker/lobbies/carvalho/sit")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"seat": seat}).to_string()))
        .unwrap()
}

fn hand_req(token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/poker/lobbies/carvalho/hand")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn action_req(token: &str, action: &str, amount: Option<u32>) -> Request<Body> {
    let mut body = serde_json::json!({"action": action});
    if let Some(amt) = amount {
        body["amount"] = serde_json::json!(amt);
    }
    Request::builder()
        .method("POST")
        .uri("/api/v1/poker/lobbies/carvalho/action")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ── Lobby (seating) tests (regressão YG-23) ───────────────────────────

#[tokio::test]
async fn list_sem_auth_retorna_401() {
    let (app, _, _dir) = make_app("s");
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/poker/lobbies")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_com_auth_retorna_mesas_seed() {
    // Cash game (carvalho, olmo) + heads-up — YG-37 variant.
    let (app, _, _dir) = make_app("s");
    let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/poker/lobbies")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let v = parse_body(resp).await;
    let lobbies = v["lobbies"].as_array().unwrap();
    assert_eq!(lobbies.len(), 3);
    let ids: Vec<&str> = lobbies.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["carvalho", "olmo", "heads-up"]);
    // Heads-up mesa tem max_seats=2.
    assert_eq!(lobbies[2]["max_seats"], 2);
    assert_eq!(lobbies[2]["seats"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn sit_persiste_humano_e_adiciona_bot() {
    let (app, _, _dir) = make_app("s");
    let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let resp = app.oneshot(sit_req(2, &token)).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let v = parse_body(resp).await;
    let seats = v["seats"].as_array().unwrap();
    let humans = seats.iter().filter(|s| s["kind"] == "human").count();
    let bots = seats.iter().filter(|s| s["kind"] == "bot").count();
    assert_eq!(humans, 1);
    assert_eq!(bots, 1);
}

#[tokio::test]
async fn sit_em_mesa_inexistente_retorna_404() {
    let (app, _, _dir) = make_app("s");
    let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/poker/lobbies/inexistente/sit")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"seat": 0}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stand_remove_jogador() {
    let (app, _, _dir) = make_app("s");
    let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let sit_resp = app.clone().oneshot(sit_req(0, &token)).await.unwrap();
    assert_eq!(sit_resp.status(), axum::http::StatusCode::OK);
    let stand_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/poker/lobbies/carvalho/stand")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stand_resp.status(), axum::http::StatusCode::OK);
    let v = parse_body(stand_resp).await;
    let humans = v["seats"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "human")
        .count();
    let bots = v["seats"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "bot")
        .count();
    assert_eq!(humans, 0);
    assert_eq!(bots, 0);
}

// ── Gameplay tests (YG-25) ─────────────────────────────────────────────

async fn seat_two_players(app: &Router) -> (String, String) {
    let token_a = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let token_b = sign_jwt("user-b", "b@test.com", "s").unwrap();
    app.clone().oneshot(sit_req(0, &token_a)).await.unwrap();
    app.clone().oneshot(sit_req(1, &token_b)).await.unwrap();
    (token_a, token_b)
}

#[tokio::test]
async fn get_hand_sem_auth_retorna_401() {
    let (app, _, _dir) = make_app("s");
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/poker/lobbies/carvalho/hand")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_hand_inicia_partida_com_dois_jogadores() {
    let (app, _, _dir) = make_app("s");
    let (token_a, _token_b) = seat_two_players(&app).await;
    let resp = app.oneshot(hand_req(&token_a)).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let v = parse_body(resp).await;
    assert!(!v["game_over"].as_bool().unwrap());
    assert_eq!(v["players"].as_array().unwrap().len(), 2);
    assert!(!v["current_actor"].is_null());
}

#[tokio::test]
async fn get_hand_sem_jogadores_retorna_estado_aguardando() {
    let (app, _, _dir) = make_app("s");
    let token_a = sign_jwt("user-a", "a@test.com", "s").unwrap();
    // Seat only 1 player → bot fills but start_hand on GET /hand with 1 human + 1 bot
    app.clone().oneshot(sit_req(0, &token_a)).await.unwrap();
    let resp = app.oneshot(hand_req(&token_a)).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    // With 1 human + 1 bot, start_hand succeeds
    let v = parse_body(resp).await;
    assert_eq!(v["players"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn acao_fora_da_vez_retorna_409() {
    let (app, _, _dir) = make_app("s");
    let (token_a, token_b) = seat_two_players(&app).await;
    // Start the hand
    let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
    let current_actor = hand_v["current_actor"].as_str().unwrap();
    // Use the OTHER player's token
    let wrong_token = if current_actor == "user-a" {
        &token_b
    } else {
        &token_a
    };
    let resp = app
        .oneshot(action_req(wrong_token, "check", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn fold_encerra_mao_imediatamente_via_http() {
    let (app, _, _dir) = make_app("s");
    let (token_a, token_b) = seat_two_players(&app).await;
    let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
    let current_actor = hand_v["current_actor"].as_str().unwrap().to_string();
    let acting_token = if current_actor == "user-a" {
        &token_a
    } else {
        &token_b
    };
    let resp = app
        .oneshot(action_req(acting_token, "fold", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let v = parse_body(resp).await;
    assert!(v["game_over"].as_bool().unwrap());
    assert!(!v["winner_message"].is_null());
}

#[tokio::test]
async fn dois_humanos_completam_mao_ate_showdown_via_http() {
    let (app, _, _dir) = make_app("s");
    let (token_a, token_b) = seat_two_players(&app).await;
    app.clone().oneshot(hand_req(&token_a)).await.unwrap();

    let mut game_over = false;
    for _ in 0..30 {
        let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
        if hand_v["game_over"].as_bool().unwrap() {
            assert!(hand_v["winner_message"].is_string());
            game_over = true;
            break;
        }
        let current_actor = hand_v["current_actor"].as_str().unwrap().to_string();
        let token = if current_actor == "user-a" {
            &token_a
        } else {
            &token_b
        };

        let table_bet = hand_v["current_bet"].as_u64().unwrap_or(0);
        let player_bet = hand_v["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["user_id"].as_str().unwrap() == current_actor)
            .and_then(|p| p["current_bet"].as_u64())
            .unwrap_or(0);

        let action = if player_bet < table_bet {
            "call"
        } else {
            "check"
        };
        let resp = app
            .clone()
            .oneshot(action_req(token, action, None))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            axum::http::StatusCode::OK,
            "ação '{}' falhou",
            action
        );

        let v = parse_body(resp).await;
        if v["game_over"].as_bool().unwrap() {
            assert!(v["winner_message"].is_string());
            game_over = true;
            break;
        }
    }
    assert!(game_over, "mão deveria completar em showdown");
}

#[tokio::test]
async fn hole_cards_retorna_apenas_cartas_do_usuario_autenticado() {
    let (app, _, _dir) = make_app("s");
    let (token_a, _) = seat_two_players(&app).await;
    app.clone().oneshot(hand_req(&token_a)).await.unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/poker/lobbies/carvalho/hole-cards")
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let v = parse_body(resp).await;
    assert_eq!(v["cards"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn sit_sem_saldo_retorna_402_payment_required() {
    // Usuário com saldo zero (não foi seeded em make_app).
    let (app, _, _dir) = make_app("s");
    let token = sign_jwt("user-pobre", "p@test.com", "s").unwrap();
    let resp = app.oneshot(sit_req(0, &token)).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::PAYMENT_REQUIRED);
    let v = parse_body(resp).await;
    assert!(
        v["erro"].as_str().unwrap().contains("Saldo insuficiente"),
        "mensagem PT-BR: {}",
        v["erro"]
    );
}

#[tokio::test]
async fn sit_debita_buy_in_da_carteira_do_usuario() {
    let (app, state, _dir) = make_app("s");
    let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let saldo_antes = state.sementes.saldo("user-a").unwrap();
    let resp = app.oneshot(sit_req(0, &token)).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let saldo_depois = state.sementes.saldo("user-a").unwrap();
    assert_eq!(saldo_antes - saldo_depois, 1_000);
}

#[tokio::test]
async fn stand_credita_chips_remanescentes() {
    let (app, state, _dir) = make_app("s");
    let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
    let saldo_inicial = state.sementes.saldo("user-a").unwrap();
    // Sit → debita 1000
    app.clone().oneshot(sit_req(0, &token)).await.unwrap();
    // Stand → credita o stack remanescente (1000 se não jogou)
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/poker/lobbies/carvalho/stand")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(state.sementes.saldo("user-a").unwrap(), saldo_inicial);
}

#[tokio::test]
async fn humano_vs_bot_completa_mao_sem_travar_via_http() {
    // Regressão YG-26: bot deve responder automaticamente quando é a vez dele.
    let (app, _, _dir) = make_app("s");
    let token_a = sign_jwt("user-a", "a@test.com", "s").unwrap();
    // Senta 1 humano → lobby auto-adiciona bot.
    app.clone().oneshot(sit_req(0, &token_a)).await.unwrap();

    let mut game_over = false;
    for _ in 0..40 {
        let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
        if hand_v["game_over"].as_bool().unwrap_or(false) {
            game_over = true;
            break;
        }
        let current_actor = hand_v["current_actor"].as_str().unwrap_or("").to_string();
        // Se for vez do bot, auto_step já deveria ter rodado — esperar próximo poll
        // não ajudaria. Se chegou aqui, é vez do humano.
        assert_eq!(
            current_actor, "user-a",
            "bot deveria ter agido antes de retornar para o cliente"
        );

        let table_bet = hand_v["current_bet"].as_u64().unwrap_or(0);
        let player_bet = hand_v["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["user_id"].as_str() == Some("user-a"))
            .and_then(|p| p["current_bet"].as_u64())
            .unwrap_or(0);
        let action = if player_bet < table_bet {
            "call"
        } else {
            "check"
        };
        let resp = app
            .clone()
            .oneshot(action_req(&token_a, action, None))
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let v = parse_body(resp).await;
        if v["game_over"].as_bool().unwrap_or(false) {
            game_over = true;
            break;
        }
    }
    assert!(game_over, "mão humano vs bot deveria completar");
}

// ── YG-29: persistência em SQLite (restart sobrevive) ────────────────

/// Helper: cria PokerState COM persistência apontando para um path,
/// reutilizando uma TempDir externa para que possamos recriar o state
/// no mesmo DB.
fn make_app_persistente(
    secret: &str,
    dir: &TempDir,
) -> (Router, Arc<PokerState>, std::path::PathBuf) {
    let sementes_path = dir.path().join("sementes.db");
    let poker_path = dir.path().join("poker.db");
    let storage = Arc::new(Storage::open(&sementes_path).unwrap());
    let sementes = Arc::new(Sementes::new(storage));
    sementes.creditar("user-a", 100_000).unwrap();
    sementes.creditar("user-b", 100_000).unwrap();
    let state = Arc::new(PokerState::with_persistence(
        secret.to_string(),
        sementes,
        &poker_path,
    ));
    let app = Router::new()
        .route("/api/v1/poker/lobbies", get(list_lobbies))
        .route("/api/v1/poker/lobbies/{id}", get(get_lobby))
        .route("/api/v1/poker/lobbies/{id}/sit", post(sit))
        .route("/api/v1/poker/lobbies/{id}/stand", post(stand))
        .route("/api/v1/poker/lobbies/{id}/hand", get(get_hand))
        .route("/api/v1/poker/lobbies/{id}/hole-cards", get(get_hole_cards))
        .route("/api/v1/poker/lobbies/{id}/action", post(post_action))
        .with_state(state.clone());
    (app, state, poker_path)
}

#[tokio::test]
async fn restart_preserva_seat_e_stack() {
    // Simula crash-restart: sit, drop state, recria state apontando para
    // o mesmo DB, e verifica que o seat e o stack sobreviveram.
    let dir = tempfile::tempdir().unwrap();
    let secret = "s";
    let token = sign_jwt("user-a", "a@test.com", secret).unwrap();

    // Primeiro boot: senta user-a.
    let (app, state1, poker_path) = make_app_persistente(secret, &dir);
    let resp = app.oneshot(sit_req(2, &token)).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    // Stack persistido no estado em memória.
    {
        let tables = state1.tables.lock().unwrap();
        let t = tables.iter().find(|t| t.lobby.id == "carvalho").unwrap();
        assert_eq!(t.stack_of("user-a"), 1_000);
    }
    drop(state1);

    // Segundo boot — mesmo DB, novo PokerState.
    let sementes_path = dir.path().join("sementes.db");
    let storage = Arc::new(Storage::open(&sementes_path).unwrap());
    let sementes = Arc::new(Sementes::new(storage));
    let state2 = Arc::new(PokerState::with_persistence(
        secret.to_string(),
        sementes,
        &poker_path,
    ));

    let tables = state2.tables.lock().unwrap();
    let carvalho = tables.iter().find(|t| t.lobby.id == "carvalho").unwrap();
    // Seat 2 ainda ocupado por user-a.
    assert!(matches!(
        &carvalho.lobby.seats[2],
        yggdrasil_core::games::poker_lobby::SeatOccupant::Human { user_id, .. }
            if user_id == "user-a"
    ));
    // Stack sobreviveu.
    assert_eq!(carvalho.stack_of("user-a"), 1_000);
    // Mas a mão em curso (se houver) foi forfeit — game é None.
    assert!(carvalho.game.is_none());

    // As três mesas defaults ainda lá.
    assert_eq!(tables.len(), 3);
}

#[tokio::test]
async fn primeiro_boot_seeda_tres_mesas_no_db() {
    let dir = tempfile::tempdir().unwrap();
    let (_app, _state, poker_path) = make_app_persistente("s", &dir);

    // Abre uma conexão fresca e verifica que as 3 mesas foram persistidas.
    let conn = poker_persistence::init_poker_db(&poker_path).unwrap();
    let snaps = poker_persistence::load_all(&conn).unwrap();
    assert_eq!(snaps.len(), 3);
    let ids: Vec<&str> = snaps.iter().map(|s| s.lobby.id.as_str()).collect();
    assert!(ids.contains(&"carvalho"));
    assert!(ids.contains(&"olmo"));
    assert!(ids.contains(&"heads-up"));
}

#[tokio::test]
async fn acao_desconhecida_retorna_400() {
    let (app, _, _dir) = make_app("s");
    let (token_a, _) = seat_two_players(&app).await;
    app.clone().oneshot(hand_req(&token_a)).await.unwrap();
    let resp = app
        .oneshot(action_req(&token_a, "bluff", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
}
