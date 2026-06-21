# ÑE'Ẽ — The Lexicon Worlds — design (YG-155 follow-ups)

> Design doc que costura as tarefas de seguimento do keystone **YG-155**
> ("ÑE'Ẽ keystone — LexiconPack"). A quilha já existe; este doc descreve a
> **visão → mecânicas → pipeline de conteúdo** que a transformam num **jogo**
> caminhável e gamificado, mantendo os invariantes do dono: **memory-light** e
> **engine-neutro** (pronto p/ um cliente 3D futuro).

## 1. O que já existe (YG-155, DONE)

- **Contrato** `LexiconPack` em `yggdrasil-core/src/comunicacao/pack.rs`: uma
  *linguagem* = **qualquer sistema de símbolos com um léxico** (`kind =
  language | music | domain`). Cada `PackEntry` tem `term/gloss/role/relations`,
  uma **posição abstrata** `{x,y,z}` (engine-neutra) e uma **dimensão de áudio**
  opcional (`EntryAudio`: `Synth` / `Sequence` / `Speech` — sempre **parâmetros
  de síntese, nunca samples**).
- **Packs seed** (hardcoded): `music_pack()` (escala + acordes + motivo, áudio
  synth) e `language_pack()` (Mbyá, áudio speech). `EntropyStats` (bits/símbolo)
  já no contrato como gancho Shannon.
- **Camada de SOM** no front: `static/universos/nee/audio-engine.js` (Web Audio,
  osciladores one-shot, `MAX_VOICES`, política de autoplay) + `pack-loader.js`
  (lazy-load, `MAX_RESIDENT_ENTRIES`).
- **Rotas**: `GET /api/v1/comunicacao/packs` e `/packs/{id}` (síntese on-read).
- **Contrato documentado**: `docs/architecture/lexicon-pack-contract.md`.

**O que falta** (escopo explicitamente adiado pelo YG-155): autoria de packs sem
recompilar, render *walkable* do pack, o loop de **bits/score**, o hook de música
como jogo, e a ponte **conteúdo CO → pack**. É o que estas tarefas cobrem.

## 2. Visão

Caminhar **dentro de uma linguagem** e **ouvi-la**. Cada linguagem (idioma,
música, domínio) é um mundo: você anda entre símbolos, cada passo **soa**, o
significado emerge das **relações**, e **aprender rende bits** (Shannon) que
compram pistas. Música é especial — a linguagem áudio-nativa — então **andar é
compor**. O conteúdo canônico mora no **CO** (markdown); o Yggdrasil é a **forma
jogável**. Separação conteúdo×forma, com round-trip de leitura.

## 3. Mecânicas

| Mecânica | Tarefa | Resumo |
|---|---|---|
| **Pack caminhável** | YG-171 | `packToRoom(pack)` → `Room` do engine (YG-148); pisar toca `audio` + inspector com glosa/relações. |
| **bits/score (Shannon)** | YG-168 | Descobrir/identificar **rende** `bits_per_symbol`; **revelar** glosa **gasta** bits. Saldo por usuário. |
| **Música = sequencer espacial** | YG-170 | Andar a um BPM dispara passos (`Sequence`); gravar a trilha andada como motivo; opcional salvar como pack. |

O laço central: **descobrir → testar → gastar**. A informação de uma linguagem
(bits) é literalmente a recompensa por aprendê-la; bits têm utilidade (pistas),
então há economia. Music adiciona um laço próprio (compor andando) sobre o mesmo
contrato — prova de que `kind` pode ter mecânicas específicas sem fork do modelo.

## 4. Pipeline de conteúdo

```
  Autor humano                       Universo CO (comunicacao)
  data/packs/<id>.yaml  ──┐          <lingua>/terms/<slug>.md  (markdown canônico)
      (YG-166)            │                    │ pack_for_language (YG-169, read-only)
                          ▼                    ▼
                    ┌──────────────── LexiconPack (contrato YG-155) ───────────┐
                    │   kind · notation · entropy_stats · entries[ audio,pos ] │
                    └───────────────────────────┬──────────────────────────────┘
                          GET /api/v1/comunicacao/packs[/{id}|/lang/{plano}]
                                                 │ lazy-load (pack-loader.js)
                          ┌──────────────────────┴───────────────────────┐
                   packToRoom (YG-171)                          AudioEngine (YG-155)
                   Room → engine.js (2D canvas)                 Synth/Sequence/Speech
                   [futuro: mesmo Room → cliente 3D/voxel]      [futuro: synth nativo]
                                                 │
                                       bits/score (YG-168)  ·  música = compor (YG-170)
```

Duas **fontes** de pack convergem no mesmo contrato:

1. **Autoria por arquivo** (YG-166): YAML em `data/packs/`, com atalhos de
   notação (`note: "C4"`, `chord: [...]`) — adicionar linguagem = soltar arquivo,
   sem recompilar.
2. **Projeção do CO** (YG-169): o léxico markdown de `comunicacao`
   (yoruba/guarani-mbya/portuguese) é **lido** (nunca escrito) e projetado em um
   `LexiconPack` com `audio: Speech` — conteúdo real do CO vira mundo.

Ambas alimentam **os mesmos consumidores** (render YG-171, áudio YG-155, score
YG-168). Os seed embutidos continuam como fallback memory-light.

## 5. Invariantes (não-negociáveis do dono)

- **Memory-light**: áudio é **síntese (params)**, nunca bytes de sample; packs
  são **lazy-load** por sala; teto `MAX_RESIDENT_ENTRIES` (LRU) e `MAX_VOICES`;
  projeções/packs de arquivo são **on-read**, sem estado pesado residente.
- **Engine-neutro**: o contrato `pack → mundo` e `entrada → objeto + som` **não
  assume 2D**. `pos.z` é preservado mesmo ignorado no canvas; o adaptador
  `packToRoom` e o `AudioEngine` são "dado→dado" / "params→som" — um cliente
  3D/voxel consome o **mesmo** JSON. Nenhuma tarefa fecha a porta do 3D.
- **Conteúdo×forma**: markdown do CO é canônico; o jogo **lê** (write-back fica
  no fluxo `publicar`/curadoria do `comunicacao`, fora deste escopo).

## 6. Ordem sugerida

1. **YG-166** (autoria por arquivo) — destrava conteúdo sem recompilar; base p/ 169/170.
2. **YG-171** (walkable) — o palco; depende do engine YG-148 + áudio YG-155.
3. **YG-168** (bits/score) — gamificação sobre o palco.
4. **YG-169** (CO→pack) — conteúdo real; consome o loader 166 e o render 171.
5. **YG-170** (música caminhável) — mecânica específica de `kind=music`; sobre 171/168.

## 7. Riscos / questões em aberto

- **Layout determinístico** (171/169): mapear `pos` abstrata → tiles sem cruzar
  objetos nem ficar ilegível para packs grandes ainda não está especificado.
- **Balanceamento de bits** (168): os números (ganho/custo) são chutes iniciais;
  precisam de telemetria (YG-145) p/ ajuste — começar simples.
- **Persistência de score/motivos**: hoje em disco junto das salas
  (`YGGDRASIL_COMUNICACAO_DIR`); migra p/ o storage definitivo quando existir.
- **Autoplay/Web Speech**: pronúncia depende de vozes da plataforma; fallback
  silencioso já é o contrato, mas a cobertura varia por navegador.
