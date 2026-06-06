//! Producer de eventos para a *federated bus* do CO (YG-93, Fase P-A do ADR
//! `docs/architecture/event-driven-sync.md`).
//!
//! **Topologia: "CO is hub".** O Yggdrasil **hospeda** as notas
//! (`/data/instances/<id>/notes/<slug>.md`, Fase 0) e **disca para fora** ao hub
//! do CO (`wss://co.artelonga.com.br/api/v1/events/bridge`, CO-384), emitindo um
//! [`FederatedEvent`] `entry.{created,updated,deleted}` a cada escrita/remoção de
//! nota. Este módulo é **one-way** (producer → hub): aplicar eventos vindos do CO
//! é Fase P-B (v3.1) e está fora de escopo — só o `hop_count` loop-guard já fica
//! pronto.
//!
//! **Gate de config.** Sem `YGG_CO_BRIDGE_URL` **e** `YGG_CO_BRIDGE_TOKEN` o
//! producer é **no-op**: [`spawn`] não sobe nenhuma task e o boot do servidor não
//! é afetado. Os testes não dependem de um CO vivo — exercitam só a lógica pura
//! (envelope, `event_log`, offset de `last_delivered_event_id`, backoff).
//!
//! **Reuso.** Espelha o padrão `broadcast` do poker
//! (`yggdrasil-core/src/games/poker/events.rs` + `.../poker/ws.rs`) para o canal
//! interno de [`NoteWritten`], e o padrão de spawn de background de
//! `universos_routes::spawn_cleanup_job` para a task de fundo. O cliente WS usa
//! `tokio-tungstenite` (TLS via rustls, alinhado com o `reqwest` de `auth_co.rs`).
//!
//! **E2E pende CO-384** — o hub `/api/v1/events/bridge` ainda não existe, então
//! aqui só há build + testes de unidade da lógica pura.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use yggdrasil_core::instance::{InstanceStore, NoteStore};

/// `universe_key` fixo deste producer no bus do CO.
pub const UNIVERSE_KEY: &str = "yggdrasil";

/// Capacidade do canal `broadcast` interno de [`NoteWritten`]. Generosa: o
/// consumidor (a task de fundo) drena rápido; lag só ocorre se o CO ficar fora
/// por muito tempo — e aí o `event_log` (persistido em memória) é a fonte de
/// verdade do replay, não o canal.
pub const NOTE_CHANNEL_CAPACITY: usize = 1024;

// ─── Evento interno (padrão broadcast do poker) ──────────────────────────────

/// Tipo de escrita que originou o evento de nota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// Nota criada (não existia antes do `save`).
    Created,
    /// Nota atualizada (já existia).
    Updated,
    /// Nota removida (`delete`).
    Deleted,
}

impl NoteKind {
    /// `event_type` do [`FederatedEvent`] correspondente.
    pub fn event_type(self) -> &'static str {
        match self {
            NoteKind::Created => "entry.created",
            NoteKind::Updated => "entry.updated",
            NoteKind::Deleted => "entry.deleted",
        }
    }
}

/// Evento interno publicado no canal `broadcast` a cada `NoteStore::save`/`delete`
/// (mesmo papel do `TableEvent` do poker). A camada web emite; a task de fundo
/// consome, faz o append no [`EventLog`] e despacha ao CO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteWritten {
    /// Id da instância dona da nota.
    pub instance: String,
    /// Slug canônico da nota.
    pub slug: String,
    /// Created / Updated / Deleted.
    pub kind: NoteKind,
    /// Título no momento da escrita (vazio em `Deleted`).
    #[serde(default)]
    pub title: String,
    /// Corpo Markdown no momento da escrita (vazio em `Deleted`).
    #[serde(default)]
    pub body: String,
    /// `updated` (RFC3339) da nota, se conhecido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl NoteWritten {
    /// `path` no envelope: `notes/<slug>.md` (convenção do ADR / CO-384).
    pub fn path(&self) -> String {
        format!("notes/{}.md", self.slug)
    }
}

// ─── Envelope FederatedEvent (compatível com CO-384) ─────────────────────────

/// O `payload` de um `entry.*`: o conteúdo da nota. Em `entry.deleted` os campos
/// ficam vazios (só `path`/`slug` identificam o alvo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryPayload {
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Envelope publicado no hub do CO. Shape alinhado ao `FederatedEvent` de CO-384:
/// `event` (id + tipo + alvo + payload) embrulhado por metadados de federação
/// (`origin_deployment`, `signed_by`, `hop_count`).
///
/// `event_id` é um id monotônico **deste** deployment (offset do `event_log`); o
/// CO deduplica idempotentemente (CO-380), então reentregar um id já visto é
/// inócuo — é o que torna cold-start = warm-reconnect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FederatedEvent {
    /// Id monotônico do evento no `event_log` deste deployment.
    pub event_id: u64,
    /// `entry.created` | `entry.updated` | `entry.deleted`.
    pub event_type: String,
    /// Sempre `yggdrasil` neste producer.
    pub universe_key: String,
    /// `notes/<slug>.md`.
    pub path: String,
    /// Conteúdo da nota.
    pub payload: EntryPayload,
    /// Deployment de origem (loop-guard / atribuição).
    pub origin_deployment: String,
    /// Identidade que assinou (o `node_id` deste producer).
    pub signed_by: String,
    /// Saltos já dados no bus. Origem = 0; o CO incrementa no fan-out. O
    /// producer **descarta** qualquer evento recebido com `hop_count > 0` que
    /// se origine aqui (loop-guard pronto p/ P-B; não recebemos em v3.0).
    pub hop_count: u32,
}

/// `origin_deployment` / `signed_by` default deste producer.
pub const ORIGIN_DEPLOYMENT: &str = "yggdrasil.artelonga.com.br";

impl FederatedEvent {
    /// Embrulha uma entrada do log num envelope pronto para o CO.
    pub fn from_log_entry(entry: &LoggedEvent, node_id: &str) -> Self {
        FederatedEvent {
            event_id: entry.id,
            event_type: entry.note.kind.event_type().to_string(),
            universe_key: UNIVERSE_KEY.to_string(),
            path: entry.note.path(),
            payload: EntryPayload {
                title: entry.note.title.clone(),
                body: entry.note.body.clone(),
                updated_at: entry.note.updated_at.clone(),
            },
            origin_deployment: ORIGIN_DEPLOYMENT.to_string(),
            signed_by: node_id.to_string(),
            hop_count: 0,
        }
    }

    /// Codifica como JSON (o frame de texto enviado no WS).
    pub fn encode(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Loop-guard: um evento recebido cuja origem é este deployment (echo do
    /// nosso próprio write, `hop_count > 0`) não deve ser reaplicado. Pronto p/
    /// P-B; em v3.0 não recebemos nada, então é só uma função pura testável.
    pub fn is_own_echo(&self) -> bool {
        self.origin_deployment == ORIGIN_DEPLOYMENT && self.hop_count > 0
    }
}

// ─── event_log + last_delivered_event_id ─────────────────────────────────────

/// Uma entrada do `event_log`: um [`NoteWritten`] com seu id monotônico.
#[derive(Debug, Clone, PartialEq)]
pub struct LoggedEvent {
    pub id: u64,
    pub note: NoteWritten,
}

/// Log append-only de eventos de nota, em memória, com rastreio do
/// `last_delivered_event_id` (o último id ACK'd pelo CO).
///
/// **Cold-start = warm-reconnect, mesmo code path** (ADR §P-A):
/// - `last_delivered == None` (primeira conexão / staging resetou) →
///   [`pending`] devolve o log **inteiro** = backfill de **todas** as notas.
/// - reconnect após ACK → [`pending`] devolve só a partir do último id ACK'd.
///
/// As notas da Fase 0 já em disco **não têm** entrada no log (são anteriores a
/// ele); [`seed_from_store`] as semeia sintetizando um `entry.created` por nota
/// no primeiro boot. Daí em diante o log é append-only por `save`/`delete`.
#[derive(Debug, Default)]
pub struct EventLog {
    events: Vec<LoggedEvent>,
    next_id: u64,
    /// Último id confirmado (ACK) pelo CO. `None` = nada entregue ainda.
    last_delivered: Option<u64>,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            next_id: 1,
            last_delivered: None,
        }
    }

    /// Anexa um evento, devolvendo o id atribuído. Ids são monotônicos a partir
    /// de 1.
    pub fn append(&mut self, note: NoteWritten) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.events.push(LoggedEvent { id, note });
        id
    }

    /// Quantos eventos há no log.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// O último id ACK'd pelo CO (`None` = nenhum).
    pub fn last_delivered(&self) -> Option<u64> {
        self.last_delivered
    }

    /// Marca tudo até `id` (inclusive) como entregue. Monotônico: um ACK menor
    /// que o atual é ignorado (reordenação/duplicata).
    pub fn ack(&mut self, id: u64) {
        if self.last_delivered.is_none_or(|cur| id > cur) {
            self.last_delivered = Some(id);
        }
    }

    /// Eventos ainda **não** entregues — o que deve ser enviado no (re)connect.
    ///
    /// É o coração do cold-start = warm-reconnect: o offset inicial vem de
    /// `last_delivered`; `None` ⇒ o log inteiro.
    pub fn pending(&self) -> &[LoggedEvent] {
        let from = self.last_delivered.unwrap_or(0);
        // ids são contíguos a partir de 1; o primeiro pendente é `from`.
        let start = self.events.partition_point(|e| e.id <= from);
        &self.events[start..]
    }

    /// Semeia o log enumerando as notas já em disco de **todas** as instâncias,
    /// sintetizando um `entry.created` por nota. Idempotente do ponto de vista
    /// do CO (dedupe por id no fan-out), mas deve rodar **uma vez** no boot
    /// antes do append-only por `save`/`delete`. Devolve quantas notas semeou.
    pub fn seed_from_store(&mut self, store: &InstanceStore) -> usize {
        let instances = match store.list_all() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("co-bridge seed: list_all falhou: {e}");
                return 0;
            }
        };
        let mut seeded = 0;
        for inst in instances {
            let notes = NoteStore::for_instance(store.root(), &inst.id);
            let listed = match notes.list() {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!("co-bridge seed: list notas de {} falhou: {e}", inst.id);
                    continue;
                }
            };
            for n in listed {
                self.append(NoteWritten {
                    instance: inst.id.clone(),
                    slug: n.slug,
                    kind: NoteKind::Created,
                    title: n.title,
                    body: n.body,
                    updated_at: Some(n.updated.to_rfc3339()),
                });
                seeded += 1;
            }
        }
        seeded
    }
}

// ─── Reconnect: backoff exponencial ──────────────────────────────────────────

/// Backoff mínimo do reconnect (1s).
pub const BACKOFF_MIN: Duration = Duration::from_secs(1);
/// Backoff máximo (teto) do reconnect (30s).
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Estado da conexão WS ao hub do CO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// Tentando (re)conectar; carrega o atraso atual do backoff.
    Connecting,
    /// Conectado e autenticado.
    Connected,
}

/// Máquina de backoff exponencial 1s → 2s → 4s → … → 30s (teto), reset em
/// conexão bem-sucedida. Pura e testável; a task de fundo dorme `current()`
/// entre tentativas.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    current: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            current: BACKOFF_MIN,
        }
    }

    /// Atraso a esperar antes da próxima tentativa.
    pub fn current(&self) -> Duration {
        self.current
    }

    /// Dobra o atraso (com teto em [`BACKOFF_MAX`]) e devolve o **novo** atraso.
    /// Chamado após uma falha de conexão.
    pub fn fail(&mut self) -> Duration {
        let doubled = self.current.saturating_mul(2);
        self.current = doubled.min(BACKOFF_MAX);
        self.current
    }

    /// Reseta para o mínimo após uma conexão bem-sucedida.
    pub fn reset(&mut self) {
        self.current = BACKOFF_MIN;
    }
}

// ─── Config (gate de env) ────────────────────────────────────────────────────

/// Config do producer, lida de `YGG_CO_BRIDGE_URL` + `YGG_CO_BRIDGE_TOKEN`.
/// Ausência de qualquer uma ⇒ producer desligado ([`from_env`] devolve `None`).
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// URL do hub (`wss://co.artelonga.com.br/api/v1/events/bridge`).
    pub url: String,
    /// Service JWT emitido pelo CO (apresentado na conexão).
    pub token: String,
    /// Identidade estável deste producer no bus.
    pub node_id: String,
}

impl BridgeConfig {
    /// Lê a config do ambiente. `None` se `YGG_CO_BRIDGE_URL` **ou**
    /// `YGG_CO_BRIDGE_TOKEN` estiver ausente/vazio — o caso no-op.
    pub fn from_env() -> Option<Self> {
        let url = non_empty_env("YGG_CO_BRIDGE_URL")?;
        let token = non_empty_env("YGG_CO_BRIDGE_TOKEN")?;
        let node_id =
            non_empty_env("YGG_CO_BRIDGE_NODE_ID").unwrap_or_else(|| ORIGIN_DEPLOYMENT.to_string());
        Some(Self {
            url,
            token,
            node_id,
        })
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Monta a request de handshake WS ao hub do CO: a URL + `Authorization: Bearer
/// <service JWT>` (a mesma relação de confiança do handover SSO de `auth_co.rs`).
/// Pura e testável; é o que o dial `tokio_tungstenite::connect_async` consome
/// quando o CO-384 existir.
fn ws_request(
    config: &BridgeConfig,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, BridgeError> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = config
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| BridgeError::BadRequest(e.to_string()))?;
    let value = format!("Bearer {}", config.token)
        .parse()
        .map_err(|_| BridgeError::BadRequest("token inválido p/ header".into()))?;
    req.headers_mut().insert("authorization", value);
    Ok(req)
}

// ─── Spawn da task de fundo (gated) ──────────────────────────────────────────

/// Canal interno + handle para a task de fundo. O `sender` vive no estado da
/// camada web (`InstancesState`); a task de fundo segura o `receiver`.
pub struct Producer {
    sender: broadcast::Sender<NoteWritten>,
}

impl Producer {
    /// Cria o canal `broadcast`. Sempre seguro de chamar (não toca em rede).
    pub fn new() -> Self {
        let (sender, _rx) = broadcast::channel(NOTE_CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Clona o `sender` para a camada web emitir [`NoteWritten`].
    pub fn sender(&self) -> broadcast::Sender<NoteWritten> {
        self.sender.clone()
    }
}

impl Default for Producer {
    fn default() -> Self {
        Self::new()
    }
}

/// Sobe a task de fundo **apenas** se [`BridgeConfig::from_env`] estiver
/// configurada (gate). Sem config ⇒ no-op: devolve `false` e não toca em rede —
/// o boot do servidor e os testes seguem intactos.
///
/// Quando configurada, semeia o `event_log` das notas da Fase 0
/// ([`EventLog::seed_from_store`]) e sobe o loop de dial/reconnect/replay.
/// Espelha o padrão de `universos_routes::spawn_cleanup_job`.
pub fn spawn(producer: &Producer, store: std::sync::Arc<InstanceStore>) -> bool {
    let Some(config) = BridgeConfig::from_env() else {
        tracing::info!(
            "co-bridge producer desligado (YGG_CO_BRIDGE_URL/TOKEN não configurados) — no-op"
        );
        return false;
    };
    let rx = producer.sender.subscribe();
    tokio::spawn(run_loop(config, store, rx));
    true
}

/// Loop de fundo: semeia o log, conecta com backoff, drena pendentes, e segue
/// fazendo append + envio a cada [`NoteWritten`].
///
/// **NOTA:** a parte de rede (dial WS ao hub) **não pode ser exercitada sem o
/// CO-384 vivo** — é o E2E que aguarda CO-384. A lógica pura abaixo (seed,
/// append, offset, backoff, encode) é coberta por testes de unidade.
async fn run_loop(
    config: BridgeConfig,
    store: std::sync::Arc<InstanceStore>,
    mut rx: broadcast::Receiver<NoteWritten>,
) {
    let mut log = EventLog::new();
    let seeded = log.seed_from_store(&store);
    tracing::info!(
        "co-bridge producer: {seeded} nota(s) da Fase 0 semeadas no event_log; \
         hub={}",
        config.url
    );

    let mut backoff = Backoff::new();
    loop {
        match connect_and_stream(&config, &mut log, &mut rx).await {
            Ok(()) => {
                // Stream encerrou sem erro (improvável); reseta e reconecta.
                backoff.reset();
            }
            Err(e) => {
                let delay = backoff.fail();
                tracing::warn!(
                    "co-bridge producer: conexão caiu ({e}); reconnect em {:?}",
                    delay
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Conecta ao hub, autentica, drena pendentes e faz streaming dos novos eventos.
///
/// **STUB de rede (E2E aguarda CO-384).** O hub `/api/v1/events/bridge` ainda
/// não existe, então não há servidor para dialar nem testar ponta-a-ponta. Esta
/// função descreve o contrato e mantém o `event_log` consistente (append por
/// evento) sem depender de um CO vivo: ela monta o envelope que *seria* enviado
/// e registra. Substituir o corpo pelo dial `tokio-tungstenite` real quando o
/// CO-384 aterrissar (a forma do cliente segue o `reqwest`/rustls de
/// `auth_co.rs`).
async fn connect_and_stream(
    config: &BridgeConfig,
    log: &mut EventLog,
    rx: &mut broadcast::Receiver<NoteWritten>,
) -> Result<(), BridgeError> {
    // — Passo 1 (pós-CO-384): dial `wss://…/events/bridge`, apresentar
    //   `config.token` (service JWT), aguardar aceite (403 ⇒ refresh + retry).
    // — Passo 2: drenar `log.pending()` (cold-start = log inteiro; warm =
    //   a partir do último ACK), enviando um `FederatedEvent` por entrada.
    // — Passo 3: a cada `NoteWritten` recebido no canal, `log.append()` +
    //   enviar; ao receber ACK, `log.ack(id)`.
    //
    // Sem o hub vivo, não abrimos socket. Validamos a request de handshake
    // (URL + Bearer) — o que `connect_async` consumirá quando o CO-384 existir —
    // e mantemos o log consistente consumindo o canal interno (para o backfill
    // futuro não duplicar), devolvendo "indisponível" para acionar o backoff.
    let _ws_request = ws_request(config)?;
    for entry in log.pending() {
        let env = FederatedEvent::from_log_entry(entry, &config.node_id);
        match env.encode() {
            Ok(_json) => {
                tracing::debug!(
                    "co-bridge producer: envelope pronto (event_id={}, type={}) — \
                     envio aguarda CO-384",
                    env.event_id,
                    env.event_type
                );
            }
            Err(e) => tracing::error!("co-bridge producer: encode falhou: {e}"),
        }
    }

    // Consome o próximo evento interno para manter o log crescendo enquanto o
    // hub não existe (evita perda silenciosa de eventos antes do CO-384).
    match rx.recv().await {
        Ok(note) => {
            log.append(note);
            Err(BridgeError::HubUnavailable)
        }
        Err(broadcast::error::RecvError::Lagged(n)) => {
            tracing::warn!("co-bridge producer: canal lagou {n} evento(s)");
            Err(BridgeError::HubUnavailable)
        }
        Err(broadcast::error::RecvError::Closed) => Err(BridgeError::ChannelClosed),
    }
}

/// Falhas da ponte (acionam backoff/reconnect).
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("hub do CO indisponível (CO-384 ainda não existe)")]
    HubUnavailable,
    #[error("canal interno de notas fechado")]
    ChannelClosed,
    #[error("request WS inválida: {0}")]
    BadRequest(String),
}

/// Caminho da nota no disco (`<root>/<instance>/notes/<slug>.md`). Útil para
/// diagnósticos; não usado no envelope (que usa o `path` relativo).
#[allow(dead_code)]
fn note_disk_path(root: &std::path::Path, instance: &str, slug: &str) -> PathBuf {
    root.join(instance).join("notes").join(format!("{slug}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn note(instance: &str, slug: &str, kind: NoteKind) -> NoteWritten {
        NoteWritten {
            instance: instance.into(),
            slug: slug.into(),
            kind,
            title: format!("Título {slug}"),
            body: format!("corpo de {slug} [[outra]]"),
            updated_at: Some("2026-06-06T00:00:00+00:00".into()),
        }
    }

    // ── Envelope ──────────────────────────────────────────────────────────

    #[test]
    fn note_kind_event_type_mapeia_entry_verbs() {
        assert_eq!(NoteKind::Created.event_type(), "entry.created");
        assert_eq!(NoteKind::Updated.event_type(), "entry.updated");
        assert_eq!(NoteKind::Deleted.event_type(), "entry.deleted");
    }

    #[test]
    fn note_written_path_e_notes_slug_md() {
        let n = note("inst-1", "minha-nota", NoteKind::Created);
        assert_eq!(n.path(), "notes/minha-nota.md");
    }

    #[test]
    fn envelope_encode_inclui_campos_de_federacao() {
        let mut log = EventLog::new();
        let id = log.append(note("inst-1", "n", NoteKind::Created));
        let entry = &log.pending()[0];
        assert_eq!(entry.id, id);
        let env = FederatedEvent::from_log_entry(entry, "node-xyz");
        assert_eq!(env.event_id, 1);
        assert_eq!(env.event_type, "entry.created");
        assert_eq!(env.universe_key, "yggdrasil");
        assert_eq!(env.path, "notes/n.md");
        assert_eq!(env.signed_by, "node-xyz");
        assert_eq!(env.origin_deployment, ORIGIN_DEPLOYMENT);
        assert_eq!(env.hop_count, 0);
        assert_eq!(env.payload.title, "Título n");

        // round-trip JSON
        let json = env.encode().unwrap();
        let back: FederatedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
        // chaves esperadas pelo CO-384 presentes no wire
        assert!(json.contains("\"event_type\":\"entry.created\""));
        assert!(json.contains("\"universe_key\":\"yggdrasil\""));
        assert!(json.contains("\"path\":\"notes/n.md\""));
        assert!(json.contains("\"hop_count\":0"));
    }

    #[test]
    fn deleted_envelope_usa_entry_deleted() {
        let mut log = EventLog::new();
        log.append(note("inst-1", "n", NoteKind::Deleted));
        let env = FederatedEvent::from_log_entry(&log.pending()[0], "node");
        assert_eq!(env.event_type, "entry.deleted");
    }

    #[test]
    fn loop_guard_descarta_proprio_echo() {
        let mut log = EventLog::new();
        log.append(note("inst-1", "n", NoteKind::Created));
        let mut env = FederatedEvent::from_log_entry(&log.pending()[0], "node");
        assert!(!env.is_own_echo(), "hop 0 da origem não é echo");
        env.hop_count = 2; // voltou do hub
        assert!(env.is_own_echo(), "echo do próprio deployment é descartado");
        // evento de outra origem não é nosso echo
        env.origin_deployment = "co.artelonga.com.br".into();
        assert!(!env.is_own_echo());
    }

    // ── event_log: append + offset (last_delivered_event_id) ──────────────

    #[test]
    fn append_atribui_ids_monotonicos_a_partir_de_1() {
        let mut log = EventLog::new();
        assert_eq!(log.append(note("i", "a", NoteKind::Created)), 1);
        assert_eq!(log.append(note("i", "b", NoteKind::Created)), 2);
        assert_eq!(log.append(note("i", "c", NoteKind::Updated)), 3);
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn cold_start_pending_e_o_log_inteiro_quando_last_delivered_null() {
        let mut log = EventLog::new();
        log.append(note("i", "a", NoteKind::Created));
        log.append(note("i", "b", NoteKind::Created));
        assert_eq!(log.last_delivered(), None);
        let pending = log.pending();
        assert_eq!(pending.len(), 2, "NULL → stream do log inteiro");
        assert_eq!(pending[0].id, 1);
        assert_eq!(pending[1].id, 2);
    }

    #[test]
    fn warm_reconnect_pending_comeca_apos_o_ultimo_ack() {
        let mut log = EventLog::new();
        for s in ["a", "b", "c", "d"] {
            log.append(note("i", s, NoteKind::Created));
        }
        log.ack(2); // CO confirmou até o id 2
        let pending = log.pending();
        assert_eq!(pending.len(), 2, "só os não-entregues");
        assert_eq!(pending[0].id, 3);
        assert_eq!(pending[1].id, 4);
        assert_eq!(log.last_delivered(), Some(2));
    }

    #[test]
    fn ack_e_monotonico_ignora_retrocesso() {
        let mut log = EventLog::new();
        for s in ["a", "b", "c"] {
            log.append(note("i", s, NoteKind::Created));
        }
        log.ack(3);
        log.ack(1); // duplicata/reordenação — ignorada
        assert_eq!(log.last_delivered(), Some(3));
        assert!(log.pending().is_empty());
    }

    #[test]
    fn novos_appends_apos_ack_ficam_pendentes() {
        let mut log = EventLog::new();
        log.append(note("i", "a", NoteKind::Created));
        log.ack(1);
        assert!(log.pending().is_empty());
        log.append(note("i", "b", NoteKind::Updated)); // id 2
        let pending = log.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, 2);
    }

    // ── cold-start backfill: seed_from_store ──────────────────────────────

    fn store_with_notes() -> (TempDir, std::sync::Arc<InstanceStore>) {
        let dir = TempDir::new().unwrap();
        let store = std::sync::Arc::new(InstanceStore::new(dir.path()).unwrap());

        // duas instâncias, com notas em disco (Fase 0, anteriores ao log)
        for (iid, slugs) in [("inst-a", vec!["um", "dois"]), ("inst-b", vec!["tres"])] {
            let inst = UniverseInstance::empty(iid, "owner@test", "Inst");
            store.save(&inst).unwrap();
            let ns = NoteStore::for_instance(store.root(), iid);
            for s in slugs {
                ns.save(s, &format!("T {s}"), &format!("corpo {s}"))
                    .unwrap();
            }
        }
        (dir, store)
    }

    use yggdrasil_core::instance::UniverseInstance;

    #[test]
    fn seed_from_store_sintetiza_entry_created_por_nota_existente() {
        let (_dir, store) = store_with_notes();
        let mut log = EventLog::new();
        let seeded = log.seed_from_store(&store);
        assert_eq!(seeded, 3, "2 notas em inst-a + 1 em inst-b");
        assert_eq!(log.len(), 3);

        // todas são entry.created e cobrem ambas as instâncias
        let pending = log.pending();
        assert_eq!(pending.len(), 3, "cold-start: log inteiro pendente");
        assert!(pending.iter().all(|e| e.note.kind == NoteKind::Created));
        let instances: std::collections::BTreeSet<_> =
            pending.iter().map(|e| e.note.instance.as_str()).collect();
        assert_eq!(instances, ["inst-a", "inst-b"].into_iter().collect());

        // o envelope sintetizado tem path notes/<slug>.md e payload preenchido
        let env = FederatedEvent::from_log_entry(&pending[0], "n");
        assert!(env.path.starts_with("notes/"));
        assert!(!env.payload.body.is_empty());
        assert!(env.payload.updated_at.is_some());
    }

    #[test]
    fn seed_de_store_vazio_nao_semeia_nada() {
        let dir = TempDir::new().unwrap();
        let store = InstanceStore::new(dir.path()).unwrap();
        let mut log = EventLog::new();
        assert_eq!(log.seed_from_store(&store), 0);
        assert!(log.is_empty());
    }

    #[test]
    fn seed_depois_append_continua_monotonico() {
        let (_dir, store) = store_with_notes();
        let mut log = EventLog::new();
        log.seed_from_store(&store); // ids 1..=3
        let next = log.append(note("inst-a", "novo", NoteKind::Created));
        assert_eq!(next, 4, "append pós-seed continua a sequência");
    }

    // ── reconnect: backoff exponencial ────────────────────────────────────

    #[test]
    fn backoff_dobra_ate_o_teto_de_30s() {
        let mut b = Backoff::new();
        assert_eq!(b.current(), Duration::from_secs(1));
        assert_eq!(b.fail(), Duration::from_secs(2));
        assert_eq!(b.fail(), Duration::from_secs(4));
        assert_eq!(b.fail(), Duration::from_secs(8));
        assert_eq!(b.fail(), Duration::from_secs(16));
        assert_eq!(b.fail(), Duration::from_secs(30), "teto em 30s");
        assert_eq!(b.fail(), Duration::from_secs(30), "satura no teto");
    }

    #[test]
    fn backoff_reseta_em_conexao_bem_sucedida() {
        let mut b = Backoff::new();
        b.fail();
        b.fail();
        assert!(b.current() > BACKOFF_MIN);
        b.reset();
        assert_eq!(b.current(), BACKOFF_MIN);
    }

    // ── config gate ───────────────────────────────────────────────────────

    #[test]
    fn config_from_env_none_sem_url_ou_token() {
        // Serializa o acesso a env vars do processo entre estes dois testes.
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("YGG_CO_BRIDGE_URL");
            std::env::remove_var("YGG_CO_BRIDGE_TOKEN");
        }
        assert!(BridgeConfig::from_env().is_none(), "sem nada → no-op");

        unsafe {
            std::env::set_var("YGG_CO_BRIDGE_URL", "wss://co.test/api/v1/events/bridge");
        }
        assert!(
            BridgeConfig::from_env().is_none(),
            "só URL, sem token → no-op"
        );
        unsafe {
            std::env::remove_var("YGG_CO_BRIDGE_URL");
        }
    }

    #[test]
    fn config_from_env_some_com_url_e_token() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("YGG_CO_BRIDGE_URL", "wss://co.test/api/v1/events/bridge");
            std::env::set_var("YGG_CO_BRIDGE_TOKEN", "service-jwt-123");
            std::env::remove_var("YGG_CO_BRIDGE_NODE_ID");
        }
        let cfg = BridgeConfig::from_env().expect("configurado");
        assert_eq!(cfg.url, "wss://co.test/api/v1/events/bridge");
        assert_eq!(cfg.token, "service-jwt-123");
        assert_eq!(cfg.node_id, ORIGIN_DEPLOYMENT, "node_id default");
        unsafe {
            std::env::remove_var("YGG_CO_BRIDGE_URL");
            std::env::remove_var("YGG_CO_BRIDGE_TOKEN");
        }
    }

    #[test]
    fn ws_request_inclui_url_e_bearer() {
        let cfg = BridgeConfig {
            url: "wss://co.test/api/v1/events/bridge".into(),
            token: "service-jwt-abc".into(),
            node_id: "node-1".into(),
        };
        let req = ws_request(&cfg).expect("request válida");
        assert_eq!(req.uri().to_string(), "wss://co.test/api/v1/events/bridge");
        let auth = req.headers().get("authorization").unwrap();
        assert_eq!(auth, "Bearer service-jwt-abc");
    }

    #[test]
    fn ws_request_url_invalida_erro() {
        let cfg = BridgeConfig {
            url: "not a url".into(),
            token: "t".into(),
            node_id: "n".into(),
        };
        assert!(matches!(ws_request(&cfg), Err(BridgeError::BadRequest(_))));
    }

    #[test]
    fn spawn_e_no_op_sem_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("YGG_CO_BRIDGE_URL");
            std::env::remove_var("YGG_CO_BRIDGE_TOKEN");
        }
        let dir = TempDir::new().unwrap();
        let store = std::sync::Arc::new(InstanceStore::new(dir.path()).unwrap());
        let producer = Producer::new();
        assert!(
            !spawn(&producer, store),
            "sem env → spawn devolve false (no-op, nada de rede)"
        );
    }

    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());
}
