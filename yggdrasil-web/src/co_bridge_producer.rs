//! Producer de eventos para a *federated bus* do CO (YG-93 Fase P-A + YG-103,
//! do ADR `docs/architecture/event-driven-sync.md`).
//!
//! **Topologia: "CO is hub".** O Yggdrasil **hospeda** o conteúdo e **disca para
//! fora** ao hub do CO (`wss://co.artelonga.com.br/api/v1/events/bridge`, CO-384),
//! emitindo um [`BridgeMessage::Event`] a cada escrita. Duas **fontes**
//! ([`FederatedSource`]) no mesmo envelope:
//! - **notas** (Fase 0, YG-93) → `universe_key=yggdrasil`,
//!   `instances/<id>/notes/<slug>.md` (instance-qualified desde YG-97);
//! - **termos de léxico** das salas de comunicação (YG-103) →
//!   `universe_key=comunicacao`, `<lang>/terms/_users/<user-slug>/<slug>.md`
//!   (consumer = CO-389, filtro `_users/` — **YG-118**).
//!
//! **Wire congelado (YG-118).** O envelope no fio é o `Event` do CO-380 embrulhado
//! em `FederatedEvent` pelo CO-384 — **não** a flat `FederatedEvent` anterior. O
//! Yggdrasil envia [`BridgeMessage::Event`] cujo `federated.event` é um [`CoEvent`]
//! (mirror de CO-380). Payloads tipados: [`TermPayload`] (termos), [`NotePayload`]
//! (notas), [`SalaPayload`] (atividade de sala).
//!
//! Além do conteúdo, emite **atividade** de sala ([`RoomActivity`],
//! `yggdrasil.sala.*`) — atividade não-conteúdo p/ o `/agora`, sem entry/log.
//! Este módulo é **one-way** (producer → hub): aplicar eventos vindos do CO é
//! Fase P-B (YG-97/v3.1) e está fora de escopo — só o `hop_count` loop-guard já
//! fica pronto.
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
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
use yggdrasil_core::comunicacao::LexiconStore;
use yggdrasil_core::comunicacao::lexicon::slugify;
use yggdrasil_core::comunicacao::room::{Element, Room};
use yggdrasil_core::comunicacao::store::RoomStore;
use yggdrasil_core::instance::{InstanceStore, NoteStore};

/// `universe_key` das **notas** (editor de instâncias) no bus do CO.
pub const UNIVERSE_KEY: &str = "yggdrasil";

/// Instância canônica do **Ayvu Rapyta** (YG-114). As notas do Caderno do corpus
/// (YG-112) são gravadas via `NoteStore` sob esta instância — assim o producer
/// staged (YG-93/97) as federa **de graça** no path instance-qualified
/// `instances/ayvu-rapyta/notes/<slug>.md`, batendo o contrato do consumer CO-389.
/// É o *seam* que trava o contrato (instância + slug + path) antes do E2E.
pub const AYVU_INSTANCE: &str = "ayvu-rapyta";

/// Adaptador **verso → slug** estável do Caderno do Ayvu Rapyta (YG-114).
/// Reusa a mesma [`slugify`] do léxico/notas, então a âncora de um verso é
/// idêntica em todo o sistema (`verse_slug(1, 2) → "1-2"`,
/// `verse_slug("I", "12a") → "i-12a"`). É a chave por baixo do `NoteWritten`
/// federado: `instances/ayvu-rapyta/notes/<verse_slug>.md` (contrato CO-389).
pub fn verse_slug(chapter: impl AsRef<str>, verse: impl AsRef<str>) -> String {
    slugify(&format!("{} {}", chapter.as_ref(), verse.as_ref()))
}

/// `universe_key` dos **termos de léxico** das salas de comunicação (YG-103).
/// É a universe `comunicacao` que **já existe** no CO (mbya/yoruba) — o producer
/// só adiciona a camada de evento ao vivo (<1s) por cima do sync de repo (YG-100).
pub const COMUNICACAO_UNIVERSE_KEY: &str = "comunicacao";

/// SHA-256 hex de um body Markdown. Usado em [`TermPayload`] e [`NotePayload`]
/// para dedup no consumer CO-389 (hash match → não reescreve).
pub fn body_sha256(body: &str) -> String {
    let mut h = Sha256::new();
    h.update(body.as_bytes());
    format!("{:x}", h.finalize())
}

// ─── Visibility (mirror CO-380) ───────────────────────────────────────────────

/// Visibilidade do evento — controla federação (CO-380). Só `Public` e
/// `UniverseMembers` são federáveis pelo hub (CO-384 `is_federatable`).
/// Privado (`universe_owner`, `user_only`, `system`) fica local: não federa.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Public,
    UniverseMembers,
    UniverseOwner,
    UserOnly,
    System,
}

impl Visibility {
    /// `true` se o evento pode ser federado ao hub.
    pub fn is_federatable(&self) -> bool {
        matches!(self, Self::Public | Self::UniverseMembers)
    }
}

// ─── Wire types: CoEvent + FederatedEvent + BridgeMessage (mirrors CO-380/384) ─

/// Mirror local do `Event` do CO-380 — o envelope interno de cada evento na bus.
/// Serializa identicamente ao `co_web::eda::event::Event`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoEvent {
    /// Identificador único (UUID v4 como string).
    pub id: String,
    /// Tipo namespaced: `entry.created`, `yggdrasil.sala.user_joined`, …
    pub event_type: String,
    /// `None` para eventos de atividade de sala (sem universe).
    pub universe_key: Option<String>,
    /// `sub` do JWT do ator (usuário que originou).
    pub user_id: Option<String>,
    /// Payload específico do tipo (TermPayload / NotePayload / SalaPayload).
    pub payload: serde_json::Value,
    /// Quem pode ver: federáveis = `public` / `universe_members`.
    pub visibility: Visibility,
    /// Quando o evento foi criado (UTC).
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Mirror local do `FederatedEvent` do CO-384 — envelope de federação que
/// embrulha um [`CoEvent`] com metadados de proveniência cross-deployment.
/// Serializa identicamente ao `co_web::eda::bridge::federated_event::FederatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FederatedEvent {
    /// Evento CO-380 original.
    pub event: CoEvent,
    /// Deployment de origem.
    pub origin_deployment: String,
    /// Identidade que assinou/encaminhou.
    pub signed_by: String,
    /// Quando este hop recebeu o evento.
    pub bridge_received_at: chrono::DateTime<chrono::Utc>,
    /// Saltos dados no bus (loop-guard: CO descarta se > 3).
    pub hop_count: u8,
}

impl FederatedEvent {
    /// Constrói o envelope a partir de um [`LoggedEvent`] (path + payload por fonte).
    pub fn from_log_entry(entry: &LoggedEvent, node_id: &str) -> Self {
        let note = &entry.note;
        let payload = build_note_payload(note);
        let co_event = CoEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: note.kind.event_type().to_string(),
            universe_key: Some(note.universe_key().to_string()),
            user_id: note.actor.clone(),
            payload,
            visibility: note.visibility.clone(),
            created_at: chrono::Utc::now(),
        };
        FederatedEvent {
            event: co_event,
            origin_deployment: ORIGIN_DEPLOYMENT.to_string(),
            signed_by: node_id.to_string(),
            bridge_received_at: chrono::Utc::now(),
            hop_count: 0,
        }
    }

    /// `true` se o evento pode ser federado ao hub.
    pub fn is_federatable(&self) -> bool {
        self.event.visibility.is_federatable()
    }

    /// Loop-guard: um evento recebido cuja origem é este deployment (echo do
    /// nosso próprio write, `hop_count > 0`) não deve ser reaplicado.
    pub fn is_own_echo(&self) -> bool {
        self.origin_deployment == ORIGIN_DEPLOYMENT && self.hop_count > 0
    }
}

/// Protocolo de fio — mirror do `BridgeMessage` do CO-384.
/// O Yggdrasil envia `Event`; na (re)conexão pode enviar `ReplayRequest`;
/// o hub responde com `Ack` para confirmar recebimento (fecha o loop de replay).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "bridge_msg_type")]
pub enum BridgeMessage {
    /// Enviado na (re)conexão para solicitar replay a partir do último id recebido.
    ReplayRequest {
        /// ULID do último evento recebido do CO. `None` = replay tudo.
        last_received_id: Option<String>,
    },
    /// Evento federado: inner event embrulhado em metadados de proveniência.
    Event {
        #[serde(flatten)]
        federated: FederatedEvent,
    },
    /// ACK enviado pelo hub após processar um evento — `event_id` é o `CoEvent.id`
    /// (UUID v4) do evento recebido.  O producer usa para avançar `last_delivered`.
    Ack { event_id: String },
}

impl BridgeMessage {
    /// Codifica como JSON (frame de texto do WS).
    pub fn encode(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Constrói um `BridgeMessage::Event` a partir de um [`LoggedEvent`].
    pub fn from_log_entry(entry: &LoggedEvent, node_id: &str) -> Self {
        BridgeMessage::Event {
            federated: FederatedEvent::from_log_entry(entry, node_id),
        }
    }
}

// ─── Payload tipados (CO-383/CO-389) ─────────────────────────────────────────

/// Payload de um **termo de léxico** federado (CO-389 TermPayload).
/// Path inclui `_users/<user-slug>/` para casar o filtro do consumer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TermPayload {
    /// Caminho no repo `comunicacao`: `<lingua>/terms/_users/<user-slug>/<slug>.md`.
    pub path: String,
    /// Frontmatter do termo como JSON (word, language_code, gloss, concept, …).
    pub frontmatter: serde_json::Value,
    /// Corpo Markdown da entrada.
    pub body: String,
    /// SHA-256 hex do `body` — permite dedup no consumer (CO-389).
    pub body_hash: String,
    /// Diretório de língua (`yoruba`, `guarani-mbya`, …) — campo de filtro/roteamento.
    pub lingua: String,
}

/// Payload de uma **nota de instância** federada (CO-383).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotePayload {
    /// Caminho instance-qualified: `instances/<id>/notes/<slug>.md`.
    pub path: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// SHA-256 hex do `body`.
    pub body_hash: String,
}

/// Payload de um evento de **atividade de sala** (`yggdrasil.sala.*`).
/// Sem mutação de conteúdo; apenas sinal de atividade p/ o `/agora` (CO-389).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalaPayload {
    pub sala_id: String,
    pub lingua: String,
    pub actor_id: String,
}

/// Constrói o `payload: serde_json::Value` correto por fonte de um [`NoteWritten`].
fn build_note_payload(note: &NoteWritten) -> serde_json::Value {
    let path = note.path();
    let body_hash = body_sha256(&note.body);
    match &note.source {
        FederatedSource::Comunicacao { lang, .. } => serde_json::to_value(TermPayload {
            path,
            frontmatter: note
                .frontmatter
                .clone()
                .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
            body: note.body.clone(),
            body_hash,
            lingua: lang.clone(),
        })
        .expect("TermPayload serialize"),
        FederatedSource::Notes => serde_json::to_value(NotePayload {
            path,
            title: note.title.clone(),
            body: note.body.clone(),
            updated_at: note.updated_at.clone(),
            body_hash,
        })
        .expect("NotePayload serialize"),
    }
}

// ─── Fonte (FederatedSource) ──────────────────────────────────────────────────

/// Fonte de um evento de conteúdo: define `universe_key` + a convenção de `path`.
/// É o que torna a *federated bus* multi-fonte sem um novo envelope (YG-103): o
/// mesmo [`BridgeMessage`] carrega notas **ou** termos, discriminados aqui.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum FederatedSource {
    /// Nota do editor de instâncias → universe `yggdrasil`,
    /// `instances/<instance>/notes/<slug>.md` (instance-qualified, YG-97).
    #[default]
    Notes,
    /// Termo de léxico de uma sala de comunicação → universe `comunicacao`,
    /// `<lang>/terms/_users/<user_slug>/<slug>.md` (YG-118, filtro `_users/`).
    Comunicacao {
        /// Diretório de língua (`yoruba`, `guarani-mbya`, …).
        lang: String,
        /// Slug do autor, derivado via `slugify(room.owner)`.
        user_slug: String,
    },
}

impl FederatedSource {
    /// `universe_key` no envelope, por fonte.
    pub fn universe_key(&self) -> &str {
        match self {
            FederatedSource::Notes => UNIVERSE_KEY,
            FederatedSource::Comunicacao { .. } => COMUNICACAO_UNIVERSE_KEY,
        }
    }

    /// `path` no envelope, por fonte.
    ///
    /// Notas: `instances/<instance>/notes/<slug>.md` (instance-qualified, YG-97).
    /// Termos: `<lang>/terms/_users/<user_slug>/<slug>.md` (YG-118, filtro `_users/`).
    pub fn path(&self, instance: &str, slug: &str) -> String {
        match self {
            FederatedSource::Notes => format!("instances/{instance}/notes/{slug}.md"),
            FederatedSource::Comunicacao { lang, user_slug } => {
                format!("{lang}/terms/_users/{user_slug}/{slug}.md")
            }
        }
    }
}

// ─── Canal interno (padrão broadcast do poker) ────────────────────────────────

/// Capacidade do canal `broadcast` interno de [`NoteWritten`].
pub const NOTE_CHANNEL_CAPACITY: usize = 1024;

/// Tipo de escrita que originou o evento de nota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Created,
    Updated,
    Deleted,
}

impl NoteKind {
    /// `event_type` do [`CoEvent`] correspondente.
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
    /// Id da instância (nota) ou da sala (termo de comunicação) de origem.
    pub instance: String,
    /// Slug canônico da nota/termo.
    pub slug: String,
    /// Created / Updated / Deleted.
    pub kind: NoteKind,
    /// Fonte do conteúdo: notas (default) ou termo de comunicação (YG-103).
    #[serde(default)]
    pub source: FederatedSource,
    /// Título (nota) / palavra (termo) no momento da escrita (vazio em `Deleted`).
    #[serde(default)]
    pub title: String,
    /// Corpo Markdown no momento da escrita (vazio em `Deleted`).
    #[serde(default)]
    pub body: String,
    /// `updated` (RFC3339) da nota, se conhecido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Frontmatter como JSON para termos de léxico (CO-389 TermPayload).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<serde_json::Value>,
    /// Ator (user_id) que originou a escrita; vira `CoEvent.user_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Visibilidade do evento (default = `Public`).
    #[serde(default)]
    pub visibility: Visibility,
}

impl NoteWritten {
    /// Constrói um evento de **nota** (fonte `Notes`) — o caso da YG-93.
    pub fn note(instance: impl Into<String>, slug: impl Into<String>, kind: NoteKind) -> Self {
        NoteWritten {
            instance: instance.into(),
            slug: slug.into(),
            kind,
            source: FederatedSource::Notes,
            title: String::new(),
            body: String::new(),
            updated_at: None,
            frontmatter: None,
            actor: None,
            visibility: Visibility::Public,
        }
    }

    /// `path` no envelope, derivado da [`FederatedSource`].
    pub fn path(&self) -> String {
        self.source.path(&self.instance, &self.slug)
    }

    /// `universe_key` no envelope, derivado da fonte.
    pub fn universe_key(&self) -> &str {
        self.source.universe_key()
    }
}

// ─── Atividade de sala (yggdrasil.sala.*) ────────────────────────────────────

/// Atividade de sala que **não** é conteúdo: publicação, entrada de usuário,
/// contribuição de termo. Vai para o `/agora` do CO via [`SalaPayload`].
/// Distinto do `entry.*` por design: o corpo do termo viaja como
/// [`BridgeMessage`]; *que uma sala foi publicada* é sinal de atividade,
/// efêmero, e não entra no `event_log` de replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomActivity {
    /// Sala passou a `published`.
    Published,
    /// Sala saiu de `published`. Aposentado do fio (CO-389 não o consome);
    /// serializa como `yggdrasil.sala.published` com `published: false` no payload.
    Unpublished,
    /// Usuário entrou numa sala (era `presence`, renomeado em YG-118).
    UserJoined,
    /// Termo contribuído pelo usuário numa sala (novo em YG-118).
    TermContributed,
}

impl RoomActivity {
    /// `event_type` para o fio do CO.
    /// `Unpublished` mapeia para `yggdrasil.sala.published` (aposentado do wire).
    pub fn event_type(self) -> &'static str {
        match self {
            RoomActivity::Published | RoomActivity::Unpublished => "yggdrasil.sala.published",
            RoomActivity::UserJoined => "yggdrasil.sala.user_joined",
            RoomActivity::TermContributed => "yggdrasil.sala.term_contributed",
        }
    }
}

/// Envelope de atividade de sala publicado no hub do CO. Compartilha a
/// auth/conexão da YG-93, mas tem um envelope distinto do `entry.*`:
/// sem `universe_key` — só *quem* fez *o quê* em *qual sala*, *quando*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityEvent {
    /// `yggdrasil.sala.{published,user_joined,term_contributed}`.
    pub event_type: String,
    /// Id da sala.
    pub room_id: String,
    /// Língua da sala.
    pub lang: String,
    /// Quem originou.
    pub actor: String,
    /// Deployment de origem.
    pub origin_deployment: String,
    /// Timestamp RFC3339, se conhecido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// `true` apenas para `Unpublished` mapeado como `published{published:false}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<bool>,
}

impl ObservabilityEvent {
    /// Constrói um evento de atividade de sala.
    pub fn room(activity: RoomActivity, room_id: &str, lang: &str, actor: &str) -> Self {
        let published = if activity == RoomActivity::Unpublished {
            Some(false)
        } else {
            None
        };
        ObservabilityEvent {
            event_type: activity.event_type().to_string(),
            room_id: room_id.to_string(),
            lang: lang.to_string(),
            actor: actor.to_string(),
            origin_deployment: ORIGIN_DEPLOYMENT.to_string(),
            at: None,
            published,
        }
    }

    /// Com timestamp RFC3339.
    pub fn at(mut self, ts: impl Into<String>) -> Self {
        self.at = Some(ts.into());
        self
    }

    /// Constrói o [`BridgeMessage::Event`] para envio ao hub.
    /// Eventos de sala **não** têm `universe_key` (atividade, não conteúdo).
    pub fn to_bridge_message(&self, node_id: &str) -> BridgeMessage {
        let mut payload = serde_json::json!({
            "sala_id": self.room_id,
            "lingua": self.lang,
            "actor_id": self.actor,
        });
        if let Some(p) = self.published {
            payload["published"] = serde_json::Value::Bool(p);
        }
        let co_event = CoEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: self.event_type.clone(),
            universe_key: None,
            user_id: Some(self.actor.clone()),
            payload,
            visibility: Visibility::Public,
            created_at: chrono::Utc::now(),
        };
        BridgeMessage::Event {
            federated: FederatedEvent {
                event: co_event,
                origin_deployment: ORIGIN_DEPLOYMENT.to_string(),
                signed_by: node_id.to_string(),
                bridge_received_at: chrono::Utc::now(),
                hop_count: 0,
            },
        }
    }

    /// Codifica como JSON (frame de texto do WS).
    pub fn encode(&self) -> Result<String, serde_json::Error> {
        self.to_bridge_message(ORIGIN_DEPLOYMENT).encode()
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

    /// Anexa um evento, devolvendo o id atribuído. Ids são monotônicos a partir de 1.
    pub fn append(&mut self, note: NoteWritten) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.events.push(LoggedEvent { id, note });
        id
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

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
    pub fn pending(&self) -> &[LoggedEvent] {
        let from = self.last_delivered.unwrap_or(0);
        let start = self.events.partition_point(|e| e.id <= from);
        &self.events[start..]
    }

    /// Semeia o log enumerando as notas já em disco de **todas** as instâncias,
    /// sintetizando um `entry.created` por nota. Devolve quantas notas semeou.
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
                    source: FederatedSource::Notes,
                    title: n.title,
                    body: n.body,
                    updated_at: Some(n.updated.to_rfc3339()),
                    frontmatter: None,
                    actor: None,
                    visibility: Visibility::Public,
                });
                seeded += 1;
            }
        }
        seeded
    }

    /// Semeia o log com os **termos de léxico** das salas de comunicação
    /// **publicadas** (YG-103, acceptance #3: cold-start backfill). Cada elemento
    /// vira um `entry.created` `universe_key=comunicacao`
    /// `path=<lang>/terms/_users/<user-slug>/<slug>.md` (YG-118).
    pub fn seed_comunicacao_from_store(&mut self, store: &RoomStore) -> usize {
        let rooms = match store.list_published() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("co-bridge seed comunicação: list_published falhou: {e}");
                return 0;
            }
        };
        let mut seeded = 0;
        for room in rooms {
            let user_slug = slugify(&room.owner);
            for elem in &room.elements {
                if let Some(ev) = comunicacao_term_event(&room, elem, &user_slug) {
                    self.append(ev);
                    seeded += 1;
                }
            }
        }
        seeded
    }
}

// ─── comunicacao_term_event ───────────────────────────────────────────────────

/// Constrói o [`NoteWritten`] de um termo de léxico (fonte `Comunicacao`),
/// resolvendo o **código** de língua (`yo`, `gn-mbya`) para o diretório canônico
/// (`yoruba`, `guarani-mbya`) via [`LexiconStore::lang_dir`] — o mesmo path do
/// write-back (YG-100). Agora inclui:
/// - `path = <lingua>/terms/_users/<user_slug>/<slug>.md` (YG-118, filtro `_users/`)
/// - `frontmatter` como JSON (CO-389 TermPayload)
/// - `actor = user_slug`
///
/// Devolve `None` se a língua não é suportada ou o slug fica vazio.
/// Compartilhado pelo backfill e pela emissão ao vivo (`comunicacao_routes`),
/// p/ path/corpo idênticos.
pub fn comunicacao_term_event(room: &Room, elem: &Element, user_slug: &str) -> Option<NoteWritten> {
    let code = if elem.lang.is_empty() {
        room.lang.as_str()
    } else {
        elem.lang.as_str()
    };
    let dir = LexiconStore::lang_dir(code)?;
    let slug = slugify(&elem.word);
    if slug.is_empty() {
        return None;
    }
    let lingua = dir.to_string();
    let body = term_body(elem);
    let frontmatter = term_frontmatter(elem, room, user_slug);
    Some(NoteWritten {
        instance: room.id.clone(),
        slug,
        kind: NoteKind::Created,
        source: FederatedSource::Comunicacao {
            lang: lingua,
            user_slug: user_slug.to_string(),
        },
        title: elem.word.clone(),
        body,
        updated_at: Some(room.updated_at.to_rfc3339()),
        frontmatter: Some(frontmatter),
        actor: Some(user_slug.to_string()),
        visibility: Visibility::Public,
    })
}

/// Constrói o frontmatter JSON de um termo a partir do [`Element`] da sala
/// (CO-389 TermPayload.frontmatter).
fn term_frontmatter(elem: &Element, room: &Room, user_slug: &str) -> serde_json::Value {
    let lang_code = if elem.lang.is_empty() {
        room.lang.as_str()
    } else {
        elem.lang.as_str()
    };
    let mut fm = serde_json::json!({
        "type": "term",
        "word": elem.word,
        "language_code": lang_code,
        "seed_status": "stub",
        "author": user_slug,
        "contributed_via": "yggdrasil-comunicacao",
    });
    if let Some(g) = elem.gloss.as_ref().filter(|s| !s.is_empty()) {
        fm["gloss"] = serde_json::Value::String(g.clone());
    }
    if let Some(c) = elem.concept.as_ref().filter(|s| !s.is_empty()) {
        fm["concept"] = serde_json::Value::String(c.clone());
    }
    if let Some(p) = elem.pronunciation.as_ref().filter(|s| !s.is_empty()) {
        fm["pronunciation"] = serde_json::Value::String(p.clone());
    }
    fm
}

/// Corpo Markdown de um termo a partir de um [`Element`] da sala.
fn term_body(elem: &Element) -> String {
    let mut parts = Vec::new();
    if let Some(g) = elem.gloss.as_ref().filter(|s| !s.is_empty()) {
        parts.push(g.clone());
    }
    if let Some(c) = elem.concept.as_ref().filter(|s| !s.is_empty()) {
        parts.push(format!("**Conceito:** {c}"));
    }
    if let Some(p) = elem.pronunciation.as_ref().filter(|s| !s.is_empty()) {
        parts.push(format!("*Pronúncia:* {p}"));
    }
    parts.join("\n\n")
}

// ─── Reconnect: backoff exponencial ──────────────────────────────────────────

pub const BACKOFF_MIN: Duration = Duration::from_secs(1);
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    Connecting,
    Connected,
}

/// Máquina de backoff exponencial 1s → 2s → 4s → … → 30s (teto).
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

    pub fn current(&self) -> Duration {
        self.current
    }

    pub fn fail(&mut self) -> Duration {
        let doubled = self.current.saturating_mul(2);
        self.current = doubled.min(BACKOFF_MAX);
        self.current
    }

    pub fn reset(&mut self) {
        self.current = BACKOFF_MIN;
    }
}

// ─── Config (gate de env) ────────────────────────────────────────────────────

pub const ORIGIN_DEPLOYMENT: &str = "yggdrasil.artelonga.com.br";

/// Config do producer, lida de `YGG_CO_BRIDGE_URL` + `YGG_CO_BRIDGE_TOKEN`.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub url: String,
    pub token: String,
    pub node_id: String,
}

impl BridgeConfig {
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

fn ws_request(
    config: &BridgeConfig,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, BridgeError> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    // CO-384 (`bridge_ws_handler`) lê `source` + `token` dos **query params**
    // (`Query<BridgeQuery>`), não do header — e só exige source na trust-list
    // (`CO_BRIDGE_TRUSTED_SOURCES`) + token não-vazio (sem JWKS/validação cripto).
    // Então o handshake precisa de `?source=<node_id>&token=<token>` na URL; o
    // Bearer fica só por compat futura. (YG-120 — alinhamento de handshake.)
    let sep = if config.url.contains('?') { '&' } else { '?' };
    let url = format!(
        "{}{}source={}&token={}",
        config.url,
        sep,
        urlq(&config.node_id),
        urlq(&config.token),
    );
    let mut req = url
        .as_str()
        .into_client_request()
        .map_err(|e| BridgeError::BadRequest(e.to_string()))?;
    let auth = format!("Bearer {}", config.token)
        .parse()
        .map_err(|_| BridgeError::BadRequest("token inválido p/ header".into()))?;
    req.headers_mut().insert("authorization", auth);
    let proto = "co.eda.bridge.v1"
        .parse()
        .map_err(|_| BridgeError::BadRequest("subprotocol inválido".into()))?;
    req.headers_mut().insert("sec-websocket-protocol", proto);
    Ok(req)
}

/// Percent-encode mínimo p/ um valor de query (espaço + os reservados que
/// quebram o parse da URL). Tokens/JWTs e hosts já são quase URL-safe; isto cobre
/// os poucos casos (`+`, `/`, `=`, `&`, `?`, espaço) sem puxar uma dep nova.
fn urlq(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ─── Spawn da task de fundo (gated) ──────────────────────────────────────────

pub struct Producer {
    sender: broadcast::Sender<NoteWritten>,
    obs_sender: broadcast::Sender<ObservabilityEvent>,
}

impl Producer {
    pub fn new() -> Self {
        let (sender, _rx) = broadcast::channel(NOTE_CHANNEL_CAPACITY);
        let (obs_sender, _orx) = broadcast::channel(NOTE_CHANNEL_CAPACITY);
        Self { sender, obs_sender }
    }

    pub fn sender(&self) -> broadcast::Sender<NoteWritten> {
        self.sender.clone()
    }

    pub fn obs_sender(&self) -> broadcast::Sender<ObservabilityEvent> {
        self.obs_sender.clone()
    }
}

impl Default for Producer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn(
    producer: &Producer,
    store: std::sync::Arc<InstanceStore>,
    rooms: std::sync::Arc<RoomStore>,
) -> bool {
    let Some(config) = BridgeConfig::from_env() else {
        tracing::info!(
            "co-bridge producer desligado (YGG_CO_BRIDGE_URL/TOKEN não configurados) — no-op"
        );
        return false;
    };
    let rx = producer.sender.subscribe();
    let obs_rx = producer.obs_sender.subscribe();
    tokio::spawn(run_loop(config, store, rooms, rx, obs_rx));
    true
}

async fn run_loop(
    config: BridgeConfig,
    store: std::sync::Arc<InstanceStore>,
    rooms: std::sync::Arc<RoomStore>,
    mut rx: broadcast::Receiver<NoteWritten>,
    mut obs_rx: broadcast::Receiver<ObservabilityEvent>,
) {
    let mut log = EventLog::new();
    let notes = log.seed_from_store(&store);
    let terms = log.seed_comunicacao_from_store(&rooms);
    tracing::info!(
        "co-bridge producer: {notes} nota(s) + {terms} termo(s) de comunicação \
         semeados no event_log; hub={}",
        config.url
    );

    let root = store.root().to_path_buf();
    let mut backoff = Backoff::new();
    loop {
        match connect_and_stream(&config, &mut log, &root, &mut rx, &mut obs_rx).await {
            Ok(()) => {
                backoff.reset();
            }
            Err(BridgeError::ChannelClosed) => {
                tracing::info!("co-bridge producer: canal interno fechado — encerrando loop");
                return;
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

/// Conecta ao hub CO-384, drena o `EventLog` pendente e faz streaming bidirecional.
///
/// Retorna `Ok(())` se a conexão encerrou de forma limpa (backoff reset);
/// retorna `Err` em qualquer falha de rede (backoff fail + reconnect no caller).
async fn connect_and_stream(
    config: &BridgeConfig,
    log: &mut EventLog,
    root: &std::path::Path,
    rx: &mut broadcast::Receiver<NoteWritten>,
    obs_rx: &mut broadcast::Receiver<ObservabilityEvent>,
) -> Result<(), BridgeError> {
    use futures_util::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use tokio_tungstenite::tungstenite::Message;

    let req = ws_request(config)?;
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .map_err(|e| BridgeError::Connect(e.to_string()))?;

    tracing::info!(
        "co-bridge producer: conectado ao hub {} (co.eda.bridge.v1)",
        config.url
    );

    // Mapeia CoEvent.id → log_id para correlacionar ACKs do hub.
    let mut pending_acks: HashMap<String, u64> = HashMap::new();

    // Drena eventos pendentes (cold-start = log inteiro; warm-reconnect = desde último ACK).
    let to_drain: Vec<(u64, BridgeMessage)> = log
        .pending()
        .iter()
        .map(|e| (e.id, BridgeMessage::from_log_entry(e, &config.node_id)))
        .collect();
    for (log_id, msg) in &to_drain {
        if let BridgeMessage::Event { federated } = msg {
            pending_acks.insert(federated.event.id.clone(), *log_id);
        }
        let json = msg
            .encode()
            .map_err(|e| BridgeError::Encode(e.to_string()))?;
        ws.send(Message::from(json))
            .await
            .map_err(|e| BridgeError::Send(e.to_string()))?;
    }
    tracing::debug!(
        "co-bridge producer: {} evento(s) drenados; aguardando tráfego",
        to_drain.len()
    );

    // Heartbeat: envia ping a cada 30s para manter a conexão viva.
    let hb_start = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut heartbeat = tokio::time::interval_at(hb_start, Duration::from_secs(30));

    loop {
        tokio::select! {
            frame = ws.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<BridgeMessage>(&text) {
                            Ok(BridgeMessage::Ack { event_id }) => {
                                if let Some(&log_id) = pending_acks.get(&event_id) {
                                    log.ack(log_id);
                                    pending_acks.retain(|_, v| *v > log_id);
                                    tracing::debug!("co-bridge producer: ACK log_id={log_id}");
                                }
                            }
                            Ok(BridgeMessage::Event { ref federated }) => {
                                let applied = crate::co_bridge_inbound::apply_inbound(root, federated);
                                tracing::debug!("co-bridge inbound: {:?}", applied);
                            }
                            Ok(BridgeMessage::ReplayRequest { .. }) => {}
                            Err(e) => {
                                tracing::warn!("co-bridge producer: frame inbound inválido: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        ws.send(Message::Pong(data))
                            .await
                            .map_err(|e| BridgeError::Send(e.to_string()))?;
                    }
                    Some(Ok(Message::Pong(_)))
                    | Some(Ok(Message::Binary(_)))
                    | Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("co-bridge producer: conexão encerrada pelo hub");
                        return Err(BridgeError::Disconnected);
                    }
                    Some(Err(e)) => {
                        return Err(BridgeError::Recv(e.to_string()));
                    }
                }
            }
            content = rx.recv() => {
                match content {
                    Ok(note) => {
                        let id = log.append(note);
                        if let Some(entry) = log.pending().iter().rev().find(|e| e.id == id) {
                            let msg = BridgeMessage::from_log_entry(entry, &config.node_id);
                            if let BridgeMessage::Event { federated } = &msg {
                                pending_acks.insert(federated.event.id.clone(), id);
                            }
                            let json = msg.encode().map_err(|e| BridgeError::Encode(e.to_string()))?;
                            ws.send(Message::from(json))
                                .await
                                .map_err(|e| BridgeError::Send(e.to_string()))?;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("co-bridge producer: canal de conteúdo lagou {n} evento(s)");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(BridgeError::ChannelClosed);
                    }
                }
            }
            obs = obs_rx.recv() => {
                match obs {
                    Ok(ev) => {
                        let msg = ev.to_bridge_message(&config.node_id);
                        let json = msg.encode().map_err(|e| BridgeError::Encode(e.to_string()))?;
                        ws.send(Message::from(json))
                            .await
                            .map_err(|e| BridgeError::Send(e.to_string()))?;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "co-bridge producer: canal de observability lagou {n} evento(s)"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(BridgeError::ChannelClosed);
                    }
                }
            }
            _ = heartbeat.tick() => {
                ws.send(Message::Ping(vec![].into()))
                    .await
                    .map_err(|e| BridgeError::Send(e.to_string()))?;
                tracing::debug!("co-bridge producer: heartbeat ping enviado");
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("canal interno de notas fechado")]
    ChannelClosed,
    #[error("request WS inválida: {0}")]
    BadRequest(String),
    #[error("erro de conexão WS: {0}")]
    Connect(String),
    #[error("erro de recepção WS: {0}")]
    Recv(String),
    #[error("erro de envio WS: {0}")]
    Send(String),
    #[error("erro de codificação de frame: {0}")]
    Encode(String),
    #[error("conexão WS encerrada pelo hub")]
    Disconnected,
}

#[allow(dead_code)]
fn note_disk_path(root: &std::path::Path, instance: &str, slug: &str) -> PathBuf {
    root.join(instance).join("notes").join(format!("{slug}.md"))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn note(instance: &str, slug: &str, kind: NoteKind) -> NoteWritten {
        NoteWritten {
            instance: instance.into(),
            slug: slug.into(),
            kind,
            source: FederatedSource::Notes,
            title: format!("Título {slug}"),
            body: format!("corpo de {slug} [[outra]]"),
            updated_at: Some("2026-06-06T00:00:00+00:00".into()),
            frontmatter: None,
            actor: None,
            visibility: Visibility::Public,
        }
    }

    // ── NoteKind / path helpers ──────────────────────────────────────────

    #[test]
    fn note_kind_event_type_mapeia_entry_verbs() {
        assert_eq!(NoteKind::Created.event_type(), "entry.created");
        assert_eq!(NoteKind::Updated.event_type(), "entry.updated");
        assert_eq!(NoteKind::Deleted.event_type(), "entry.deleted");
    }

    #[test]
    fn note_written_path_e_instance_qualified() {
        let n = note("inst-1", "minha-nota", NoteKind::Created);
        assert_eq!(n.path(), "instances/inst-1/notes/minha-nota.md");
    }

    // ── YG-114: contrato do Caderno do Ayvu Rapyta ───────────────────────

    #[test]
    fn verse_slug_e_estavel_e_sem_diacriticos() {
        assert_eq!(verse_slug("1", "2"), "1-2");
        assert_eq!(verse_slug("I", "12a"), "i-12a");
        assert_eq!(verse_slug("Capítulo 3", "Verso 4"), "capitulo-3-verso-4");
    }

    #[test]
    fn caderno_note_federa_no_path_contratado_co389() {
        let slug = verse_slug("1", "2");
        let ev = NoteWritten::note(AYVU_INSTANCE, &slug, NoteKind::Created);
        assert_eq!(ev.universe_key(), "yggdrasil");
        assert_eq!(ev.path(), "instances/ayvu-rapyta/notes/1-2.md");
        assert_eq!(ev.path(), FederatedSource::Notes.path(AYVU_INSTANCE, &slug));
    }

    // ── Wire: BridgeMessage formato CO-384 ──────────────────────────────

    #[test]
    fn bridge_message_note_tem_formato_correto() {
        let mut log = EventLog::new();
        log.append(note("inst-1", "n", NoteKind::Created));
        let entry = &log.pending()[0];
        let msg = BridgeMessage::from_log_entry(entry, "node-xyz");

        let BridgeMessage::Event { federated } = &msg else {
            panic!("esperava Event");
        };
        assert_eq!(federated.event.event_type, "entry.created");
        assert_eq!(federated.event.universe_key.as_deref(), Some("yggdrasil"));
        assert_eq!(federated.signed_by, "node-xyz");
        assert_eq!(federated.origin_deployment, ORIGIN_DEPLOYMENT);
        assert_eq!(federated.hop_count, 0);

        // payload é NotePayload com path + body_hash
        let payload = &federated.event.payload;
        assert_eq!(
            payload["path"].as_str().unwrap(),
            "instances/inst-1/notes/n.md"
        );
        assert_eq!(payload["title"].as_str().unwrap(), "Título n");
        assert!(
            payload["body_hash"]
                .as_str()
                .map(|h| h.len() == 64)
                .unwrap_or(false),
            "body_hash deve ser hex SHA-256 de 64 chars"
        );

        // round-trip JSON → BridgeMessage (CO-380 shape)
        let json = msg.encode().unwrap();
        assert!(json.contains("\"bridge_msg_type\":\"Event\""));
        assert!(json.contains("\"event_type\":\"entry.created\""));
        assert!(json.contains("\"universe_key\":\"yggdrasil\""));
        assert!(json.contains("\"hop_count\":0"));
        let back: BridgeMessage = serde_json::from_str(&json).unwrap();
        let BridgeMessage::Event {
            federated: back_fed,
        } = back
        else {
            panic!("round-trip: esperava Event");
        };
        assert_eq!(back_fed.event.event_type, "entry.created");
    }

    #[test]
    fn bridge_message_deleted_usa_entry_deleted() {
        let mut log = EventLog::new();
        log.append(note("inst-1", "n", NoteKind::Deleted));
        let msg = BridgeMessage::from_log_entry(&log.pending()[0], "node");
        let BridgeMessage::Event { federated } = msg else {
            panic!()
        };
        assert_eq!(federated.event.event_type, "entry.deleted");
    }

    #[test]
    fn loop_guard_descarta_proprio_echo() {
        let mut log = EventLog::new();
        log.append(note("inst-1", "n", NoteKind::Created));
        let mut fed = FederatedEvent::from_log_entry(&log.pending()[0], "node");
        assert!(!fed.is_own_echo(), "hop 0 da origem não é echo");
        fed.hop_count = 2;
        assert!(fed.is_own_echo());
        fed.origin_deployment = "co.artelonga.com.br".into();
        assert!(!fed.is_own_echo());
    }

    #[test]
    fn visibility_impede_federacao_de_privado() {
        let mut n = note("inst-1", "n", NoteKind::Created);
        n.visibility = Visibility::UserOnly;
        let mut log = EventLog::new();
        log.append(n);
        let fed = FederatedEvent::from_log_entry(&log.pending()[0], "node");
        assert!(!fed.is_federatable(), "UserOnly não é federável");

        let mut n2 = note("inst-1", "n2", NoteKind::Created);
        n2.visibility = Visibility::Public;
        let mut log2 = EventLog::new();
        log2.append(n2);
        let fed2 = FederatedEvent::from_log_entry(&log2.pending()[0], "node");
        assert!(fed2.is_federatable(), "Public é federável");
    }

    // ── body_sha256 ──────────────────────────────────────────────────────

    #[test]
    fn body_sha256_retorna_hex_de_64_chars() {
        let h = body_sha256("hello world");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        // idempotente: mesma entrada → mesmo hash
        assert_eq!(h, body_sha256("hello world"));
        // diferente de string vazia
        assert_ne!(h, body_sha256(""));
    }

    #[test]
    fn body_sha256_dois_bodies_distintos_hashs_distintos() {
        assert_ne!(body_sha256("a"), body_sha256("b"));
    }

    // ── event_log: append + offset ───────────────────────────────────────

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
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].id, 1);
        assert_eq!(pending[1].id, 2);
    }

    #[test]
    fn warm_reconnect_pending_comeca_apos_o_ultimo_ack() {
        let mut log = EventLog::new();
        for s in ["a", "b", "c", "d"] {
            log.append(note("i", s, NoteKind::Created));
        }
        log.ack(2);
        let pending = log.pending();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].id, 3);
        assert_eq!(pending[1].id, 4);
    }

    #[test]
    fn ack_e_monotonico_ignora_retrocesso() {
        let mut log = EventLog::new();
        for s in ["a", "b", "c"] {
            log.append(note("i", s, NoteKind::Created));
        }
        log.ack(3);
        log.ack(1);
        assert_eq!(log.last_delivered(), Some(3));
        assert!(log.pending().is_empty());
    }

    #[test]
    fn novos_appends_apos_ack_ficam_pendentes() {
        let mut log = EventLog::new();
        log.append(note("i", "a", NoteKind::Created));
        log.ack(1);
        assert!(log.pending().is_empty());
        log.append(note("i", "b", NoteKind::Updated));
        assert_eq!(log.pending().len(), 1);
        assert_eq!(log.pending()[0].id, 2);
    }

    // ── cold-start backfill: seed_from_store ─────────────────────────────

    use yggdrasil_core::instance::UniverseInstance;

    fn store_with_notes() -> (TempDir, std::sync::Arc<InstanceStore>) {
        let dir = TempDir::new().unwrap();
        let store = std::sync::Arc::new(InstanceStore::new(dir.path()).unwrap());
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

    #[test]
    fn seed_from_store_sintetiza_entry_created_por_nota_existente() {
        let (_dir, store) = store_with_notes();
        let mut log = EventLog::new();
        let seeded = log.seed_from_store(&store);
        assert_eq!(seeded, 3);
        let pending = log.pending();
        assert_eq!(pending.len(), 3);
        assert!(pending.iter().all(|e| e.note.kind == NoteKind::Created));

        let msg = BridgeMessage::from_log_entry(&pending[0], "n");
        let BridgeMessage::Event { federated } = msg else {
            panic!()
        };
        assert!(
            federated.event.payload["path"]
                .as_str()
                .unwrap()
                .starts_with("instances/")
        );
        assert!(
            federated.event.payload["body_hash"]
                .as_str()
                .map(|h| h.len() == 64)
                .unwrap_or(false)
        );
        assert!(federated.event.payload["updated_at"].is_string());
    }

    #[test]
    fn seed_de_store_vazio_nao_semeia_nada() {
        let dir = TempDir::new().unwrap();
        let store = InstanceStore::new(dir.path()).unwrap();
        let mut log = EventLog::new();
        assert_eq!(log.seed_from_store(&store), 0);
    }

    #[test]
    fn seed_depois_append_continua_monotonico() {
        let (_dir, store) = store_with_notes();
        let mut log = EventLog::new();
        log.seed_from_store(&store);
        let next = log.append(note("inst-a", "novo", NoteKind::Created));
        assert_eq!(next, 4);
    }

    // ── backoff exponencial ──────────────────────────────────────────────

    #[test]
    fn backoff_dobra_ate_o_teto_de_30s() {
        let mut b = Backoff::new();
        assert_eq!(b.current(), Duration::from_secs(1));
        assert_eq!(b.fail(), Duration::from_secs(2));
        assert_eq!(b.fail(), Duration::from_secs(4));
        assert_eq!(b.fail(), Duration::from_secs(8));
        assert_eq!(b.fail(), Duration::from_secs(16));
        assert_eq!(b.fail(), Duration::from_secs(30));
        assert_eq!(b.fail(), Duration::from_secs(30));
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

    // ── config gate ──────────────────────────────────────────────────────

    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn config_from_env_none_sem_url_ou_token() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("YGG_CO_BRIDGE_URL");
            std::env::remove_var("YGG_CO_BRIDGE_TOKEN");
        }
        assert!(BridgeConfig::from_env().is_none());
        unsafe {
            std::env::set_var("YGG_CO_BRIDGE_URL", "wss://co.test/api/v1/events/bridge");
        }
        assert!(BridgeConfig::from_env().is_none());
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
        assert_eq!(cfg.node_id, ORIGIN_DEPLOYMENT);
        unsafe {
            std::env::remove_var("YGG_CO_BRIDGE_URL");
            std::env::remove_var("YGG_CO_BRIDGE_TOKEN");
        }
    }

    #[test]
    fn ws_request_inclui_source_token_query_bearer_e_subprotocol() {
        let cfg = BridgeConfig {
            url: "wss://co.test/api/v1/events/bridge".into(),
            token: "service-jwt-abc".into(),
            node_id: "yggdrasil.artelonga.com.br".into(),
        };
        let req = ws_request(&cfg).expect("request válida");
        // CO-384 lê `?source=&token=` da query (não do header) — YG-120.
        let uri = req.uri().to_string();
        assert_eq!(
            uri,
            "wss://co.test/api/v1/events/bridge?source=yggdrasil.artelonga.com.br&token=service-jwt-abc"
        );
        let auth = req.headers().get("authorization").unwrap();
        assert_eq!(auth, "Bearer service-jwt-abc");
        let proto = req.headers().get("sec-websocket-protocol").unwrap();
        assert_eq!(proto, "co.eda.bridge.v1");
    }

    #[test]
    fn ws_request_anexa_query_com_amp_se_url_ja_tem_query() {
        let cfg = BridgeConfig {
            url: "wss://co.test/api/v1/events/bridge?x=1".into(),
            token: "t/b+c=".into(),
            node_id: "ygg".into(),
        };
        let uri = ws_request(&cfg).unwrap().uri().to_string();
        // `&` quando já há query; token reservado é percent-encoded.
        assert_eq!(
            uri,
            "wss://co.test/api/v1/events/bridge?x=1&source=ygg&token=t%2Fb%2Bc%3D"
        );
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
        let rooms = std::sync::Arc::new(RoomStore::new(dir.path()).unwrap());
        let producer = Producer::new();
        assert!(!spawn(&producer, store, rooms));
    }

    // ── YG-103: fonte comunicação ────────────────────────────────────────

    #[test]
    fn source_notes_mapeia_yggdrasil_e_path_instance_qualified() {
        let s = FederatedSource::Notes;
        assert_eq!(s.universe_key(), "yggdrasil");
        assert_eq!(
            s.path("inst-1", "minha-nota"),
            "instances/inst-1/notes/minha-nota.md"
        );
    }

    #[test]
    fn source_comunicacao_mapeia_universe_comunicacao_e_users_path() {
        let s = FederatedSource::Comunicacao {
            lang: "yoruba".into(),
            user_slug: "alice".into(),
        };
        assert_eq!(s.universe_key(), "comunicacao");
        assert_eq!(s.path("sala-1", "ase"), "yoruba/terms/_users/alice/ase.md");
    }

    // ── YG-118: TermPayload, path _users/, body_hash, frontmatter ────────

    use yggdrasil_core::comunicacao::room::{Element, Room};

    fn room_with_terms(id: &str, lang: &str, published: bool, words: &[&str]) -> Room {
        let mut room = Room::empty(id, "alice@test", "Sala", lang);
        room.published = published;
        for (i, w) in words.iter().enumerate() {
            room.elements
                .push(Element::new(format!("e{i}"), *w, lang).with_gloss(format!("gloss de {w}")));
        }
        room
    }

    #[test]
    fn termo_publicado_gera_term_payload_com_users_path_e_body_hash() {
        let room = room_with_terms("sala-1", "yo", true, &["ase"]);
        let elem = &room.elements[0];
        let user_slug = "alice-test";
        let ev = comunicacao_term_event(&room, elem, user_slug).unwrap();

        // universe_key e path corretos (YG-118 acceptance #1)
        assert_eq!(ev.universe_key(), "comunicacao");
        assert_eq!(ev.path(), "yoruba/terms/_users/alice-test/ase.md");
        assert!(ev.path().contains("_users/"), "path deve ter _users/");
        assert_eq!(ev.kind, NoteKind::Created);

        // constrói BridgeMessage e verifica payload (TermPayload shape)
        let mut log = EventLog::new();
        log.append(ev);
        let msg = BridgeMessage::from_log_entry(&log.pending()[0], "n");
        let BridgeMessage::Event { federated } = msg else {
            panic!()
        };

        assert_eq!(federated.event.event_type, "entry.created");
        assert_eq!(federated.event.universe_key.as_deref(), Some("comunicacao"));
        let payload = &federated.event.payload;
        assert_eq!(
            payload["path"].as_str().unwrap(),
            "yoruba/terms/_users/alice-test/ase.md"
        );
        assert!(
            payload["body_hash"]
                .as_str()
                .map(|h| h.len() == 64)
                .unwrap_or(false)
        );
        assert!(
            payload["frontmatter"].is_object(),
            "frontmatter deve ser JSON object"
        );
        assert_eq!(payload["lingua"].as_str().unwrap(), "yoruba");

        // round-trip: desserializa como BridgeMessage (CO-380 shape)
        let json = BridgeMessage::from_log_entry(&log.pending()[0], "n")
            .encode()
            .unwrap();
        assert!(json.contains("\"bridge_msg_type\":\"Event\""));
        assert!(json.contains("\"event_type\":\"entry.created\""));
        assert!(json.contains("_users/alice-test/ase.md"));
        let back: BridgeMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, BridgeMessage::Event { .. }));
    }

    #[test]
    fn nota_de_instancia_payload_inclui_body_hash() {
        let mut log = EventLog::new();
        log.append(NoteWritten {
            instance: "inst-x".into(),
            slug: "minha-nota".into(),
            kind: NoteKind::Updated,
            source: FederatedSource::Notes,
            title: "Minha Nota".into(),
            body: "corpo da nota".into(),
            updated_at: Some("2026-06-08T00:00:00+00:00".into()),
            frontmatter: None,
            actor: None,
            visibility: Visibility::Public,
        });
        let msg = BridgeMessage::from_log_entry(&log.pending()[0], "n");
        let BridgeMessage::Event { federated } = msg else {
            panic!()
        };

        // YG-118 acceptance #2: universe_key=yggdrasil, path instance-qualified
        assert_eq!(federated.event.universe_key.as_deref(), Some("yggdrasil"));
        let payload = &federated.event.payload;
        assert_eq!(
            payload["path"].as_str().unwrap(),
            "instances/inst-x/notes/minha-nota.md"
        );
        // body_hash presente
        let hash = payload["body_hash"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, body_sha256("corpo da nota"));
    }

    #[test]
    fn sala_events_tem_payload_correto() {
        // YG-118 acceptance #3: yggdrasil.sala.{published,term_contributed,user_joined}
        for (activity, expected_type) in [
            (RoomActivity::Published, "yggdrasil.sala.published"),
            (
                RoomActivity::TermContributed,
                "yggdrasil.sala.term_contributed",
            ),
            (RoomActivity::UserJoined, "yggdrasil.sala.user_joined"),
        ] {
            let ev = ObservabilityEvent::room(activity, "sala-1", "yoruba", "alice");
            let json = ev.encode().unwrap();
            let msg: BridgeMessage = serde_json::from_str(&json).unwrap();
            let BridgeMessage::Event { federated } = msg else {
                panic!("expected Event for {expected_type}");
            };
            assert_eq!(federated.event.event_type, expected_type);
            assert!(
                federated.event.universe_key.is_none(),
                "sala events não têm universe_key"
            );
            let payload = &federated.event.payload;
            assert_eq!(payload["sala_id"].as_str().unwrap(), "sala-1");
            assert_eq!(payload["lingua"].as_str().unwrap(), "yoruba");
            assert_eq!(payload["actor_id"].as_str().unwrap(), "alice");
        }
    }

    #[test]
    fn unpublished_mapeia_para_published_com_published_false() {
        let ev = ObservabilityEvent::room(RoomActivity::Unpublished, "sala-1", "yoruba", "alice");
        let json = ev.encode().unwrap();
        let msg: BridgeMessage = serde_json::from_str(&json).unwrap();
        let BridgeMessage::Event { federated } = msg else {
            panic!()
        };
        assert_eq!(federated.event.event_type, "yggdrasil.sala.published");
        assert_eq!(federated.event.payload["published"].as_bool(), Some(false));
    }

    // ── YG-103: cold-start backfill termos ───────────────────────────────

    #[test]
    fn seed_comunicacao_so_inclui_salas_publicadas() {
        let dir = TempDir::new().unwrap();
        let store = RoomStore::new(dir.path()).unwrap();
        store
            .save(&room_with_terms("pub", "yo", true, &["ase", "ori"]))
            .unwrap();
        store
            .save(&room_with_terms("priv", "gn-mbya", false, &["nhande"]))
            .unwrap();

        let mut log = EventLog::new();
        let seeded = log.seed_comunicacao_from_store(&store);
        assert_eq!(seeded, 2);
        let pending = log.pending();
        assert!(
            pending
                .iter()
                .all(|e| e.note.universe_key() == "comunicacao")
        );
        // path deve conter _users/
        assert!(pending.iter().all(|e| e.note.path().contains("_users/")));
        let paths: std::collections::BTreeSet<_> = pending.iter().map(|e| e.note.path()).collect();
        // alice@test → slugify → "alice-test"
        assert!(paths.iter().any(|p| p.contains("yoruba/terms/_users/")));
    }

    #[test]
    fn seed_comunicacao_e_notas_coexistem_no_mesmo_log() {
        let dir = TempDir::new().unwrap();
        let istore = std::sync::Arc::new(InstanceStore::new(dir.path()).unwrap());
        let inst = UniverseInstance::empty("inst-x", "o@test", "X");
        istore.save(&inst).unwrap();
        NoteStore::for_instance(istore.root(), "inst-x")
            .save("uma", "Uma", "corpo")
            .unwrap();
        let rstore = RoomStore::new(dir.path()).unwrap();
        rstore
            .save(&room_with_terms("pub", "yo", true, &["ase"]))
            .unwrap();

        let mut log = EventLog::new();
        let n = log.seed_from_store(&istore);
        let t = log.seed_comunicacao_from_store(&rstore);
        assert_eq!((n, t), (1, 1));
        assert_eq!(log.len(), 2);
        let universes: std::collections::BTreeSet<_> = log
            .pending()
            .iter()
            .map(|e| e.note.universe_key())
            .collect();
        assert_eq!(
            universes,
            ["comunicacao", "yggdrasil"].into_iter().collect()
        );
    }

    // ── YG-103: observability de sala (RoomActivity) ─────────────────────

    #[test]
    fn room_activity_event_types_namespaced() {
        assert_eq!(
            RoomActivity::Published.event_type(),
            "yggdrasil.sala.published"
        );
        assert_eq!(
            RoomActivity::UserJoined.event_type(),
            "yggdrasil.sala.user_joined"
        );
        assert_eq!(
            RoomActivity::TermContributed.event_type(),
            "yggdrasil.sala.term_contributed"
        );
        assert_eq!(
            RoomActivity::Unpublished.event_type(),
            "yggdrasil.sala.published"
        );
    }

    #[test]
    fn note_written_compat_serde_sem_source_defaulta_notes() {
        let legacy = r#"{"instance":"i","slug":"n","kind":"created"}"#;
        let n: NoteWritten = serde_json::from_str(legacy).unwrap();
        assert_eq!(n.source, FederatedSource::Notes);
        assert_eq!(n.universe_key(), "yggdrasil");
        assert_eq!(n.path(), "instances/i/notes/n.md");
    }

    // ── YG-119: integração WS mock — handshake + drain + ACK ────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn integracao_ws_mock_handshake_drain_e_ack() {
        use futures_util::{SinkExt, StreamExt};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use tokio::net::TcpListener;
        use tokio_tungstenite::tungstenite::{
            Message,
            handshake::server::{Request as WsRequest, Response as WsResponse},
        };

        let subprotocol_ok = Arc::new(AtomicBool::new(false));
        let subprotocol_clone = subprotocol_ok.clone();
        let received: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(vec![]));
        let received_clone = received.clone();

        // Porta efêmera em 127.0.0.1 (sem exposição externa)
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Servidor WS mock: verifica subprotocol, drena eventos, envia ACK
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |req: &WsRequest, mut resp: WsResponse| {
                    if req
                        .headers()
                        .get("sec-websocket-protocol")
                        .map(|v| v.as_bytes() == b"co.eda.bridge.v1")
                        .unwrap_or(false)
                    {
                        subprotocol_clone.store(true, Ordering::SeqCst);
                        resp.headers_mut().insert(
                            "sec-websocket-protocol",
                            "co.eda.bridge.v1".parse().unwrap(),
                        );
                    }
                    Ok(resp)
                },
            )
            .await
            .unwrap();

            let (mut sink, mut stream) = ws.split();
            let mut acked = 0usize;

            while let Some(Ok(msg)) = stream.next().await {
                match msg {
                    Message::Text(text) => {
                        let m: BridgeMessage = serde_json::from_str(&text).unwrap();
                        if let BridgeMessage::Event { ref federated } = m {
                            received_clone
                                .lock()
                                .unwrap()
                                .push(federated.event.event_type.clone());
                            let ack = BridgeMessage::Ack {
                                event_id: federated.event.id.clone(),
                            };
                            sink.send(Message::from(ack.encode().unwrap()))
                                .await
                                .unwrap();
                            acked += 1;
                            if acked >= 2 {
                                sink.send(Message::Close(None)).await.ok();
                                break;
                            }
                        }
                    }
                    Message::Ping(data) => {
                        sink.send(Message::Pong(data)).await.ok();
                    }
                    _ => {}
                }
            }
        });

        // Cliente aponta para o servidor mock
        let cfg = BridgeConfig {
            url: format!("ws://127.0.0.1:{port}/"),
            token: "test-jwt".into(),
            node_id: "test-node".into(),
        };
        let mut log = EventLog::new();
        log.append(note("inst-1", "a", NoteKind::Created));
        log.append(note("inst-1", "b", NoteKind::Updated));

        let dir = TempDir::new().unwrap();
        let (_tx, mut rx) = broadcast::channel::<NoteWritten>(16);
        let (_obs_tx, mut obs_rx) = broadcast::channel::<ObservabilityEvent>(16);

        let result = connect_and_stream(&cfg, &mut log, dir.path(), &mut rx, &mut obs_rx).await;

        // Servidor fechou após ACKar os 2 eventos
        assert!(
            matches!(
                result,
                Err(BridgeError::Disconnected) | Err(BridgeError::Recv(_))
            ),
            "esperava Disconnected ou Recv, got: {result:?}"
        );

        // Ambos os eventos foram ACK'd no log
        assert_eq!(log.last_delivered(), Some(2), "log_id 2 deve estar ACK'd");

        server.await.unwrap();

        // Subprotocol correto no handshake
        assert!(
            subprotocol_ok.load(Ordering::SeqCst),
            "sec-websocket-protocol: co.eda.bridge.v1 deve estar presente no handshake"
        );

        // Servidor recebeu os 2 eventos drenados na ordem correta
        let events = received.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "entry.created");
        assert_eq!(events[1], "entry.updated");
    }
    // temp diagnostic appended to the test module

    #[tokio::test]
    async fn dial_compativel_com_axum_websocket_upgrade() {
        use axum::Router;
        use axum::extract::ws::{WebSocket, WebSocketUpgrade};
        use axum::response::Response;
        use axum::routing::get;
        use tokio::net::TcpListener;

        async fn h(ws: WebSocketUpgrade) -> Response {
            ws.protocols(["co.eda.bridge.v1"])
                .on_upgrade(|mut s: WebSocket| async move {
                    let _ = s.recv().await;
                })
        }
        let app = Router::new().route("/api/v1/events/bridge", get(h));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let cfg = BridgeConfig {
            url: format!("ws://{addr}/api/v1/events/bridge"),
            token: "tok".into(),
            node_id: "yggdrasil.artelonga.com.br".into(),
        };
        let req = ws_request(&cfg).unwrap();
        let result = tokio_tungstenite::connect_async(req).await;
        assert!(
            result.is_ok(),
            "axum recusou o handshake: {:?}",
            result.err()
        );
    }
}
