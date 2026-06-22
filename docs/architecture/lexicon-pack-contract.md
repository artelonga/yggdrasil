# Contrato `LexiconPack` — engine-neutro (YG-155)

> A **quilha** do ÑE'Ẽ ("The Lexicon Worlds"). Uma *linguagem* é **qualquer
> sistema de símbolos com um léxico**: idioma natural, **música** ou um domínio
> (neuro, Hebraico ritual…). Cada entrada pode **soar**. Adicionar uma linguagem
> = soltar um pacote.

## Por que existe

O léxico original (`yggdrasil-core/src/comunicacao/lexicon.rs`) é texto preso a
"idioma natural", sem som e sem uma abstração que escale. O `LexiconPack`
generaliza isso e dá a cada entrada uma **dimensão de áudio** — com a **música
como a primeira linguagem áudio-nativa** (a "palavra" *é* o som).

## O contrato (renderer-agnóstico)

Definido em `yggdrasil-core/src/comunicacao/pack.rs`. **Nenhum campo assume
renderização 2D.** O canvas/DOM 2D de hoje é apenas UM consumidor; um cliente
3D/voxel ("minecraft-like") futuro consome o **mesmo** JSON.

```
LexiconPack {
  id, kind: language | music | domain,
  notation,            // "12tet" | "ipa" | livre por domínio
  title, theme,        // theme = só um NOME (dica visual), não pixels
  entropy_stats?,      // gancho Shannon: bits por símbolo
  entries: [ PackEntry ]
}

PackEntry {
  term, gloss?, role?,          // role: note | chord | motif | scale | word | …
  refs[], relations[], examples[],
  audio?,                       // a dimensão de SOM (ver abaixo) — None = mudo
  pos: { x, y, z }             // posição ABSTRATA (z default 0)
}

EntryAudio (tagged por `mode`) — sempre PARÂMETROS de síntese, nunca samples:
  synth    { waveform, freqs[], envelope(ADSR), duration_ms }   // nota=1 voz, acorde=N
  sequence { waveform, steps[ {freqs[], duration_ms} ], envelope } // motivo/melodia
  speech   { text, ipa?, lang }                                  // pronúncia (Web Speech)
```

### Pontos engine-neutros

- **Posição abstrata** `{x, y, z}` — não é pixel nem célula. O consumidor 2D
  ignora `z`; um cliente 3D usa as três. Mapear pos → tela/voxel é do renderer.
- **`entry → objeto + som`**: o pacote diz *o que* é (term/role/relations) e
  *como soa* (audio); **como desenhar** é decisão do cliente (tema = só um nome).
- **Áudio = descrição, não bytes**: `freqs`/`waveform`/ADSR são tocados ao vivo
  por `OscillatorNode` (web) — ou por qualquer sintetizador num cliente nativo.
  Sem MB de samples no bundle/git (memory-light).

## Os dois consumidores

| Camada | Hoje (2D) | Futuro (3D/voxel) |
|---|---|---|
| Carregar pacote | `static/universos/nee/pack-loader.js` | mesmo contrato HTTP `/api/v1/comunicacao/packs/{id}` |
| Tocar som | `static/universos/nee/audio-engine.js` (Web Audio) | mesmo `EntryAudio` → sintetizador nativo |
| Render | `nee.js` (DOM) / canvas | engine 3D (não fecha a porta) |

O back-end (`comunicacao_routes.rs`) **sintetiza os pacotes sob demanda** (sem
estado pesado) e os serve em JSON. Lazy-load por pacote no cliente; cache LRU com
teto `MAX_RESIDENT_ENTRIES` (definido no core, espelhado no `pack-loader.js`).

## Memory-light (requisito do dono)

1. **Sem samples** — áudio é síntese (params), não áudio gravado.
2. **Lazy-load** — pacote/entradas só carregam ao entrar na sala (reusa YG-151).
3. **Cap de residentes** — `MAX_RESIDENT_ENTRIES` + descarte LRU no cliente;
   teto de vozes simultâneas (`MAX_VOICES`) no `AudioEngine`.
4. **Sem cópia desnecessária** — relações referenciam `term` (id), não blobs.

## Formato de autoria por arquivo (YG-166)

Criar um pack **não requer Rust nem recompilação**. Basta soltar um `*.yaml` em
`data/packs/` (ou no diretório apontado por `YGGDRASIL_PACKS_DIR`) e reiniciar
o servidor. O loader (`pack_file::load_dir`) lê todos os YAML do diretório e os
mescla com os packs-seed embutidos — sem colisão, pois o `id` do arquivo
prevalece por ordem de leitura.

### Schema do arquivo (`PackFile`)

```yaml
id: meu-pack           # único; sem espaços (usado na URL /packs/{id})
kind: music            # language | music | domain
notation: 12tet        # "12tet" | "ipa" | livre por domínio
title: "Título legível"
theme: aurora          # dica visual engine-neutra (só um nome)
entropy_stats:         # opcional — gancho Shannon
  symbols: 12
  bits_per_symbol: 3.58

entries:
  - term: "nome"
    gloss: "significado em PT-BR"   # opcional
    role: note                       # opcional: note|chord|motif|scale|word|…
    pos: { x: 0.0, y: 0.0 }        # z omitido = 0 (posição abstrata)
    audio: <atalho — ver abaixo>
    relations:                       # opcional
      - to: "outro-term"
        label: "tipo"
    examples:                        # opcional
      - "frase de exemplo"
```

### Atalhos de áudio

| Atalho YAML | Resultado | Exemplo |
|---|---|---|
| `note: "A4"` | `Synth` 1 voz — nota nomeada → Hz via 12-TET | `note: "C#5"` |
| `chord: [...]` | `Synth` N vozes — acorde de notas nomeadas | `chord: ["C4","E4","G4"]` |
| `sequence: [{chord:[...], duration_ms:...}]` | `Sequence` de passos | ver abaixo |
| `speech: {text, ipa?, lang}` | `Speech` — pronúncia Web | `speech: {text: "olá", lang: "pt-BR"}` |
| `freqs: [Hz...]` | `Synth` com Hz crus (escape hatch microtonal) | `freqs: [432.0, 528.0]` |

Todos os atalhos aceitam `waveform` (`sine`|`square`|`sawtooth`|`triangle`,
default `triangle`), `duration_ms` (default `500`) e `envelope` (ADSR, default
pluck curto).

#### Exemplo de `sequence`

```yaml
audio:
  sequence:
    - chord: ["C4", "E4", "G4"]
      duration_ms: 480
    - chord: ["G4", "B4", "D5"]
      duration_ms: 480
  waveform: square
```

#### Entrada sem áudio (muda)

Omita `audio` para uma entrada que não soa — fallback silencioso no cliente.

### Arquivo-exemplo

`data/packs/exemplo.yaml` demonstra todos os atalhos (note + chord + sequence +
speech) com ≤8 entradas. Serve como template para novos packs.

### Isolamento de erros

Um arquivo YAML inválido gera aviso no stderr mas **não impede** os demais packs
de carregar. Cada pack falha isoladamente (`PackFileError` identificável por tipo:
`Io`, `Yaml`, `InvalidNote`).

## O que NÃO é deste contrato (próximas tarefas)

- O jogo/quests completo e o render *walkable* do pacote no Mundo (YG-148+).
- O cliente 3D/voxel em si — aqui só garantimos que o contrato **não o trava**.
- Packs Hebraico/neuro completos — entram pelo mesmo `kind` quando chegarem.
- UI de edição de packs no browser (autoria é arquivo por enquanto — YG-166).
- Ponte CO-universe → pack (YG-169, consome o loader `load_dir`).
