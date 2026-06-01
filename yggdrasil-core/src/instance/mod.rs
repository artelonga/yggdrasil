//! Editor de universos data-driven (epic YG-73).
//!
//! Conceito **paralelo** ao catálogo de engines compiladas ([`crate::universes`]):
//! uma [`UniverseInstance`] é dado puro autorado por usuário, renderizado por um
//! player genérico client-side — sem WASM, sem tick.
//!
//! - [`schema`] — o formato serializado (`UniverseInstance`, `Layer`, `Block`…).
//! - [`edit`] — [`EditOp`] e sua aplicação/validação granular.
//! - [`store`] — persistência em disco + anexos content-addressed.
//! - [`template`] — instâncias-semente (blank, neuroanatomia).

pub mod edit;
pub mod schema;
pub mod store;
pub mod template;

pub use edit::{EditError, EditOp};
pub use schema::{
    AttachmentKind, Block, Cell, Connection, ContentRef, GridSpec, Layer, LayerKind, Projection,
    SCHEMA_VERSION, UniverseInstance,
};
pub use store::{InstanceStore, StoreError};
pub use template::{
    Template, TemplateSummary, all_templates, blank_template, neuroanatomia_template,
    template_by_slug, template_summaries,
};
