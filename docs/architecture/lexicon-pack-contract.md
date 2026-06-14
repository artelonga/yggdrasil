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

## O que NÃO é deste contrato (próximas tarefas)

- O jogo/quests completo e o render *walkable* do pacote no Mundo (YG-148+).
- O cliente 3D/voxel em si — aqui só garantimos que o contrato **não o trava**.
- Packs Hebraico/neuro completos — entram pelo mesmo `kind` quando chegarem.
