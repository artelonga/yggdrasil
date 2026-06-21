//! `POST /api/v1/comunicacao/motivos` — salvar motivo gravado pelo sequencer
//! espacial (YG-170). Escrita atômica (temp+rename). Memory-light: aceita só
//! `PackEntry` com `audio` de síntese (Synth/Sequence) — nenhum byte de áudio
//! gravado em disco. Rejeita `Speech` (modo de idioma, não-musical).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use nanoid::nanoid;
use yggdrasil_core::comunicacao::{EntryAudio, PackEntry};

use crate::auth::verify_jwt;

pub struct MotivosState {
    pub jwt_secret: String,
    /// Raiz do diretório de motivos — estrutura `{motivos_dir}/{user}/{id}.json`.
    pub motivos_dir: PathBuf,
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn err_json(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({ "erro": msg }))).into_response()
}

/// Escrita atômica: grava em `.tmp` e depois renomeia. Sem corrupção em crash.
fn save_motivo_file(dir: &Path, user: &str, entry: &PackEntry) -> std::io::Result<String> {
    let user_dir = dir.join(user);
    std::fs::create_dir_all(&user_dir)?;
    let id = nanoid!();
    let path = user_dir.join(format!("{id}.json"));
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(entry).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(id)
}

/// `POST /api/v1/comunicacao/motivos` — persiste um motivo como `PackEntry`
/// (formato de pack de arquivo, YG-166), atribuído ao usuário logado.
/// Auth obrigatório (JWT Bearer `sub`). Entrada deve ter `audio.mode` = `synth`
/// ou `sequence` — `speech` é rejeitado (motivos são sintetizados, não falados).
pub async fn save_motivo(
    State(state): State<Arc<MotivosState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let token = match extract_bearer(&headers) {
        Some(t) => t,
        None => return err_json(StatusCode::UNAUTHORIZED, "nao_autenticado"),
    };
    let user = match verify_jwt(&token, &state.jwt_secret) {
        Ok(u) => u,
        Err(_) => return err_json(StatusCode::UNAUTHORIZED, "nao_autenticado"),
    };

    let entry: PackEntry = match serde_json::from_value(body) {
        Ok(e) => e,
        Err(e) => return err_json(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
    };

    // Rejeita Speech — motivos são sintetizados; sem bytes de áudio em disco
    if matches!(entry.audio, Some(EntryAudio::Speech { .. })) {
        return err_json(StatusCode::UNPROCESSABLE_ENTITY, "motivo_nao_pode_ser_fala");
    }

    match save_motivo_file(&state.motivos_dir, &user, &entry) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": id, "motivo": entry })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(erro = %e, "save_motivo: io");
            err_json(StatusCode::INTERNAL_SERVER_ERROR, "erro_ao_salvar_motivo")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt;
    use yggdrasil_core::comunicacao::pack::{AbstractPos, Adsr, SeqStep, Waveform};
    use yggdrasil_core::comunicacao::{EntryAudio, PackEntry};

    use crate::auth::sign_jwt;

    const SECRET: &str = "test-secret";

    fn make_state(tmp: &tempfile::TempDir) -> Arc<MotivosState> {
        Arc::new(MotivosState {
            jwt_secret: SECRET.into(),
            motivos_dir: tmp.path().to_path_buf(),
        })
    }

    fn make_app(state: Arc<MotivosState>) -> Router {
        Router::new()
            .route("/api/v1/comunicacao/motivos", post(save_motivo))
            .with_state(state)
    }

    fn token(user: &str) -> String {
        sign_jwt(user, "u@test.com", SECRET).unwrap()
    }

    fn seq_entry() -> PackEntry {
        PackEntry::new("motivo-teste", AbstractPos::new(0.0, 0.0, 0.0))
            .with_role("motif")
            .with_gloss("3 passos de teste")
            .with_audio(EntryAudio::Sequence {
                waveform: Waveform::Triangle,
                steps: vec![
                    SeqStep {
                        freqs: vec![261.63],
                        duration_ms: 500,
                    },
                    SeqStep {
                        freqs: vec![329.63],
                        duration_ms: 500,
                    },
                    SeqStep {
                        freqs: vec![392.0],
                        duration_ms: 500,
                    },
                ],
                envelope: Adsr::default(),
            })
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn salva_sequencia_valida_cria_arquivo() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(&tmp);
        let app = make_app(state);
        let body = serde_json::to_string(&seq_entry()).unwrap();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/motivos")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = body_json(resp).await;
        assert!(v["id"].as_str().is_some(), "id presente");
        assert_eq!(v["motivo"]["role"], "motif");

        // arquivo gravado na subpasta do usuário
        let alice_dir = tmp.path().join("alice");
        assert!(alice_dir.is_dir(), "pasta do usuário criada");
        let files: Vec<_> = std::fs::read_dir(&alice_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1, "exatamente 1 arquivo gravado");
    }

    #[tokio::test]
    async fn rejeita_sem_token() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(&tmp);
        let app = make_app(state);
        let body = serde_json::to_string(&seq_entry()).unwrap();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/motivos")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejeita_audio_speech() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(&tmp);
        let app = make_app(state);
        let entry = PackEntry::new("teste", AbstractPos::new(0.0, 0.0, 0.0)).with_audio(
            EntryAudio::Speech {
                text: "olá".into(),
                ipa: None,
                lang: "pt".into(),
            },
        );
        let body = serde_json::to_string(&entry).unwrap();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/motivos")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let v = body_json(resp).await;
        assert_eq!(v["erro"], "motivo_nao_pode_ser_fala");
    }

    #[tokio::test]
    async fn escrita_atomica_sem_tmp_residual() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(&tmp);
        let app = make_app(state);
        let body = serde_json::to_string(&seq_entry()).unwrap();

        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/comunicacao/motivos")
                .header("authorization", format!("Bearer {}", token("bob")))
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

        let bob_dir = tmp.path().join("bob");
        let tmps: Vec<_> = std::fs::read_dir(&bob_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "tmp").unwrap_or(false))
            .collect();
        assert!(tmps.is_empty(), ".tmp não deve restar após rename atômico");
    }

    #[tokio::test]
    async fn aceita_synth_direto() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(&tmp);
        let app = make_app(state);
        let entry = PackEntry::new("nota-sol", AbstractPos::new(1.0, 0.0, 0.0))
            .with_role("note")
            .with_audio(yggdrasil_core::comunicacao::EntryAudio::Synth {
                waveform: Waveform::Sine,
                freqs: vec![392.0],
                envelope: Adsr::default(),
                duration_ms: 500,
            });
        let body = serde_json::to_string(&entry).unwrap();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/motivos")
                    .header("authorization", format!("Bearer {}", token("carol")))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }
}
