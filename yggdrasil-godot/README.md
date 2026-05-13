# yggdrasil-godot

POC Godot 4.5 — substrato avaliado para os universos do Yggdrasil. Trilho B
do epic [YG-22](../work/yggdrasil/YG-22.md), tarefas YG-31..YG-35.

> **Status:** scaffold. Nenhum jogo aqui ainda — apenas uma scene "Olá
> universo" que valida que a toolchain (editor + export web + export
> headless + Docker) está sã. Lógica de lobby entra em YG-32; signal bus
> autoritativo em YG-33; multiplayer JWT em YG-34; PokerTable E2E em YG-35.

## O que está sendo avaliado

A arquitetura proposta para os universos do Yggdrasil tem quatro pilares
(ver [`docs/ARQUITETURA-UNIVERSOS.md`](../docs/ARQUITETURA-UNIVERSOS.md)):

1. **Scene tree** — cada universo é uma `.tscn` instanciável; o grafo
   `UniverseNode { Root | Variant | Composition }` mapeia 1:1 para uma
   árvore de `PackedScene.instantiate()`.
2. **Signals** — comunicação entre nós por sinais nomeados, sem
   acoplamento direto.
3. **Lazy spawn** — sub-universos só são instanciados quando o jogador
   entra no portal correspondente.
4. **Multiplayer nativo** — `MultiplayerAPI` do Godot 4 + JWT do Rust
   gateway (autenticação reaproveita o sistema YG-11 + handover CO).

Decisão de migração: ao final de YG-35, em `docs/ADR-002-godot-poc-resultado.md`.

## Layout

```
yggdrasil-godot/
├── project.godot                    # Godot 4.5, scene principal HelloUniverso
├── export_presets.cfg               # Web (HTML5) + Linux/X11 (headless)
├── scenes/
│   └── HelloUniverso.tscn           # Node2D + Label "Olá universo"
├── scripts/
│   ├── hello_universo.gd            # imprime hello from server|client
│   └── build.sh                     # exporta os dois targets
├── Dockerfile                       # multi-stage: build + slim runtime
└── .gitignore                       # .godot/, out/, *.tmp.tscn
```

## Pré-requisitos

- **Godot 4.5+** (editor para desenvolver, headless para CI/build).
  - Download: <https://godotengine.org/download/>
  - Em Linux: extrair o binário e colocar em `$PATH` como `godot`.
- **Export templates 4.5** (para `--export-release`).
  - Baixar na mesma página de releases (`Godot_v4.5-stable_export_templates.tpz`).
  - Instalar via `Project → Tools → Manage Export Templates` no editor,
    ou descompactar em `~/.local/share/godot/export_templates/4.5.stable/`.

## Abrir no Godot Editor

```bash
cd yggdrasil-godot
godot --editor .
```

A primeira vez vai importar os recursos e criar `.godot/` (gitignored).
O editor abre na cena `HelloUniverso.tscn`. F5 roda a cena — o console
imprime `hello from client` (não há server local).

## Buildar os dois targets localmente

```bash
cd yggdrasil-godot
./scripts/build.sh           # ambos: web + headless
./scripts/build.sh web       # somente HTML5 em out/web/
./scripts/build.sh headless  # somente binário Linux em out/headless/
```

O script falha cedo com mensagem clara se `godot` não está no `$PATH`.

## Rodar o headless localmente

```bash
./scripts/build.sh headless
./out/headless/yggdrasil-godot --headless --rendering-driver dummy
# Saída esperada:
# hello from client
```

Quando YG-34 adicionar o flag `--server`, a mesma binary terá modos
distintos para host e client; por ora, sem multiplayer ativo, sempre
imprime `hello from client`.

## Servir o build web localmente

Godot 4 exige cross-origin isolation headers (COOP/COEP) para
`SharedArrayBuffer`. Use `python3 -m http.server` apenas para
sanity-check; para teste real, sirva via servidor que injete:

- `Cross-Origin-Opener-Policy: same-origin`
- `Cross-Origin-Embedder-Policy: require-corp`

```bash
./scripts/build.sh web
cd out/web && python3 -m http.server 8000
# abrir http://localhost:8000/
```

## Docker

```bash
docker build -t yggdrasil-godot .
docker run --rm -p 3031:3031 yggdrasil-godot
```

A imagem final é `debian:trixie-slim` + binário headless + assets web.
Porta `3031` é dedicada ao POC Godot (a `3030` é do `yggdrasil-web`
Rust — coexistem sem conflito).

## Não-objetivos desta tarefa (YG-31)

- Multiplayer (vem em YG-34).
- Lobby com portais para outras scenes (vem em YG-32).
- Deploy no Fly (vem em YG-34 quando a autenticação for resolvida).
- Lógica de jogo qualquer (Pôquer E2E entra em YG-35).

Veja [`work/yggdrasil/YG-31.md`](../work/yggdrasil/YG-31.md) para o
critério de aceitação completo desta tarefa.
