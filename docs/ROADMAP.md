# Yggdrasil — Roadmap

> Estado real em **2026-06-13** (v2.24.0 em produção). Este doc foi reescrito
> porque a versão anterior (2026-05-23) planejava v1.1→v2.0 com fatos já
> ultrapassados. Visão geral viva; detalhe por release no `CHANGELOG.md` e a
> sequência cross-repo no `docs/release-pipeline.md`.

## Onde estamos

Plataforma de universos digitais, single-binary em `yggdrasil-artelonga.fly.dev`.
Desde a v1.0 (2026-05-20) shipou muito além do plano original:

- **Federação CO ⟷ Yggdrasil ao vivo** (YG-117..122, v2.7) — notas e termos
  federam pelo bridge EDA; handshake `?source=&token=` + User-Agent (CO-397).
- **Conteúdo alcançável** (v2.8) — criação de universo, editor popup, rascunho
  cross-device (YG-125), links seguros por hash.
- **Modelo unificado por composição** (YG-126..131) — um tipo de universo;
  views de runtime (Mapa/Timeline/Grafo); nó = nota (pasta/índice/artigo são
  render); tarefa = nota + status; manipulação direta + árvore TUI.
- **Analytics ao vivo** (YG-127/128, v2.15–2.20) — tracker privacy-first → hub
  do CO; `/analytics` com stream WS; retenção 90d.
- **Ayvu Rapyta / comunicação** (YG-132..141) — corpus ligado ao léxico completo
  (4.837) com pivô de glosa; concordância; escolher sentença; lounge com rank +
  popularidade. **Framework NLP de corpus** (YG-139, DuckDB): corpora nomeados,
  frequência, joins cross-linguísticos (`/universos/corpus-lab`).
- **Perfil universal de usuário** (YG-137) — um perfil, universos compõem por cima.
- **Catálogo** (YG-68) — REGISTRY + filtros + Shandara + ~40 RPGs BR.
- **Qualidade** — suíte Playwright (`e2e/`) plugada no CI (YG-142); fixture
  self-contida; co_graph vendorizado (sem dep de host externo).

## Aberto — por prioridade

### Lacunas de produto (north-star)
- **Superfície de campanha / launch** (Catarse-style) — `docs/REWARDS.md` existe,
  mas não há página de campanha nem integração de recompensas. *Maior ausência
  para o objetivo declarado; sem task.*
- **Reader de SRD do Shandara** — catálogo anuncia `playable`, nada renderiza o
  SRD. *Sem task.*

### Round-trip com o CO (bloqueado lá)
- **YG-124** — editar nota no editor do CO (deep-link + round-trip).
- **YG-138** — universo criado pelo usuário vira universo no CO + convite /
  público-subscribe. Ambos dependem de API user-facing de criação de
  universo/parent_key no CO (hoje só interna).

### Fast-follow rastreado
- **YG-127 F-ops** — token dos rollups (1 comando) para ligar os agregados de
  jogo no painel do CO.
- **YG-139 F3/F4** — salvar-resultado-de-join como corpus (Parquet, federável ao
  CO); módulos "game"/pedagógicos (estilo SensorySpeech) + sugestão de tradução PT.
- **YG-113 / YG-115** — curadoria de sugestões de verso; etimologia (NOTAS de
  Cadogan). Conteúdo.
- **YG-71** — changelog por universo via git pathspec.
- **YG-28** — poker WebSocket (substituir polling). Low.

### Dívidas técnicas
- `scripts/build-universes.sh` não roda no bash 3.2 do macOS (`declare -A`) —
  só Linux/CI.
- Trilho Godot (YG-32..35) — decidido pós-launch (ADR `docs/adr/`).
- `docker build` ainda não roda no CI (precisa de co/game-core + comunicacao no
  layout-pai; o `cargo build --release` cobre o binário, não o Dockerfile).

## Princípios estabelecidos (não regredir)
- **Composição sobre herança**: toda funcionalidade do editor é campo/render/
  gesto sobre *nota* — nunca tipo novo (ver `[[instance-unified-model]]`).
- **Conteúdo alcançável**: nada público a mais de um clique de uma navegação
  visível (checklist em `docs/experiencia-usuario-exemplo.md`).
- **Canônico vs. derivado**: markdown/JSON é a verdade; índices (léxico, corpus
  DuckDB, telemetria) são derivados reconstruíveis no boot.
- **Release**: bump no `Cargo.toml` raiz confirmado no staging; `flyctl deploy`
  sem pipe (exit real); `/version` em prod tem de bater (ver memória
  `deploy-sequencing`).
