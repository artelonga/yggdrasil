//! Universo **comunicação** — salas interativas de léxico cross-linguístico.
//!
//! Cada usuário tem salas (mapas navegáveis: pan/zoom) onde posiciona
//! [`Element`]s — palavras em língua nativa (Iorubá, Mbyá Guaraní, …) com
//! glosa, pronúncia, conceito âncora e referências. Publicar um elemento
//! **liga-o** a uma entrada já existente do léxico compartilhado, ou **cria**
//! uma entrada nova atribuída ao usuário no repositório `comunicacao`
//! ([`lexicon`]). Termos publicados entram numa fila de revisão espaçada
//! ([`review`]).
//!
//! Universo auto-contido: depende só de `serde`/`chrono` e da auth do
//! `yggdrasil-web` (via `sub` do JWT). Não acopla ao engine WASM nem ao
//! editor de instâncias (YG-73) — reusa apenas os mesmos princípios de
//! persistência (disco como fonte da verdade, escrita atômica).

pub mod caderno;
pub mod lexicon;
pub mod pack;
pub mod public;
pub mod review;
pub mod room;
pub mod score;
pub mod store;
pub mod templates;
pub mod writeback;

pub use caderno::{Caderno, CadernoNote, CadernoStore};
pub use lexicon::{Contribution, LexiconError, LexiconScope, LexiconStore, Promotion};
pub use pack::{
    Adsr, EntropyStats, EntryAudio, LexiconPack, PackEntry, PackKind, PackSummary, Relation,
    Waveform, note_freq, seed_pack, seed_packs, seed_summaries,
};
pub use public::{PUBLIC_MBYA, PUBLIC_YORUBA, SYSTEM_OWNER, ensure_public_rooms, is_public_id};
pub use review::{ReviewItem, ReviewQueue};
pub use room::{EditError, Element, LexiconLink, Link, Reference, Room, RoomEdit, Viewport};
pub use score::{BitsLedger, BitsLedgerStore, pack_bits_per_symbol};
pub use store::{RoomStore, StoreError};
pub use templates::{TemplateSummary, instantiate as template_instantiate, template_summaries};
pub use writeback::{Writeback, WritebackConfig, WritebackError, WritebackOutcome};
