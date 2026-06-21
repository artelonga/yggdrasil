//! Formato de autoria de `LexiconPack` em arquivo YAML (YG-166).
//!
//! Permite criar packs **sem editar nem recompilar Rust**: basta soltar um
//! `<id>.yaml` no diretório `data/packs/` (ou `$YGGDRASIL_PACKS_DIR`).
//!
//! ## Atalhos de áudio
//!
//! O campo `audio` de cada entrada aceita atalhos de notação musical:
//!
//! - `note: "C4"` → `Synth` com 1 voz (nota nomeada → Hz via `note_freq`).
//! - `chord: ["C4","E4","G4"]` → `Synth` com N vozes (acorde).
//! - `sequence: [{chord:[...], duration_ms:...}]` → `Sequence`.
//! - `speech: {text, ipa?, lang}` → `Speech`.
//! - `freqs: [Hz...]` → `Synth` (escape hatch p/ microtonal; Hz crus).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use super::pack::{
    AbstractPos, Adsr, EntropyStats, EntryAudio, LexiconPack, PackEntry, PackKind, Relation,
    SeqStep, Waveform, note_freq,
};
use super::room::Reference;

/// Erro de carregamento ou parse de um pack-file.
#[derive(Debug, Error)]
pub enum PackFileError {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML inválido em {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("nota inválida '{name}' na entrada '{term}'")]
    InvalidNote { name: String, term: String },
}

// ─── Sub-tipos do formato de arquivo ─────────────────────────────────────────

/// Posição abstrata no arquivo — aceita omitir `z` (default 0).
#[derive(Debug, Deserialize)]
pub struct PackFilePos {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
}

/// Um passo de sequência no arquivo YAML.
#[derive(Debug, Deserialize)]
pub struct PackFileStep {
    /// Notas nomeadas (e.g. `["C4","E4"]`) ou frequências cruas em Hz.
    #[serde(default)]
    pub chord: Vec<String>,
    /// Frequências cruas (Hz); usado quando `chord` está vazio.
    #[serde(default)]
    pub freqs: Vec<f64>,
    pub duration_ms: u32,
}

/// Especificação de Speech no YAML.
#[derive(Debug, Deserialize)]
pub struct SpeechSpec {
    pub text: String,
    #[serde(default)]
    pub ipa: Option<String>,
    pub lang: String,
}

/// Atalhos de áudio no YAML — superset authoring do [`EntryAudio`].
///
/// Serde usa `untagged`: tenta cada variante em ordem; a primeira que casa vence.
/// A distinção entre variantes é pelo campo-chave (`note`, `chord`, `sequence`,
/// `speech`, `freqs`) — sem conflito entre elas.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PackFileAudio {
    /// `note: "A4"` → `Synth` de 1 voz.
    Note {
        note: String,
        #[serde(default = "default_waveform")]
        waveform: Waveform,
        #[serde(default = "default_duration")]
        duration_ms: u32,
        #[serde(default)]
        envelope: Adsr,
    },
    /// `chord: ["C4","E4","G4"]` → `Synth` de N vozes.
    Chord {
        chord: Vec<String>,
        #[serde(default = "default_waveform")]
        waveform: Waveform,
        #[serde(default = "default_duration")]
        duration_ms: u32,
        #[serde(default)]
        envelope: Adsr,
    },
    /// `sequence: [{chord:[...], duration_ms:...}]` → `Sequence`.
    Sequence {
        sequence: Vec<PackFileStep>,
        #[serde(default = "default_waveform")]
        waveform: Waveform,
        #[serde(default)]
        envelope: Adsr,
    },
    /// `speech: {text, ipa?, lang}` → `Speech`.
    Speech { speech: SpeechSpec },
    /// `freqs: [Hz...]` → `Synth` (microtonal / Hz cru).
    Freqs {
        freqs: Vec<f64>,
        #[serde(default = "default_waveform")]
        waveform: Waveform,
        #[serde(default = "default_duration")]
        duration_ms: u32,
        #[serde(default)]
        envelope: Adsr,
    },
}

fn default_waveform() -> Waveform {
    Waveform::Triangle
}

fn default_duration() -> u32 {
    500
}

/// Uma entrada de pack no arquivo YAML.
#[derive(Debug, Deserialize)]
pub struct PackFileEntry {
    pub term: String,
    #[serde(default)]
    pub gloss: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub refs: Vec<Reference>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub audio: Option<PackFileAudio>,
    pub pos: PackFilePos,
}

/// O arquivo YAML de um pack — superset de [`LexiconPack`].
#[derive(Debug, Deserialize)]
pub struct PackFile {
    pub id: String,
    pub kind: PackKind,
    pub notation: String,
    pub title: String,
    pub theme: String,
    #[serde(default)]
    pub entropy_stats: Option<EntropyStats>,
    pub entries: Vec<PackFileEntry>,
}

// ─── Conversão para LexiconPack ──────────────────────────────────────────────

impl PackFileAudio {
    fn resolve(self, term: &str) -> Result<EntryAudio, PackFileError> {
        match self {
            PackFileAudio::Note {
                note,
                waveform,
                duration_ms,
                envelope,
            } => {
                let freq = note_freq(&note).ok_or_else(|| PackFileError::InvalidNote {
                    name: note.clone(),
                    term: term.to_string(),
                })?;
                Ok(EntryAudio::Synth {
                    waveform,
                    freqs: vec![freq],
                    envelope,
                    duration_ms,
                })
            }
            PackFileAudio::Chord {
                chord,
                waveform,
                duration_ms,
                envelope,
            } => {
                let mut freqs = Vec::with_capacity(chord.len());
                for name in &chord {
                    let freq = note_freq(name).ok_or_else(|| PackFileError::InvalidNote {
                        name: name.clone(),
                        term: term.to_string(),
                    })?;
                    freqs.push(freq);
                }
                Ok(EntryAudio::Synth {
                    waveform,
                    freqs,
                    envelope,
                    duration_ms,
                })
            }
            PackFileAudio::Sequence {
                sequence,
                waveform,
                envelope,
            } => {
                let steps = sequence
                    .into_iter()
                    .map(|s| {
                        let freqs = if !s.freqs.is_empty() {
                            s.freqs
                        } else {
                            s.chord.iter().filter_map(|n| note_freq(n)).collect()
                        };
                        SeqStep {
                            freqs,
                            duration_ms: s.duration_ms,
                        }
                    })
                    .collect();
                Ok(EntryAudio::Sequence {
                    waveform,
                    steps,
                    envelope,
                })
            }
            PackFileAudio::Speech { speech } => Ok(EntryAudio::Speech {
                text: speech.text,
                ipa: speech.ipa,
                lang: speech.lang,
            }),
            PackFileAudio::Freqs {
                freqs,
                waveform,
                duration_ms,
                envelope,
            } => Ok(EntryAudio::Synth {
                waveform,
                freqs,
                envelope,
                duration_ms,
            }),
        }
    }
}

impl PackFileEntry {
    fn into_pack_entry(self) -> Result<PackEntry, PackFileError> {
        let audio = self.audio.map(|a| a.resolve(&self.term)).transpose()?;
        Ok(PackEntry {
            term: self.term,
            gloss: self.gloss,
            role: self.role,
            refs: self.refs,
            relations: self.relations,
            examples: self.examples,
            audio,
            pos: AbstractPos::new(self.pos.x, self.pos.y, self.pos.z),
        })
    }
}

impl PackFile {
    /// Converte o arquivo para `LexiconPack`, resolvendo todos os atalhos de áudio.
    pub fn into_lexicon_pack(self) -> Result<LexiconPack, PackFileError> {
        let mut entries = Vec::with_capacity(self.entries.len());
        for entry in self.entries {
            entries.push(entry.into_pack_entry()?);
        }
        Ok(LexiconPack {
            id: self.id,
            kind: self.kind,
            notation: self.notation,
            title: self.title,
            theme: self.theme,
            entropy_stats: self.entropy_stats,
            entries,
        })
    }
}

// ─── Funções públicas de carregamento ────────────────────────────────────────

/// Carrega um pack a partir de um arquivo YAML.
///
/// Retorna `Err` se o arquivo não existir, não for lido ou tiver YAML inválido.
/// O chamador decide se aborta ou continua com os demais packs.
pub fn load_file(path: &Path) -> Result<LexiconPack, PackFileError> {
    let text = std::fs::read_to_string(path)?;
    let pf: PackFile = serde_yaml::from_str(&text).map_err(|source| PackFileError::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    pf.into_lexicon_pack()
}

/// Carrega todos os `*.yaml` de um diretório como packs.
///
/// Arquivo inválido gera mensagem de erro no stderr mas **não aborta** os demais
/// — cada pack falha isoladamente. Diretório inexistente retorna vetor vazio
/// sem pânico (permite deploy sem `data/packs/`).
pub fn load_dir(dir: &Path) -> Vec<LexiconPack> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut packs = Vec::new();
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        match load_file(&path) {
            Ok(pack) => packs.push(pack),
            Err(e) => eprintln!("yggdrasil: pack inválido em {}: {e}", path.display()),
        }
    }
    packs
}

// ─── Testes ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comunicacao::pack::{EntryAudio, PackKind, Waveform};

    const EXEMPLO_YAML: &str = include_str!("../../../data/packs/exemplo.yaml");

    #[test]
    fn exemplo_yaml_parse_round_trip_json() {
        let pf: PackFile = serde_yaml::from_str(EXEMPLO_YAML).expect("YAML inválido");
        let pack = pf.into_lexicon_pack().expect("conversão falhou");

        // Serializa para JSON e desserializa — deve ser idêntico.
        let json = serde_json::to_string(&pack).expect("serialização falhou");
        let back: LexiconPack = serde_json::from_str(&json).expect("deserialização falhou");
        assert_eq!(pack, back, "round-trip JSON falhou");
    }

    #[test]
    fn exemplo_yaml_note_a4_resolve_440hz() {
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(EXEMPLO_YAML)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        // Deve existir uma entrada com `note: "A4"` → freqs=[440.0]
        let entry = pack
            .entries
            .iter()
            .find(|e| {
                matches!(
                    &e.audio,
                    Some(EntryAudio::Synth { freqs, .. }) if freqs.len() == 1 && (freqs[0] - 440.0).abs() < 1e-9
                )
            })
            .expect("entrada note:A4 não encontrada no exemplo.yaml");
        match entry.audio.as_ref().unwrap() {
            EntryAudio::Synth { freqs, .. } => {
                assert!((freqs[0] - 440.0).abs() < 1e-9, "A4 deve ser 440 Hz");
            }
            other => panic!("esperado Synth, foi {other:?}"),
        }
    }

    #[test]
    fn exemplo_yaml_chord_tem_multiplas_vozes() {
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(EXEMPLO_YAML)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        let chord_entry = pack
            .entries
            .iter()
            .find(|e| e.role.as_deref() == Some("chord"))
            .expect("nenhuma entrada chord no exemplo.yaml");
        match chord_entry.audio.as_ref().unwrap() {
            EntryAudio::Synth { freqs, .. } => {
                assert!(freqs.len() >= 2, "acorde deve ter ≥2 vozes");
            }
            other => panic!("chord deve ser Synth, foi {other:?}"),
        }
    }

    #[test]
    fn exemplo_yaml_tem_sequence_ou_speech() {
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(EXEMPLO_YAML)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        let has_seq = pack
            .entries
            .iter()
            .any(|e| matches!(&e.audio, Some(EntryAudio::Sequence { .. })));
        let has_speech = pack
            .entries
            .iter()
            .any(|e| matches!(&e.audio, Some(EntryAudio::Speech { .. })));
        assert!(
            has_seq || has_speech,
            "exemplo.yaml deve ter sequence ou speech"
        );
    }

    #[test]
    fn load_dir_diretorio_inexistente_retorna_vazio() {
        let resultado = load_dir(Path::new("/nao/existe/aqui"));
        assert!(resultado.is_empty());
    }

    #[test]
    fn load_dir_carrega_yaml_e_ignora_outros_arquivos() {
        let dir = tempfile::tempdir().unwrap();
        // Arquivo válido
        std::fs::write(
            dir.path().join("teste.yaml"),
            r#"
id: teste
kind: domain
notation: livre
title: Teste
theme: verde
entries:
  - term: "alpha"
    pos: {x: 0.0, y: 0.0}
"#,
        )
        .unwrap();
        // Arquivo que deve ser ignorado (não-yaml)
        std::fs::write(dir.path().join("ignore.txt"), "texto ignorado").unwrap();
        // Arquivo YAML inválido — não deve abortar os válidos
        std::fs::write(dir.path().join("invalido.yaml"), "id: [broken yaml").unwrap();

        let packs = load_dir(dir.path());
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, "teste");
    }

    #[test]
    fn note_atalho_resolve_para_synth() {
        let yaml = r#"
id: t
kind: music
notation: 12tet
title: T
theme: t
entries:
  - term: "C4"
    pos: {x: 0.0, y: 0.0}
    audio:
      note: "C4"
      waveform: triangle
      duration_ms: 400
"#;
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(yaml)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        let entry = &pack.entries[0];
        match entry.audio.as_ref().unwrap() {
            EntryAudio::Synth {
                waveform,
                freqs,
                duration_ms,
                ..
            } => {
                assert_eq!(*waveform, Waveform::Triangle);
                assert_eq!(freqs.len(), 1);
                // C4: midi=60, A4=69 → 440 * 2^((60-69)/12)
                let esperado = 440.0 * 2f64.powf((60.0 - 69.0) / 12.0);
                assert!((freqs[0] - esperado).abs() < 1e-6);
                assert_eq!(*duration_ms, 400);
            }
            other => panic!("esperado Synth, foi {other:?}"),
        }
    }

    #[test]
    fn chord_atalho_resolve_tres_vozes() {
        let yaml = r#"
id: t
kind: music
notation: 12tet
title: T
theme: t
entries:
  - term: "Dó maior"
    role: chord
    pos: {x: 1.0, y: 0.0}
    audio:
      chord: ["C4", "E4", "G4"]
      waveform: sawtooth
      duration_ms: 900
"#;
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(yaml)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        match pack.entries[0].audio.as_ref().unwrap() {
            EntryAudio::Synth {
                freqs, waveform, ..
            } => {
                assert_eq!(freqs.len(), 3, "tríade = 3 vozes");
                assert_eq!(*waveform, Waveform::Sawtooth);
            }
            other => panic!("esperado Synth, foi {other:?}"),
        }
    }

    #[test]
    fn sequence_atalho_resolve_steps() {
        let yaml = r#"
id: t
kind: music
notation: 12tet
title: T
theme: t
entries:
  - term: "Motivo"
    role: motif
    pos: {x: 2.0, y: 0.0}
    audio:
      sequence:
        - chord: ["C4", "E4", "G4"]
          duration_ms: 480
        - chord: ["G4", "B4", "D5"]
          duration_ms: 480
      waveform: square
"#;
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(yaml)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        match pack.entries[0].audio.as_ref().unwrap() {
            EntryAudio::Sequence {
                steps, waveform, ..
            } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].freqs.len(), 3);
                assert_eq!(*waveform, Waveform::Square);
            }
            other => panic!("esperado Sequence, foi {other:?}"),
        }
    }

    #[test]
    fn speech_atalho_resolve() {
        let yaml = r#"
id: t
kind: language
notation: ipa
title: T
theme: t
entries:
  - term: "olá"
    role: word
    pos: {x: 0.0, y: 0.0}
    audio:
      speech:
        text: "olá"
        ipa: "oˈla"
        lang: "pt-BR"
"#;
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(yaml)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        match pack.entries[0].audio.as_ref().unwrap() {
            EntryAudio::Speech { text, ipa, lang } => {
                assert_eq!(text, "olá");
                assert_eq!(ipa.as_deref(), Some("oˈla"));
                assert_eq!(lang, "pt-BR");
            }
            other => panic!("esperado Speech, foi {other:?}"),
        }
    }

    #[test]
    fn nota_invalida_gera_erro_identificavel() {
        let yaml = r#"
id: t
kind: music
notation: 12tet
title: T
theme: t
entries:
  - term: "nota-ruim"
    pos: {x: 0.0, y: 0.0}
    audio:
      note: "X9"
      duration_ms: 500
"#;
        let result = serde_yaml::from_str::<PackFile>(yaml)
            .unwrap()
            .into_lexicon_pack();
        assert!(
            matches!(result, Err(PackFileError::InvalidNote { .. })),
            "nota inválida deve gerar PackFileError::InvalidNote"
        );
    }

    #[test]
    fn pack_file_sem_audio_aceita_entrada_muda() {
        let yaml = r#"
id: silencioso
kind: domain
notation: livre
title: Silencioso
theme: cinza
entries:
  - term: "sem-som"
    gloss: "entrada sem áudio"
    pos: {x: 0.0, y: 0.0}
"#;
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(yaml)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        assert!(pack.entries[0].audio.is_none(), "entrada muda OK");
    }

    #[test]
    fn exemplo_yaml_kind_correto() {
        let pack: LexiconPack = serde_yaml::from_str::<PackFile>(EXEMPLO_YAML)
            .unwrap()
            .into_lexicon_pack()
            .unwrap();
        // O exemplo cobre music (contém notas e acordes)
        assert!(
            matches!(
                pack.kind,
                PackKind::Music | PackKind::Domain | PackKind::Language
            ),
            "kind deve ser válido"
        );
        assert!(!pack.entries.is_empty(), "exemplo deve ter entradas");
        assert!(pack.entries.len() <= 8, "limite de ≤8 entradas");
    }
}
