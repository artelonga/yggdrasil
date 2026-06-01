//! Sala de comunicação — universo autorado por usuário, focado em léxico
//! cross-linguístico (Mbyá Guaraní × Iorubá × Português).
//!
//! Diferente da [`crate::instance::UniverseInstance`] (grade densa, anatomia),
//! a sala é um **mapa livre** que o usuário navega (pan/zoom) e onde posiciona
//! [`Element`]s — palavras numa língua nativa com glosa, pronúncia, conceito
//! âncora e referências. Cada elemento que o usuário "publica" vira (ou liga-se
//! a) uma entrada de léxico no repositório `comunicacao` — ver
//! [`crate::comunicacao::lexicon`].
//!
//! Coordenadas são em **unidades de mundo** (`f64`), não células: o mapa é
//! contínuo e o cliente projeta mundo→tela via [`Viewport`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Versão do schema serializado da sala. Incrementar em migração incompatível.
pub const SCHEMA_VERSION: u32 = 1;

/// Uma sala interativa de um usuário.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Room {
    /// Id estável (nanoid); também o nome do diretório em disco.
    pub id: String,
    pub schema_version: u32,
    /// `sub` do JWT do dono.
    pub owner: String,
    pub title: String,
    /// Língua primária da sala (`language_code`: `gn-mbya`, `yo`, `pt`…).
    /// Define o `terms/` default ao publicar elementos sem língua própria.
    pub lang: String,
    /// Slug do template-semente (`"yoruba"`, `"mbya"`, `"blank"`).
    #[serde(default)]
    pub template: String,
    /// Estado de câmera persistido — restaura pan/zoom do usuário.
    #[serde(default)]
    pub viewport: Viewport,
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Relações (links) entre elementos — o significado emerge das relações.
    #[serde(default)]
    pub links: Vec<Link>,
    /// Visível no feed público (`?published=true`).
    #[serde(default)]
    pub published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Room {
    /// Sala vazia com viewport default.
    pub fn empty(
        id: impl Into<String>,
        owner: impl Into<String>,
        title: impl Into<String>,
        lang: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            schema_version: SCHEMA_VERSION,
            owner: owner.into(),
            title: title.into(),
            lang: lang.into(),
            template: String::new(),
            viewport: Viewport::default(),
            elements: Vec::new(),
            links: Vec::new(),
            published: false,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    pub fn has_element(&self, id: &str) -> bool {
        self.elements.iter().any(|e| e.id == id)
    }

    pub fn has_link(&self, id: &str) -> bool {
        self.links.iter().any(|l| l.id == id)
    }
}

/// Estado de câmera 2D do mapa. `zoom` é multiplicativo (1.0 = 100%).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

/// Limites de zoom aceitos (evita NaN/zoom degenerado vindo do cliente).
pub const ZOOM_MIN: f64 = 0.1;
pub const ZOOM_MAX: f64 = 8.0;

/// Um elemento posicionado no mapa: uma palavra numa língua, com metadados
/// que a ligam ao léxico cross-linguístico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// Id estável dentro da sala.
    pub id: String,
    /// Coordenada de mundo (não pixels nem células).
    pub x: f64,
    pub y: f64,
    /// Palavra/composto na língua nativa (preserva diacríticos: `àṣẹ`, `nhe'ẽ`).
    pub word: String,
    /// `language_code` desta palavra. Default = língua da sala.
    pub lang: String,
    /// Pronúncia (IPA ou aproximação).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronunciation: Option<String>,
    /// Glosa / tradução / definição curta (PT-BR).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    /// Conceito âncora language-agnostic, ex. `co://concepts/water.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept: Option<String>,
    /// Ícone do elemento (slug/emoji/SVG ref) — opcional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Referências (citações, vídeos, fontes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Reference>,
    /// Situação no léxico compartilhado.
    #[serde(default)]
    pub lexicon: LexiconLink,
}

impl Element {
    /// Constrói um elemento mínimo (usado pelos templates e pelo cliente).
    pub fn new(id: impl Into<String>, word: impl Into<String>, lang: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            x: 0.0,
            y: 0.0,
            word: word.into(),
            lang: lang.into(),
            pronunciation: None,
            gloss: None,
            concept: None,
            icon: None,
            refs: Vec::new(),
            lexicon: LexiconLink::Local,
        }
    }

    pub fn at(mut self, x: f64, y: f64) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn with_gloss(mut self, gloss: impl Into<String>) -> Self {
        self.gloss = Some(gloss.into());
        self
    }

    pub fn with_pronunciation(mut self, p: impl Into<String>) -> Self {
        self.pronunciation = Some(p.into());
        self
    }

    pub fn with_concept(mut self, c: impl Into<String>) -> Self {
        self.concept = Some(c.into());
        self
    }

    pub fn with_ref(mut self, r: Reference) -> Self {
        self.refs.push(r);
        self
    }
}

/// Citação/fonte de um elemento.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub url: String,
    pub source: String,
    /// `youtube`, `article`, `book`, `dictionary`… (livre).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Reference {
    pub fn new(url: impl Into<String>, source: impl Into<String>, kind: &str) -> Self {
        Self {
            url: url.into(),
            source: source.into(),
            kind: Some(kind.to_string()),
        }
    }
}

/// Relação (link) entre dois elementos da sala. O significado emerge das
/// relações entre termos — `from`/`to` referenciam [`Element::id`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// Id estável dentro da sala.
    pub id: String,
    /// [`Element::id`] de origem.
    pub from: String,
    /// [`Element::id`] de destino.
    pub to: String,
    /// Rótulo da relação (ex. `compõe`, `funda`, `paralelo`, `relacionado`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `true` desenha seta from→to; `false` é relação simétrica.
    #[serde(default = "default_true_link")]
    pub directed: bool,
    /// Tipo da relação: `None`/"relacao" = livre; "compoe" = composição
    /// (from é composto por to → estrutura fractal/etimológica).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

fn default_true_link() -> bool {
    true
}

/// Marca de link composicional (termo `from` é composto pelo morfema `to`).
pub const KIND_COMPOSE: &str = "compoe";

impl Link {
    pub fn new(id: impl Into<String>, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            label: None,
            directed: true,
            kind: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn undirected(mut self) -> Self {
        self.directed = false;
        self
    }

    pub fn is_compose(&self) -> bool {
        self.kind.as_deref() == Some(KIND_COMPOSE)
    }
}

/// Ligação de um elemento ao léxico compartilhado do repo `comunicacao`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LexiconLink {
    /// Ainda só existe na sala do usuário; não foi publicado no léxico.
    #[default]
    Local,
    /// Liga-se a uma entrada **já existente** no léxico compartilhado.
    Linked { path: String },
    /// Criou uma entrada nova no léxico, atribuída a este usuário.
    Contributed { path: String },
}

impl LexiconLink {
    /// Caminho relativo da entrada de léxico, se houver.
    pub fn path(&self) -> Option<&str> {
        match self {
            LexiconLink::Local => None,
            LexiconLink::Linked { path } | LexiconLink::Contributed { path } => Some(path),
        }
    }
}

// ─── Operações de edição (PATCH) ──────────────────────────────────────────────

/// Falhas ao aplicar uma [`RoomEdit`].
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditError {
    #[error("elemento '{0}' já existe")]
    ElementExists(String),
    #[error("elemento '{0}' não encontrado")]
    ElementNotFound(String),
    #[error("palavra vazia")]
    EmptyWord,
    #[error("zoom inválido: {0}")]
    InvalidZoom(f64),
    #[error("coordenada não-finita")]
    NonFiniteCoord,
    #[error("link '{0}' já existe")]
    LinkExists(String),
    #[error("link '{0}' não encontrado")]
    LinkNotFound(String),
    #[error("link referencia elemento inexistente: '{0}'")]
    LinkEndpointMissing(String),
    #[error("link não pode ligar um elemento a si mesmo")]
    SelfLink,
    #[error("composição precisa de ao menos um termo-parente")]
    ComposeNoParents,
}

/// Patch atômico sobre a sala. Tagged por `"op"` no JSON (mesmo padrão do
/// editor de instâncias).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RoomEdit {
    /// Adiciona um elemento novo (id deve ser único na sala).
    AddElement { element: Element },
    /// Move um elemento para nova coordenada de mundo.
    MoveElement { id: String, x: f64, y: f64 },
    /// Edita campos textuais de um elemento (campos `None` ficam inalterados).
    EditElement {
        id: String,
        #[serde(default)]
        word: Option<String>,
        #[serde(default)]
        gloss: Option<String>,
        #[serde(default)]
        pronunciation: Option<String>,
        #[serde(default)]
        concept: Option<String>,
        #[serde(default)]
        icon: Option<String>,
    },
    /// Remove um elemento (e em cascata os links que o referenciam).
    DeleteElement { id: String },
    /// Cria uma relação entre dois elementos.
    AddLink { link: Link },
    /// Remove uma relação.
    DeleteLink { id: String },
    /// Compõe um termo novo a partir de elementos existentes (estrutura
    /// fractal/etimológica): cria o elemento `word` e um link `compoe` dele
    /// para cada parente (ex.: `ayvu rapyta` ← `ayvu` + `rapyta`).
    Compose {
        id: String,
        word: String,
        #[serde(default)]
        lang: Option<String>,
        x: f64,
        y: f64,
        parents: Vec<String>,
    },
    /// Persiste o estado de câmera (pan/zoom).
    SetViewport { pan_x: f64, pan_y: f64, zoom: f64 },
    /// Define visibilidade pública da sala.
    SetPublished { published: bool },
}

fn finite(v: f64) -> Result<f64, EditError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(EditError::NonFiniteCoord)
    }
}

impl RoomEdit {
    /// Aplica a operação. Em sucesso, `updated_at` é avançado pelo caller (store).
    pub fn apply(self, room: &mut Room) -> Result<(), EditError> {
        match self {
            RoomEdit::AddElement { element } => {
                if element.word.trim().is_empty() {
                    return Err(EditError::EmptyWord);
                }
                if room.has_element(&element.id) {
                    return Err(EditError::ElementExists(element.id));
                }
                finite(element.x)?;
                finite(element.y)?;
                room.elements.push(element);
            }
            RoomEdit::MoveElement { id, x, y } => {
                finite(x)?;
                finite(y)?;
                let el = room
                    .element_mut(&id)
                    .ok_or(EditError::ElementNotFound(id))?;
                el.x = x;
                el.y = y;
            }
            RoomEdit::EditElement {
                id,
                word,
                gloss,
                pronunciation,
                concept,
                icon,
            } => {
                let el = room
                    .element_mut(&id)
                    .ok_or(EditError::ElementNotFound(id))?;
                if let Some(w) = word {
                    if w.trim().is_empty() {
                        return Err(EditError::EmptyWord);
                    }
                    el.word = w;
                }
                if let Some(g) = gloss {
                    el.gloss = Some(g);
                }
                if let Some(p) = pronunciation {
                    el.pronunciation = Some(p);
                }
                if let Some(c) = concept {
                    el.concept = Some(c);
                }
                if let Some(i) = icon {
                    el.icon = Some(i);
                }
            }
            RoomEdit::DeleteElement { id } => {
                let before = room.elements.len();
                room.elements.retain(|e| e.id != id);
                if room.elements.len() == before {
                    return Err(EditError::ElementNotFound(id));
                }
                // cascata: remove links que tocavam o elemento removido
                room.links.retain(|l| l.from != id && l.to != id);
            }
            RoomEdit::AddLink { link } => {
                if link.from == link.to {
                    return Err(EditError::SelfLink);
                }
                if room.has_link(&link.id) {
                    return Err(EditError::LinkExists(link.id));
                }
                if !room.has_element(&link.from) {
                    return Err(EditError::LinkEndpointMissing(link.from));
                }
                if !room.has_element(&link.to) {
                    return Err(EditError::LinkEndpointMissing(link.to));
                }
                room.links.push(link);
            }
            RoomEdit::DeleteLink { id } => {
                let before = room.links.len();
                room.links.retain(|l| l.id != id);
                if room.links.len() == before {
                    return Err(EditError::LinkNotFound(id));
                }
            }
            RoomEdit::Compose {
                id,
                word,
                lang,
                x,
                y,
                parents,
            } => {
                if word.trim().is_empty() {
                    return Err(EditError::EmptyWord);
                }
                if room.has_element(&id) {
                    return Err(EditError::ElementExists(id));
                }
                if parents.is_empty() {
                    return Err(EditError::ComposeNoParents);
                }
                finite(x)?;
                finite(y)?;
                for p in &parents {
                    if !room.has_element(p) {
                        return Err(EditError::ElementNotFound(p.clone()));
                    }
                }
                let lang = lang.unwrap_or_else(|| room.lang.clone());
                room.elements.push(Element::new(&id, word, lang).at(x, y));
                // um link `compoe` do novo termo para cada parente (fractal)
                for p in parents {
                    let link_id = format!("{id}~{p}");
                    if !room.has_link(&link_id) {
                        room.links.push(
                            Link::new(link_id, id.clone(), p)
                                .with_label("compõe-se de")
                                .with_kind(KIND_COMPOSE),
                        );
                    }
                }
            }
            RoomEdit::SetViewport { pan_x, pan_y, zoom } => {
                finite(pan_x)?;
                finite(pan_y)?;
                if !zoom.is_finite() || !(ZOOM_MIN..=ZOOM_MAX).contains(&zoom) {
                    return Err(EditError::InvalidZoom(zoom));
                }
                room.viewport = Viewport { pan_x, pan_y, zoom };
            }
            RoomEdit::SetPublished { published } => {
                room.published = published;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> Room {
        Room::empty("r1", "alice", "Minha sala", "yo")
    }

    #[test]
    fn add_element_ok() {
        let mut r = room();
        let op = RoomEdit::AddElement {
            element: Element::new("e1", "àṣẹ", "yo").at(10.0, 20.0),
        };
        op.apply(&mut r).unwrap();
        assert_eq!(r.elements.len(), 1);
        assert_eq!(r.element("e1").unwrap().word, "àṣẹ");
    }

    #[test]
    fn add_element_palavra_vazia_falha() {
        let mut r = room();
        let op = RoomEdit::AddElement {
            element: Element::new("e1", "   ", "yo"),
        };
        assert_eq!(op.apply(&mut r), Err(EditError::EmptyWord));
    }

    #[test]
    fn add_element_id_duplicado_falha() {
        let mut r = room();
        RoomEdit::AddElement {
            element: Element::new("e1", "omi", "yo"),
        }
        .apply(&mut r)
        .unwrap();
        let dup = RoomEdit::AddElement {
            element: Element::new("e1", "orí", "yo"),
        };
        assert_eq!(
            dup.apply(&mut r),
            Err(EditError::ElementExists("e1".into()))
        );
    }

    #[test]
    fn move_element_atualiza_coord() {
        let mut r = room();
        RoomEdit::AddElement {
            element: Element::new("e1", "omi", "yo"),
        }
        .apply(&mut r)
        .unwrap();
        RoomEdit::MoveElement {
            id: "e1".into(),
            x: 5.5,
            y: -3.0,
        }
        .apply(&mut r)
        .unwrap();
        let e = r.element("e1").unwrap();
        assert_eq!((e.x, e.y), (5.5, -3.0));
    }

    #[test]
    fn move_element_nao_finito_falha() {
        let mut r = room();
        RoomEdit::AddElement {
            element: Element::new("e1", "omi", "yo"),
        }
        .apply(&mut r)
        .unwrap();
        let op = RoomEdit::MoveElement {
            id: "e1".into(),
            x: f64::NAN,
            y: 0.0,
        };
        assert_eq!(op.apply(&mut r), Err(EditError::NonFiniteCoord));
    }

    #[test]
    fn edit_element_campos_parciais() {
        let mut r = room();
        RoomEdit::AddElement {
            element: Element::new("e1", "omi", "yo"),
        }
        .apply(&mut r)
        .unwrap();
        RoomEdit::EditElement {
            id: "e1".into(),
            word: None,
            gloss: Some("água".into()),
            pronunciation: Some("[ɔ.mi]".into()),
            concept: Some("co://concepts/water.md".into()),
            icon: None,
        }
        .apply(&mut r)
        .unwrap();
        let e = r.element("e1").unwrap();
        assert_eq!(e.word, "omi"); // inalterado
        assert_eq!(e.gloss.as_deref(), Some("água"));
        assert_eq!(e.concept.as_deref(), Some("co://concepts/water.md"));
    }

    #[test]
    fn delete_element_inexistente_falha() {
        let mut r = room();
        let op = RoomEdit::DeleteElement { id: "x".into() };
        assert_eq!(
            op.apply(&mut r),
            Err(EditError::ElementNotFound("x".into()))
        );
    }

    #[test]
    fn set_viewport_valida_zoom() {
        let mut r = room();
        RoomEdit::SetViewport {
            pan_x: 100.0,
            pan_y: -50.0,
            zoom: 2.0,
        }
        .apply(&mut r)
        .unwrap();
        assert_eq!(r.viewport.zoom, 2.0);
        let bad = RoomEdit::SetViewport {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 100.0,
        };
        assert!(matches!(bad.apply(&mut r), Err(EditError::InvalidZoom(_))));
    }

    #[test]
    fn edit_op_round_trip_json() {
        let op = RoomEdit::AddElement {
            element: Element::new("e1", "nhandereko", "gn-mbya").with_gloss("nosso modo de viver"),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"op\":\"add_element\""));
        let back: RoomEdit = serde_json::from_str(&json).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn lexicon_link_default_local() {
        let e = Element::new("e1", "teko", "gn-mbya");
        assert_eq!(e.lexicon, LexiconLink::Local);
        assert_eq!(e.lexicon.path(), None);
    }

    fn room_with_two() -> Room {
        let mut r = room();
        RoomEdit::AddElement {
            element: Element::new("a", "ayvu", "gn-mbya"),
        }
        .apply(&mut r)
        .unwrap();
        RoomEdit::AddElement {
            element: Element::new("b", "nhe'ẽ", "gn-mbya"),
        }
        .apply(&mut r)
        .unwrap();
        r
    }

    #[test]
    fn add_link_ok() {
        let mut r = room_with_two();
        RoomEdit::AddLink {
            link: Link::new("l1", "a", "b").with_label("relacionado"),
        }
        .apply(&mut r)
        .unwrap();
        assert_eq!(r.links.len(), 1);
        assert_eq!(r.links[0].label.as_deref(), Some("relacionado"));
    }

    #[test]
    fn add_link_endpoint_inexistente_falha() {
        let mut r = room_with_two();
        let op = RoomEdit::AddLink {
            link: Link::new("l1", "a", "zzz"),
        };
        assert_eq!(
            op.apply(&mut r),
            Err(EditError::LinkEndpointMissing("zzz".into()))
        );
    }

    #[test]
    fn add_link_self_falha() {
        let mut r = room_with_two();
        let op = RoomEdit::AddLink {
            link: Link::new("l1", "a", "a"),
        };
        assert_eq!(op.apply(&mut r), Err(EditError::SelfLink));
    }

    #[test]
    fn add_link_id_duplicado_falha() {
        let mut r = room_with_two();
        RoomEdit::AddLink {
            link: Link::new("l1", "a", "b"),
        }
        .apply(&mut r)
        .unwrap();
        let dup = RoomEdit::AddLink {
            link: Link::new("l1", "b", "a"),
        };
        assert_eq!(dup.apply(&mut r), Err(EditError::LinkExists("l1".into())));
    }

    #[test]
    fn delete_element_remove_links_em_cascata() {
        let mut r = room_with_two();
        RoomEdit::AddLink {
            link: Link::new("l1", "a", "b"),
        }
        .apply(&mut r)
        .unwrap();
        RoomEdit::DeleteElement { id: "a".into() }
            .apply(&mut r)
            .unwrap();
        assert!(r.links.is_empty(), "link órfão deveria ser removido");
    }

    #[test]
    fn delete_link_inexistente_falha() {
        let mut r = room_with_two();
        let op = RoomEdit::DeleteLink { id: "nope".into() };
        assert_eq!(
            op.apply(&mut r),
            Err(EditError::LinkNotFound("nope".into()))
        );
    }

    #[test]
    fn link_op_round_trip_json() {
        let op = RoomEdit::AddLink {
            link: Link::new("l1", "a", "b").with_label("compõe"),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"op\":\"add_link\""));
        let back: RoomEdit = serde_json::from_str(&json).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn compose_cria_termo_e_links_compoe_para_parents() {
        let mut r = room_with_two(); // a = ayvu, b = nhe'ẽ
        RoomEdit::Compose {
            id: "c".into(),
            word: "ayvu rapyta".into(),
            lang: None,
            x: 0.0,
            y: -120.0,
            parents: vec!["a".into(), "b".into()],
        }
        .apply(&mut r)
        .unwrap();
        assert!(r.has_element("c"));
        // herda a língua da sala quando lang=None
        assert_eq!(r.element("c").unwrap().lang, "yo");
        let compose: Vec<_> = r
            .links
            .iter()
            .filter(|l| l.is_compose() && l.from == "c")
            .collect();
        assert_eq!(compose.len(), 2, "um link compoe por parente");
        assert!(compose.iter().all(|l| l.directed));
        let alvos: Vec<&str> = compose.iter().map(|l| l.to.as_str()).collect();
        assert!(alvos.contains(&"a") && alvos.contains(&"b"));
    }

    #[test]
    fn compose_sem_parents_falha() {
        let mut r = room_with_two();
        let op = RoomEdit::Compose {
            id: "c".into(),
            word: "x".into(),
            lang: None,
            x: 0.0,
            y: 0.0,
            parents: vec![],
        };
        assert_eq!(op.apply(&mut r), Err(EditError::ComposeNoParents));
    }

    #[test]
    fn compose_parent_inexistente_falha() {
        let mut r = room_with_two();
        let op = RoomEdit::Compose {
            id: "c".into(),
            word: "x".into(),
            lang: None,
            x: 0.0,
            y: 0.0,
            parents: vec!["zzz".into()],
        };
        assert_eq!(
            op.apply(&mut r),
            Err(EditError::ElementNotFound("zzz".into()))
        );
    }
}
