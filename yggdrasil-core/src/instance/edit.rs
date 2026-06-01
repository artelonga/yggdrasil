//! Edições granulares sobre uma [`UniverseInstance`] (YG-76).
//!
//! O servidor carrega a instância, aplica um [`EditOp`], valida invariantes num
//! único ponto, e persiste. A lógica vive no core para ser testável sem axum e
//! reutilizável por qualquer host (web ou Godot).

use serde::{Deserialize, Serialize};

use super::schema::{Block, Cell, Connection, ContentRef, Layer, UniverseInstance};

/// Uma operação de edição atômica. Tag `op` no JSON, ex.
/// `{ "op": "place_block", "layer": "base", "block": { ... } }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    PlaceBlock {
        layer: String,
        block: Block,
    },
    MoveBlock {
        layer: String,
        block_id: String,
        to: Cell,
    },
    DeleteBlock {
        layer: String,
        block_id: String,
    },
    EditLayer {
        layer: String,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        name: Option<String>,
    },
    AddLayer {
        layer: Layer,
    },
    AddConnection {
        connection: Connection,
    },
    DeleteConnection {
        connection_id: String,
    },
    AttachContent {
        layer: String,
        block_id: String,
        content: ContentRef,
    },
}

/// Falha de validação ao aplicar um [`EditOp`]. Mapeia para status HTTP no host.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditError {
    #[error("camada '{0}' não encontrada")]
    LayerNotFound(String),
    #[error("camada '{0}' já existe")]
    LayerExists(String),
    #[error("bloco '{0}' não encontrado")]
    BlockNotFound(String),
    #[error("bloco '{0}' já existe")]
    BlockExists(String),
    #[error("conexão '{0}' não encontrada")]
    ConnectionNotFound(String),
    #[error("bloco endpoint '{0}' não existe")]
    EndpointNotFound(String),
    #[error("célula ({0}, {1}) fora dos limites da grade")]
    OutOfBounds(u32, u32),
    #[error("opacidade {0} fora do intervalo 0.0..=1.0")]
    InvalidOpacity(f32),
}

impl EditOp {
    /// Aplica esta operação à instância, validando invariantes. Em erro, a
    /// instância não é modificada (validação ocorre antes de mutar).
    pub fn apply(self, inst: &mut UniverseInstance) -> Result<(), EditError> {
        match self {
            EditOp::PlaceBlock { layer, block } => {
                if !inst.grid.contains(block.pos) {
                    return Err(EditError::OutOfBounds(block.pos.x, block.pos.y));
                }
                if inst.has_block(&block.id) {
                    return Err(EditError::BlockExists(block.id));
                }
                let l = inst
                    .layer_mut(&layer)
                    .ok_or_else(|| EditError::LayerNotFound(layer.clone()))?;
                l.blocks.push(block);
            }
            EditOp::MoveBlock {
                layer,
                block_id,
                to,
            } => {
                if !inst.grid.contains(to) {
                    return Err(EditError::OutOfBounds(to.x, to.y));
                }
                let l = inst
                    .layer_mut(&layer)
                    .ok_or_else(|| EditError::LayerNotFound(layer.clone()))?;
                let b = l
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == block_id)
                    .ok_or_else(|| EditError::BlockNotFound(block_id.clone()))?;
                b.pos = to;
            }
            EditOp::DeleteBlock { layer, block_id } => {
                let l = inst
                    .layer_mut(&layer)
                    .ok_or_else(|| EditError::LayerNotFound(layer.clone()))?;
                let before = l.blocks.len();
                l.blocks.retain(|b| b.id != block_id);
                if l.blocks.len() == before {
                    return Err(EditError::BlockNotFound(block_id));
                }
                // Remove conexões órfãs apontando para o bloco removido.
                inst.connections
                    .retain(|c| c.from != block_id && c.to != block_id);
            }
            EditOp::EditLayer {
                layer,
                visible,
                opacity,
                name,
            } => {
                if let Some(o) = opacity
                    && !(0.0..=1.0).contains(&o)
                {
                    return Err(EditError::InvalidOpacity(o));
                }
                let l = inst
                    .layer_mut(&layer)
                    .ok_or_else(|| EditError::LayerNotFound(layer.clone()))?;
                if let Some(v) = visible {
                    l.visible = v;
                }
                if let Some(o) = opacity {
                    l.opacity = o;
                }
                if let Some(n) = name {
                    l.name = n;
                }
            }
            EditOp::AddLayer { layer } => {
                if !(0.0..=1.0).contains(&layer.opacity) {
                    return Err(EditError::InvalidOpacity(layer.opacity));
                }
                if inst.layers.iter().any(|l| l.id == layer.id) {
                    return Err(EditError::LayerExists(layer.id));
                }
                inst.layers.push(layer);
            }
            EditOp::AddConnection { connection } => {
                if !inst.has_block(&connection.from) {
                    return Err(EditError::EndpointNotFound(connection.from));
                }
                if !inst.has_block(&connection.to) {
                    return Err(EditError::EndpointNotFound(connection.to));
                }
                if inst.connections.iter().any(|c| c.id == connection.id) {
                    return Err(EditError::ConnectionNotFound(connection.id));
                }
                inst.connections.push(connection);
            }
            EditOp::DeleteConnection { connection_id } => {
                let before = inst.connections.len();
                inst.connections.retain(|c| c.id != connection_id);
                if inst.connections.len() == before {
                    return Err(EditError::ConnectionNotFound(connection_id));
                }
            }
            EditOp::AttachContent {
                layer,
                block_id,
                content,
            } => {
                let l = inst
                    .layer_mut(&layer)
                    .ok_or_else(|| EditError::LayerNotFound(layer.clone()))?;
                let b = l
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == block_id)
                    .ok_or_else(|| EditError::BlockNotFound(block_id.clone()))?;
                b.attachments.push(content);
            }
        }
        inst.updated_at = chrono::Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::schema::{Cell, GridSpec, Projection};

    fn base() -> UniverseInstance {
        UniverseInstance::empty("inst", "owner", "T")
    }

    fn block(id: &str, x: u32, y: u32) -> Block {
        Block {
            id: id.into(),
            block_type: "landmark".into(),
            pos: Cell { x, y },
            label: None,
            attachments: vec![],
            props: Default::default(),
        }
    }

    #[test]
    fn place_block_ok() {
        let mut inst = base();
        EditOp::PlaceBlock {
            layer: "base".into(),
            block: block("a", 1, 1),
        }
        .apply(&mut inst)
        .unwrap();
        assert!(inst.has_block("a"));
    }

    #[test]
    fn place_block_fora_dos_limites() {
        let mut inst = base();
        inst.grid = GridSpec {
            width: 5,
            height: 5,
            cell_size: 16,
        };
        let err = EditOp::PlaceBlock {
            layer: "base".into(),
            block: block("a", 99, 0),
        }
        .apply(&mut inst)
        .unwrap_err();
        assert_eq!(err, EditError::OutOfBounds(99, 0));
    }

    #[test]
    fn place_block_camada_inexistente() {
        let mut inst = base();
        let err = EditOp::PlaceBlock {
            layer: "fantasma".into(),
            block: block("a", 1, 1),
        }
        .apply(&mut inst)
        .unwrap_err();
        assert_eq!(err, EditError::LayerNotFound("fantasma".into()));
    }

    #[test]
    fn place_block_duplicado() {
        let mut inst = base();
        EditOp::PlaceBlock {
            layer: "base".into(),
            block: block("a", 1, 1),
        }
        .apply(&mut inst)
        .unwrap();
        let err = EditOp::PlaceBlock {
            layer: "base".into(),
            block: block("a", 2, 2),
        }
        .apply(&mut inst)
        .unwrap_err();
        assert_eq!(err, EditError::BlockExists("a".into()));
    }

    #[test]
    fn add_connection_endpoint_inexistente() {
        let mut inst = base();
        EditOp::PlaceBlock {
            layer: "base".into(),
            block: block("a", 1, 1),
        }
        .apply(&mut inst)
        .unwrap();
        let err = EditOp::AddConnection {
            connection: Connection {
                id: "c1".into(),
                from: "a".into(),
                to: "inexistente".into(),
                label: None,
                directed: false,
                props: Default::default(),
            },
        }
        .apply(&mut inst)
        .unwrap_err();
        assert_eq!(err, EditError::EndpointNotFound("inexistente".into()));
    }

    #[test]
    fn delete_block_remove_conexoes_orfas() {
        let mut inst = base();
        for b in ["a", "b"] {
            EditOp::PlaceBlock {
                layer: "base".into(),
                block: block(b, 1, 1),
            }
            .apply(&mut inst)
            .unwrap();
        }
        // segundo bloco em outra célula para não duplicar pos (não validado, mas limpo)
        inst.layer_mut("base").unwrap().blocks[1].pos = Cell { x: 2, y: 2 };
        EditOp::AddConnection {
            connection: Connection {
                id: "c1".into(),
                from: "a".into(),
                to: "b".into(),
                label: None,
                directed: false,
                props: Default::default(),
            },
        }
        .apply(&mut inst)
        .unwrap();
        EditOp::DeleteBlock {
            layer: "base".into(),
            block_id: "a".into(),
        }
        .apply(&mut inst)
        .unwrap();
        assert!(inst.connections.is_empty(), "conexão órfã deve sumir");
    }

    #[test]
    fn edit_layer_opacity_invalida() {
        let mut inst = base();
        let err = EditOp::EditLayer {
            layer: "base".into(),
            visible: None,
            opacity: Some(1.5),
            name: None,
        }
        .apply(&mut inst)
        .unwrap_err();
        assert_eq!(err, EditError::InvalidOpacity(1.5));
    }

    #[test]
    fn edit_layer_opacity_persiste() {
        let mut inst = base();
        EditOp::EditLayer {
            layer: "base".into(),
            visible: Some(false),
            opacity: Some(0.3),
            name: None,
        }
        .apply(&mut inst)
        .unwrap();
        let l = inst.layer("base").unwrap();
        assert_eq!(l.opacity, 0.3);
        assert!(!l.visible);
    }

    #[test]
    fn editop_deserializa_do_json_tagueado() {
        let json =
            r#"{ "op": "move_block", "layer": "base", "block_id": "a", "to": { "x": 3, "y": 4 } }"#;
        let op: EditOp = serde_json::from_str(json).unwrap();
        assert!(matches!(op, EditOp::MoveBlock { .. }));
    }

    #[test]
    fn _projection_referenciada() {
        // garante import usado mesmo sem teste dedicado de projeção aqui
        let _ = Projection::Isometric;
    }
}
