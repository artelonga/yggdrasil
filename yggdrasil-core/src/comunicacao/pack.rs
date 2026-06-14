//! `LexiconPack` — a **quilha** do ÑE'Ẽ (YG-155).
//!
//! Generaliza o léxico de [`super::lexicon`] para a abstração que escala: uma
//! **linguagem** é *qualquer sistema de símbolos com um léxico* — idioma natural
//! (`kind = language`), **música** (`kind = music`) ou um domínio como "neuro"
//! (`kind = domain`). Cada entrada ganha uma **dimensão de áudio** opcional, e a
//! **música é a primeira linguagem áudio-nativa**: a "palavra" *é* o som (uma
//! nota/acorde sintetizado), não um arquivo.
//!
//! ## Contratos engine-neutros
//!
//! Os tipos aqui **não assumem renderização 2D**. Uma entrada carrega uma
//! posição abstrata ([`AbstractPos`] com `x/y/z`) e uma descrição de áudio
//! ([`EntryAudio`]) que descreve *como sintetizar* o som — não bytes de sample.
//! O `<canvas>` 2D de hoje é UM consumidor; um cliente 3D/voxel futuro consome o
//! **mesmo** pacote (ver `docs/architecture/lexicon-pack-contract.md`).
//!
//! ## Memory-light
//!
//! Pacotes são **sintetizados sob demanda** (sem MB de samples no bundle/git) e
//! servidos por entrada/sala (lazy-load no front, ver `pack-loader.js`). O áudio
//! é só *parâmetros* (freq/forma de onda/ADSR), tocados por `OscillatorNode` ao
//! vivo. [`MAX_RESIDENT_ENTRIES`] documenta o teto de entradas residentes que o
//! cliente deve respeitar (descarte LRU além disso).

use serde::{Deserialize, Serialize};

use super::room::Reference;

/// Teto recomendado de entradas de pacote residentes em memória no cliente.
/// Além disso, o consumidor descarta as menos-recentemente-usadas (LRU). Vive
/// aqui (e não só no front) para ser o contrato único entre back e front.
pub const MAX_RESIDENT_ENTRIES: usize = 256;

/// Afinação de referência: A4 = 440 Hz (12-TET).
pub const A4_HZ: f64 = 440.0;

/// Que tipo de sistema de símbolos o pacote descreve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    /// Idioma natural (Mbyá, Iorubá, Português…). Áudio = pronúncia.
    Language,
    /// Música — a primeira linguagem áudio-nativa. Áudio = a própria nota/acorde.
    Music,
    /// Domínio arbitrário (neuro, Hebraico ritual…). Entra pelo mesmo tipo.
    Domain,
}

/// Forma de onda do oscilador (espelha os tipos do Web Audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Waveform {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

/// Envelope de amplitude ADSR (milissegundos, `sustain` em 0..=1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Adsr {
    pub attack_ms: u32,
    pub decay_ms: u32,
    pub sustain: f64,
    pub release_ms: u32,
}

impl Default for Adsr {
    fn default() -> Self {
        // Um pluck curto e limpo — bom default p/ nota/acorde.
        Self {
            attack_ms: 8,
            decay_ms: 120,
            sustain: 0.6,
            release_ms: 240,
        }
    }
}

/// Um passo de um motivo (notas tocadas juntas por uma duração).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeqStep {
    /// Frequências (Hz) tocadas simultaneamente neste passo.
    pub freqs: Vec<f64>,
    pub duration_ms: u32,
}

/// A **dimensão de áudio** de uma entrada — sempre *parâmetros de síntese*,
/// nunca bytes de sample. Tagueada por `mode` no JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum EntryAudio {
    /// Nota ou acorde: `freqs` tocadas **juntas** (1 voz = nota, N = acorde).
    Synth {
        waveform: Waveform,
        freqs: Vec<f64>,
        #[serde(default)]
        envelope: Adsr,
        duration_ms: u32,
    },
    /// Motivo/melodia: passos **em sequência** no tempo.
    Sequence {
        waveform: Waveform,
        steps: Vec<SeqStep>,
        #[serde(default)]
        envelope: Adsr,
    },
    /// Pronúncia de idioma via Web Speech (cliente). `ipa` é dica/exibição;
    /// `text` é o que se fala. Fallback silencioso se a plataforma não suportar.
    Speech {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ipa: Option<String>,
        lang: String,
    },
}

/// Posição abstrata, **agnóstica de renderização**. `z` default 0 → o consumidor
/// 2D ignora; um cliente 3D/voxel usa as três. Não é pixels nem células.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AbstractPos {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
}

impl AbstractPos {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// Relação entre entradas do pacote (o significado emerge das relações). `to` é
/// o `term` de outra entrada do mesmo pacote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Relation {
    pub fn new(to: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            label: Some(label.into()),
        }
    }
}

/// Uma entrada de léxico generalizada. Espelha os campos de [`super::room::Element`]
/// (`term`/`gloss`/`refs`) mas acrescenta `audio`, `relations`, `examples` e uma
/// posição **engine-neutra** — sem duplicar a persistência markdown do léxico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackEntry {
    /// O símbolo (palavra, nome de nota/acorde, termo de domínio).
    pub term: String,
    /// Glosa/significado em PT-BR (ou referência ao próprio som, na música).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    /// Sub-tipo livre dentro do pacote: `note`/`chord`/`motif`/`scale`/`word`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    /// A dimensão de áudio — `None` = entrada muda (fallback silencioso).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<EntryAudio>,
    /// Posição abstrata no "mundo" do pacote.
    pub pos: AbstractPos,
}

impl PackEntry {
    pub fn new(term: impl Into<String>, pos: AbstractPos) -> Self {
        Self {
            term: term.into(),
            gloss: None,
            role: None,
            refs: Vec::new(),
            relations: Vec::new(),
            examples: Vec::new(),
            audio: None,
            pos,
        }
    }

    pub fn with_gloss(mut self, g: impl Into<String>) -> Self {
        self.gloss = Some(g.into());
        self
    }

    pub fn with_role(mut self, r: impl Into<String>) -> Self {
        self.role = Some(r.into());
        self
    }

    pub fn with_audio(mut self, a: EntryAudio) -> Self {
        self.audio = Some(a);
        self
    }

    pub fn with_relation(mut self, r: Relation) -> Self {
        self.relations.push(r);
        self
    }
}

/// Estatística de informação do pacote (gancho p/ a camada Shannon — bits por
/// linguagem). `bits_per_symbol = log2(symbols)` para um alfabeto uniforme.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntropyStats {
    pub symbols: usize,
    pub bits_per_symbol: f64,
}

impl EntropyStats {
    /// Entropia de um alfabeto uniforme de `symbols` símbolos.
    pub fn uniform(symbols: usize) -> Self {
        let bits_per_symbol = if symbols > 1 {
            (symbols as f64).log2()
        } else {
            0.0
        };
        Self {
            symbols,
            bits_per_symbol,
        }
    }
}

/// Um pacote de léxico jogável e que **soa**. Adicionar uma linguagem = soltar
/// um destes (idioma, música, domínio) — o mesmo contrato alimenta canvas 2D e
/// um cliente 3D futuro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexiconPack {
    pub id: String,
    pub kind: PackKind,
    /// Sistema de notação (`12tet`, `ipa`, livre por domínio).
    pub notation: String,
    /// Título legível.
    pub title: String,
    /// Dica de tema visual (engine-neutra — só um nome).
    pub theme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entropy_stats: Option<EntropyStats>,
    pub entries: Vec<PackEntry>,
}

impl LexiconPack {
    /// Procura uma entrada pelo `term`.
    pub fn entry(&self, term: &str) -> Option<&PackEntry> {
        self.entries.iter().find(|e| e.term == term)
    }
}

/// Frequência (Hz) de uma nota nomeada (`"A4"`, `"C#5"`, `"Eb3"`) em 12-TET.
/// `None` se o nome não casa. A4 = [`A4_HZ`].
pub fn note_freq(name: &str) -> Option<f64> {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let letter = bytes[0].to_ascii_uppercase();
    // semitom da letra a partir de C
    let base = match letter {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let mut idx = 1;
    let mut semis = base;
    while idx < bytes.len() {
        match bytes[idx] {
            b'#' => semis += 1,
            b'b' => semis -= 1,
            _ => break,
        }
        idx += 1;
    }
    let octave: i32 = std::str::from_utf8(&bytes[idx..]).ok()?.parse().ok()?;
    // MIDI: C4 = 60, A4 = 69. midi = (octave+1)*12 + semis
    let midi = (octave + 1) * 12 + semis;
    Some(A4_HZ * 2f64.powf((midi - 69) as f64 / 12.0))
}

/// Constrói o `EntryAudio` de uma nota (uma voz) a partir do nome.
fn note_audio(name: &str, waveform: Waveform, duration_ms: u32) -> EntryAudio {
    EntryAudio::Synth {
        waveform,
        freqs: note_freq(name).into_iter().collect(),
        envelope: Adsr::default(),
        duration_ms,
    }
}

/// Constrói o `EntryAudio` de um acorde (várias vozes juntas).
fn chord_audio(names: &[&str], waveform: Waveform, duration_ms: u32) -> EntryAudio {
    EntryAudio::Synth {
        waveform,
        freqs: names.iter().filter_map(|n| note_freq(n)).collect(),
        envelope: Adsr::default(),
        duration_ms,
    }
}

/// Pacote-seed de **música** (`kind = music`): a escala maior de Dó, ≥3 acordes
/// e 1 motivo — cada um com `audio` de síntese. A "palavra" é o som.
pub fn music_pack() -> LexiconPack {
    const SCALE: [&str; 8] = ["C4", "D4", "E4", "F4", "G4", "A4", "B4", "C5"];
    let mut entries: Vec<PackEntry> = Vec::new();

    // Escala — uma entrada "resumo" + cada grau como nota tocável.
    entries.push(
        PackEntry::new("Escala de Dó maior", AbstractPos::new(0.0, 0.0, 0.0))
            .with_role("scale")
            .with_gloss("Os sete graus da tonalidade de Dó maior (mais a oitava).")
            .with_audio(EntryAudio::Sequence {
                waveform: Waveform::Triangle,
                steps: SCALE
                    .iter()
                    .map(|n| SeqStep {
                        freqs: note_freq(n).into_iter().collect(),
                        duration_ms: 220,
                    })
                    .collect(),
                envelope: Adsr::default(),
            }),
    );
    for (i, name) in SCALE.iter().enumerate() {
        entries.push(
            PackEntry::new(*name, AbstractPos::new((i as f64) + 1.0, 1.0, 0.0))
                .with_role("note")
                .with_gloss(format!("Nota {name} — {:.2} Hz.", note_freq(name).unwrap()))
                .with_audio(note_audio(name, Waveform::Triangle, 500)),
        );
    }

    // ≥3 acordes da tonalidade.
    let chords: [(&str, &[&str], &str); 4] = [
        ("I — Dó maior", &["C4", "E4", "G4"], "Tônica."),
        ("IV — Fá maior", &["F4", "A4", "C5"], "Subdominante."),
        ("V — Sol maior", &["G4", "B4", "D5"], "Dominante."),
        ("vi — Lá menor", &["A4", "C5", "E5"], "Relativa menor."),
    ];
    for (i, (name, notes, gloss)) in chords.iter().enumerate() {
        let mut e = PackEntry::new(*name, AbstractPos::new((i as f64) * 2.0, 3.0, 0.0))
            .with_role("chord")
            .with_gloss(*gloss)
            .with_audio(chord_audio(notes, Waveform::Sawtooth, 900));
        for n in notes.iter() {
            e = e.with_relation(Relation::new(*n, "voz"));
        }
        entries.push(e);
    }

    // 1 motivo: I–V–vi–IV resumido como melodia (o "som-âncora" do pacote).
    entries.push(
        PackEntry::new("Motivo I–V–vi–IV", AbstractPos::new(0.0, 5.0, 0.0))
            .with_role("motif")
            .with_gloss("A progressão pop mais comum, como motivo melódico.")
            .with_audio(EntryAudio::Sequence {
                waveform: Waveform::Square,
                steps: [
                    (&["C4", "E4", "G4"][..], 480),
                    (&["G4", "B4", "D5"][..], 480),
                    (&["A4", "C5", "E5"][..], 480),
                    (&["F4", "A4", "C5"][..], 720),
                ]
                .iter()
                .map(|(notes, dur)| SeqStep {
                    freqs: notes.iter().filter_map(|n| note_freq(n)).collect(),
                    duration_ms: *dur,
                })
                .collect(),
                envelope: Adsr::default(),
            }),
    );

    LexiconPack {
        id: "musica".to_string(),
        kind: PackKind::Music,
        notation: "12tet".to_string(),
        title: "Música — a primeira linguagem áudio-nativa".to_string(),
        theme: "aurora".to_string(),
        // 12 semitons no alfabeto cromático.
        entropy_stats: Some(EntropyStats::uniform(12)),
        entries,
    }
}

/// Pacote-seed de **idioma** (`kind = language`): poucas palavras Mbyá Guaraní
/// com pronúncia (IPA + Web Speech) — prova de que a dimensão de áudio serve
/// idioma E música pelo mesmo contrato.
pub fn language_pack() -> LexiconPack {
    // (termo, glosa, IPA, texto-falado)
    let words: [(&str, &str, &str, &str); 4] = [
        ("ñe'ẽ", "palavra; fala; alma-linguagem", "ɲẽˈʔẽ", "nhe en"),
        ("ayvu", "linguagem; som-fundamento", "aɁɨˈvu", "aivu"),
        ("kuarahy", "sol", "kwaɾaˈhɨ", "kuarai"),
        ("yvoty", "flor", "ɨʋoˈtɨ", "ivoti"),
    ];
    let entries = words
        .iter()
        .enumerate()
        .map(|(i, (term, gloss, ipa, spoken))| {
            PackEntry::new(*term, AbstractPos::new(i as f64, 0.0, 0.0))
                .with_role("word")
                .with_gloss(*gloss)
                .with_audio(EntryAudio::Speech {
                    text: spoken.to_string(),
                    ipa: Some(ipa.to_string()),
                    lang: "gn".to_string(),
                })
        })
        .collect::<Vec<_>>();

    LexiconPack {
        id: "guarani-mbya".to_string(),
        kind: PackKind::Language,
        notation: "ipa".to_string(),
        title: "Guaraní Mbyá — idioma que soa".to_string(),
        theme: "floresta".to_string(),
        entropy_stats: None,
        entries,
    }
}

/// Resumo leve de um pacote (p/ listagem sem carregar as entradas — lazy-load).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackSummary {
    pub id: String,
    pub kind: PackKind,
    pub title: String,
    pub theme: String,
    pub entries: usize,
}

impl PackSummary {
    fn of(p: &LexiconPack) -> Self {
        Self {
            id: p.id.clone(),
            kind: p.kind,
            title: p.title.clone(),
            theme: p.theme.clone(),
            entries: p.entries.len(),
        }
    }
}

/// Todos os pacotes-seed embutidos. Sintetizados sob demanda (memory-light).
pub fn seed_packs() -> Vec<LexiconPack> {
    vec![music_pack(), language_pack()]
}

/// Resumos dos pacotes-seed (listagem leve).
pub fn seed_summaries() -> Vec<PackSummary> {
    seed_packs().iter().map(PackSummary::of).collect()
}

/// Acha um pacote-seed por id.
pub fn seed_pack(id: &str) -> Option<LexiconPack> {
    seed_packs().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_freq_referencia_a4() {
        assert!((note_freq("A4").unwrap() - 440.0).abs() < 1e-9);
    }

    #[test]
    fn note_freq_oitava_dobra() {
        let a4 = note_freq("A4").unwrap();
        let a5 = note_freq("A5").unwrap();
        assert!((a5 - a4 * 2.0).abs() < 1e-6);
    }

    #[test]
    fn note_freq_meio_tom() {
        // C5 está 3 semitons acima de A4.
        let c5 = note_freq("C5").unwrap();
        let esperado = 440.0 * 2f64.powf(3.0 / 12.0);
        assert!((c5 - esperado).abs() < 1e-6);
        // acidentes
        assert!(note_freq("C#5").unwrap() > c5);
        assert!((note_freq("Db5").unwrap() - note_freq("C#5").unwrap()).abs() < 1e-9);
    }

    #[test]
    fn note_freq_nome_invalido() {
        assert_eq!(note_freq(""), None);
        assert_eq!(note_freq("H4"), None);
        assert_eq!(note_freq("Cx"), None);
    }

    #[test]
    fn music_pack_tem_escala_acordes_e_motivo() {
        let p = music_pack();
        assert_eq!(p.kind, PackKind::Music);
        let notes = p
            .entries
            .iter()
            .filter(|e| e.role.as_deref() == Some("note"));
        assert_eq!(notes.count(), 8, "8 graus tocáveis");
        let chords = p
            .entries
            .iter()
            .filter(|e| e.role.as_deref() == Some("chord"))
            .count();
        assert!(chords >= 3, "≥3 acordes, achei {chords}");
        let motifs = p
            .entries
            .iter()
            .filter(|e| e.role.as_deref() == Some("motif"))
            .count();
        assert_eq!(motifs, 1, "exatamente 1 motivo");
    }

    #[test]
    fn cada_entrada_de_musica_tem_audio_de_sintese() {
        let p = music_pack();
        for e in &p.entries {
            let audio = e.audio.as_ref().expect("entrada de música deve soar");
            // música nunca usa Speech — é áudio-nativa (síntese).
            assert!(
                !matches!(audio, EntryAudio::Speech { .. }),
                "{} usou Speech",
                e.term
            );
        }
    }

    #[test]
    fn acorde_toca_varias_vozes() {
        let p = music_pack();
        let do_maior = p.entry("I — Dó maior").unwrap();
        match do_maior.audio.as_ref().unwrap() {
            EntryAudio::Synth { freqs, .. } => assert_eq!(freqs.len(), 3, "tríade = 3 vozes"),
            other => panic!("acorde deveria ser Synth, foi {other:?}"),
        }
    }

    #[test]
    fn nota_toca_uma_voz() {
        let p = music_pack();
        match p.entry("A4").unwrap().audio.as_ref().unwrap() {
            EntryAudio::Synth { freqs, .. } => {
                assert_eq!(freqs.len(), 1);
                assert!((freqs[0] - 440.0).abs() < 1e-9);
            }
            other => panic!("nota deveria ser Synth, foi {other:?}"),
        }
    }

    #[test]
    fn language_pack_carrega_pronuncia() {
        let p = language_pack();
        assert_eq!(p.kind, PackKind::Language);
        assert!(!p.entries.is_empty());
        for e in &p.entries {
            match e.audio.as_ref().expect("palavra deve ter pronúncia") {
                EntryAudio::Speech { ipa, lang, text } => {
                    assert!(ipa.is_some(), "IPA presente p/ {}", e.term);
                    assert!(!text.is_empty());
                    assert_eq!(lang, "gn");
                }
                other => panic!("idioma deveria ser Speech, foi {other:?}"),
            }
        }
    }

    #[test]
    fn pacote_serializa_round_trip() {
        let p = music_pack();
        let json = serde_json::to_string(&p).unwrap();
        // o áudio é tagueado por `mode`
        assert!(json.contains("\"mode\":\"synth\""));
        assert!(json.contains("\"mode\":\"sequence\""));
        let back: LexiconPack = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn entropy_uniforme_12_semitons() {
        let s = EntropyStats::uniform(12);
        assert!((s.bits_per_symbol - 12f64.log2()).abs() < 1e-9);
        assert_eq!(EntropyStats::uniform(1).bits_per_symbol, 0.0);
    }

    #[test]
    fn seed_lookup() {
        assert!(seed_pack("musica").is_some());
        assert!(seed_pack("guarani-mbya").is_some());
        assert!(seed_pack("inexistente").is_none());
        let sums = seed_summaries();
        assert_eq!(sums.len(), 2);
        assert!(sums.iter().any(|s| s.kind == PackKind::Music));
        assert!(sums.iter().any(|s| s.kind == PackKind::Language));
    }
}
