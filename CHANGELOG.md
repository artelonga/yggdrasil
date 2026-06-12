# Changelog

Todas as mudanças relevantes ao projeto Yggdrasil. Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versionamento: [SemVer](https://semver.org/lang/pt-BR/).

## [2.16.0] — 2026-06-12 — YG-128 (parte 1): /analytics — seção pública de analytics ao vivo

### Added

- **`/analytics`** no yggdrasil (padrão do dashboard público da ArteLonga): KPIs de
  site (views, visitantes, sessões, países — `summary?universe=yggdrasil` do hub do
  CO, PII-stripped, cache 5min), KPIs **vivos** de jogo (jogando agora ●, sessões
  24h — `/api/v1/stats` local, polling 10s), gráfico de visitas por dia, páginas
  mais visitadas, geografia e placares recentes. Período selecionável (1/7/30/90d).
  Nota de privacidade com opt-out documentado. Link "📈 Ao vivo" na nav global.
- Pendente na YG-128: stream WebSocket próprio, retenção agendada, redaction de
  paths privados.

## [2.15.0] — 2026-06-12 — YG-127 Fase 1: telemetria do yggdrasil no hub de analytics do CO

### Added

- **`static/telemetria.js`** (15 páginas): tracker privacy-first no contrato da
  ArteLonga — `page_view`, `page_end` (duração), `js_error` em batches ao hub do CO
  (`site: "yggdrasil"` → universe_key). vid/sid opacos aleatórios (zero PII);
  respeita DNT e opt-out (`ygg_optout=1`); fila local + `sendBeacon`; hook
  `window.yggTelemetria.track()` para eventos de app. O server-side do CO aplica
  ip_hash com salt diário, filtro de bots e retenção 90d.
- **`src/co_rollups.rs`**: push horário (upsert idempotente por dia) dos agregados
  de telemetria de JOGO (`game_sessions`, `game_completions`, por universo) para
  `co/api/v1/analytics/public/rollups` — env-gated (`YGG_CO_ROLLUP_TOKEN`; sem
  token → no-op), só agregados anônimos.
- **YG-127** (épico, Fase 1) + **YG-128** (Fase 2: página `/analytics`, stream WS
  próprio, retenção agendada + redaction de drafts) mintadas, com os requisitos
  R1–R8 derivados da revisão co/ArteLonga/quilombo.

## [2.14.1] — 2026-06-12 — Docs: experiência de usuário por exemplo

### Docs

- `docs/experiencia-usuario-exemplo.md`: jornada narrada (persona Marina) pelas
  cenas reais do produto v2.14.0 — chegada anônima, login com intenção, criação
  única, rascunho-branch/save-commit cross-device, links por hash, pastas e
  ligações tipadas, lentes Mapa/Timeline/Grafo, federação invisível — com os 8
  princípios de design e um checklist de regressão de UX.

## [2.14.0] — 2026-06-12 — YG-126: um tipo de universo, todas as formas (views de runtime)

### Changed

- **Um único modelo de criação** ("Universo": nota + pasta + evento) — revisado o
  padrão do co: view é toggle de runtime (`state.view`), nunca propriedade do
  universo. Os modelos legados (blank/jardim/neuroanatomia/timeline) saem da
  criação mas seguem resolvendo por slug (paleta de instâncias existentes intacta).
- **Player ganha seletor de views**: 🗺 Mapa | 🕐 Timeline | 🕸 Grafo sobre a MESMA
  instância. A timeline virou **lente read-only derivada** de qualquer universo:
  blocos com `props.at_iso` (evento explícito), **criação de notas** (fallback
  `created_at`, como o co faz) e a **criação do próprio universo** viram eventos,
  em faixas por família de kind — eventos de sistema futuros (scores etc., via
  bridge) já têm onde aterrissar. Cena derivada client-side espelha a régua
  `x_for`/`lane_rows` do gerador (adendo registrado na spec draft do CO-387).
- Instâncias com `projection: timeline` abrem direto na lente timeline.

## [2.13.0] — 2026-06-12 — YG-123: Projection::Timeline — mundo-timeline navegável

### Added

- **`Projection::Timeline`** (SCHEMA_VERSION 2→3, aditivo — v1/v2 seguem legíveis):
  eixo X = tempo (`props.at_iso`), uma faixa Y por família de `kind`.
- **Gerador** `instance/generators/timeline.rs`: entradas (`at_iso` + `kind`) →
  instância com uma layer por família (toggle de kind de graça), colisões empilham
  na faixa. Layout em **funções puras** (`x_for`, `lane_rows`) — candidatas
  declaradas ao crate compartilhado do CO-396; não evoluir layout aqui.
- **Renderer timeline** no player: pan (arrastar o vazio) + zoom no X (scroll,
  ancorado no cursor) sem distorcer blocos (escala o pitch das colunas, não o
  canvas — hit-test exato via inversa em `screenToCell`); eixo de tempo no rodapé
  com ticks pt-BR pela mesma régua do gerador; inspetor mostra 🕐 `at_iso` + `kind`.
- **Template `timeline`** semeado com o céu de 2026 (equinócios, solstícios, luas
  cheias, eclipse solar) — datas fixas, seed determinístico.
- **Aprendizados → spec**: `docs/architecture/co-387-time-lens-spec-draft.md`
  (contrato do crate de layout, política de colisão, o que falta para a lens do co).
- Conteúdo vivo do universo `time` continua chegando só via bridge inbound (P-B) —
  nenhum importador paralelo foi criado, conforme o escopo.

## [2.12.0] — 2026-06-12 — YG-125: rascunho server-side — a branch cross-device do editor

### Added

- **`DraftStore`** (`yggdrasil-core`): rascunhos por usuário em
  `<id>/drafts/<user-slug>/<slug>.md` — deliberadamente **fora de `notes/`**, então um
  rascunho **nunca passa pelo producer do bridge** (privacidade por construção; teste
  cobre zero `NoteWritten` em writes de draft).
- **API** `GET/PUT/DELETE /api/v1/instances/{id}/notes/{slug}/draft` (owner-only por
  JWT; 401 anônimo, 403 não-dono).
- **Editor popup**: a branch de rascunho agora sincroniza com o servidor (debounce) —
  trocar de navegador/dispositivo oferece "Há um rascunho de outro dispositivo
  (HH:MM) — Continuar / Descartar". localStorage segue como fallback offline; commit
  ou descartar dissolve a branch nos dois lados. Anônimo: 100% local, sem regressão.

## [2.11.0] — 2026-06-11 — Editor de nota em popup: rascunho-como-branch, preview ao vivo, links seguros

### Added

- **Editor de nota em popup** (espelha as affordances do editor do CO, vanilla JS):
  textarea monoespaçada + **preview de markdown ao vivo** lado a lado, fullscreen-ish,
  fecha com Esc/✕/clique fora; ⌘/Ctrl+Enter salva. Substitui o textarea espremido na
  sidebar.
- **Rascunho-como-branch**: digitar guarda rascunho local automático (por
  instância+nota+usuário, debounce 800ms) — **nada toca a nota canônica até
  "💾 Salvar (commit)"** (PUT → persiste e federa ao CO). Fechar preserva o rascunho;
  reabrir oferece "Continuar rascunho / Descartar".
- **Links seguros por hash**: `#nota=<slug>&editar=1` abre a nota (e o editor, se
  dono). Fragment não vai a logs de servidor nem Referer e não carrega credencial —
  editar continua owner-only por JWT no servidor. Botão "🔗 link" copia o deep-link.
- **YG-124** mintada: "Editar no CO" (round-trip v3.1) — bloqueada pelo gate
  `ReadOnlyUniverse` do CO em universos federados; pré-requisito CO-side documentado.

## [2.10.1] — 2026-06-11 — Static com revalidação (fim do JS velho) + editar nota sem caçar o toggle

### Fixed

- **"Pasta não funciona / não consigo editar notas" = JS velho em cache**: `/static/*`
  não mandava `Cache-Control`, então o browser segurava o `instance.js` antigo por
  heuristic caching mesmo depois do deploy (paleta/editor "sumiam"). Agora
  `Cache-Control: no-cache` — o browser guarda mas revalida (304 quando inalterado);
  cada deploy chega na hora, sem Cmd+Shift+R.
- **Editar nota não depende mais de descobrir o ✏️ no topo**: no modo visualizar, o
  dono vê "✎ Editar nota" direto no inspetor da nota (entra em edição e reabre o
  editor). Alternar o modo também re-renderiza o inspetor do bloco selecionado.

## [2.10.0] — 2026-06-11 — YG-116: Caderno do corpus sincronizado com o servidor

### Added

- **`corpus.js` ligado ao Caderno servidor (YG-116, fecha o fold-in adiado de YG-112)**:
  logado, cada ★ favorito, ✎ nota e avanço de leitura grava em
  `/api/v1/comunicacao/caderno/*` (JWT) — durável e cross-device; as notas federam
  via instância canônica do Ayvu (YG-114). No primeiro boot logado o blob do
  `localStorage` é fundido no servidor (`POST .../migrar`, idempotente) e o
  consolidado volta ao local (favoritos/notas de outros devices aparecem, com
  labels/ci/vi reconstruídos da âncora). Anônimo/offline: 100% localStorage, sem
  regressão. Progresso com debounce (1 escrita por pausa, não por tecla).

## [2.9.0] — 2026-06-11 — Pastas na grade, ligações tipadas e drag-to-link

### Added

- **📁 Pasta** na paleta (jardim e blank): simula diretórios na grade. **Arrastar uma
  nota para cima de uma pasta** cria a ligação `parent` (filho→pasta); sobre outra
  nota cria `ref`. O tipo viaja em `Connection.props.kind` — sem migração de schema.
- **Ligações separadas por tipo** com legenda clicável na sidebar: pasta (pai/filho,
  dourada), referência (ciano), wikilink (tracejada roxa) e **irmãos da mesma pasta**
  (derivada, pontilhada sutil, desligada por padrão) — "ser filho de um pai comum" é
  um tipo de ligação por si só.
- Inspetor de pasta lista o conteúdo do "diretório" (filhos via `parent`), com clique
  para abrir cada nota.

### Fixed

- **Clicar na grade não fazia nada**: o player abria em modo visualizar mesmo para o
  dono de um universo recém-criado. Agora: universo vazio do dono → entra direto em
  ✏️ Editar; clique morto em modo visualizar mostra dica em vez de silêncio.

## [2.8.1] — 2026-06-11 — Bridge: User-Agent no dial (o "400 CO-side" do YG-122)

### Fixed

- **O 400 que bloqueava o go-live da federação (YG-121/122) era o CO-397**: a abuse
  protection do CO (merged 2026-06-09, mesmo dia da investigação do YG-122) recusa
  requests sem `User-Agent` com 400 `missing_user_agent` — e o tokio-tungstenite não
  manda um por padrão. O dial agora envia `User-Agent: yggdrasil-bridge/<versão>`.
  Pré-flight contra co-staging confirmado nos dois lados: `conectado ao hub` (yggdrasil)
  + `EDA bridge: peer connected source=yggdrasil.artelonga.com.br` (CO).

## [2.8.0] — 2026-06-11 — Criar universo de verdade: página de criação + meus universos

### Added

- **`/universos/instance/new`** — a página que faltava para o conteúdo existir: escolha
  de modelo (`GET /api/v1/templates`), título, criação (`POST /api/v1/instances`) e
  redirect direto para o player. Inclui a seção **"Meus universos"** (instâncias do
  usuário logado, com badge público/privado) — antes não havia NENHUM caminho na UI
  para criar ou reabrir uma instância; o jardim de notas era inalcançável.
- Catálogo (`/universos`): botão "✦ criar universo" no topo.
- Landing: CTAs "Criar universo" deslogados agora levam a
  `/login?next=/universos/instance/new` (pós-login cai na criação, não no lobby).

## [2.7.1] — 2026-06-11 — Slugs do catálogo sem página redirecionam ao catálogo (fim dos 404)

### Fixed

- **Todos os universos sem página própria davam 404**: o catálogo (YG-68) lista 41
  universos e a landing/sidebar linka cada um para `/universos/<slug>`, mas só os
  embedded têm rota. Novo fallback `/universos/{slug}` → 307 para `/universos`
  (catálogo); rotas específicas continuam vencendo a captura. Cobre também
  `shandara` (embedded/`playable: true` no catálogo, mas ainda sem reader de SRD).
- **Dockerfile**: o builder não copiava `universes/REGISTRY.yaml` (`include_str!`
  do catálogo) — todo deploy falhava desde a v2.6.0 e prod ficou presa na 2.1.0.
  Também não copiava `yggdrasil-web/embedded/`: o `build.rs` gerava stubs de 8
  bytes e os universos WASM iriam quebrados. Os artefatos reais agora entram na
  imagem. (Commitado como parte do ciclo de deploy da 2.7.0.)

## [2.7.0] — 2026-06-11 — Bridge go-live (wire + dial CO-384) e sessão na landing

### Added — Wire congelado + producer alinhado ao contrato CO-384/389 — YG-118

- `co_bridge_producer.rs` emite o envelope canônico `BridgeMessage::Event { federated:
  FederatedEvent { event: CoEvent { … } } }` (espelha CO-380/CO-384) em vez da
  `FederatedEvent` plana anterior.
- Termos de léxico (`universe_key=comunicacao`): path inclui `_users/<user-slug>/` para
  casar o filtro do consumer CO-389; `FederatedSource::Comunicacao` carrega `user_slug`.
- Notas de instância: mantido `instances/<id>/notes/<slug>.md`, acrescentado `body_hash`
  (SHA-256 hex) ao payload.
- Atividade de sala: `Presence` → `UserJoined`; novo `TermContributed` (emitido em
  `publicar_elemento`); `Unpublished` aposentado do wire.
- `co_bridge_inbound.rs` lê o novo shape aninhado (`event.event.*`).

### Added — Dial WS real ao hub CO-384 — YG-119

- Substituído o stub `connect_and_stream` por conexão real via
  `tokio_tungstenite::connect_async` com `Sec-WebSocket-Protocol: co.eda.bridge.v1`
  (gate de env inalterado — sem `YGG_CO_BRIDGE_URL`/`TOKEN` é no-op).
- Drena o `EventLog` pendente na (re)conexão; `log.ack()` ao receber
  `BridgeMessage::Ack { event_id }` (novo no protocolo de fio).
- Roteia frames inbound para `co_bridge_inbound::apply_inbound` com echo-filter;
  heartbeat Ping 30s; reconexão com `Backoff` (1→30s).
- Teste de integração com servidor WS mock (handshake, drain, round-trip de ACK).

### Changed — Handshake alinhado ao contrato real do CO-384 — YG-120

- O dial manda `source` + `token` como query params (`?source=<node_id>&token=<token>`,
  percent-encoded), como o `bridge_ws_handler` do CO-384 de fato lê. Antes ia só no
  header `Authorization` → o CO veria token vazio (401 silencioso).
- JWKS dispensado: o CO-384 valida trust-list + token não-vazio, então Yggdrasil não
  precisa expor `/.well-known/jwks.json`. O slot YG-120 (era "JWKS") foi reescopado.

### Fixed — Handshake do dial recusado pelo axum (HTTP 400) — YG-122

- E2E local pegou o `WebSocketUpgrade` do axum recusando o handshake com 400; o teste
  mock (tungstenite lenient) mascarava. Fix + teste de regressão com servidor axum real.

### Added — Estado de sessão na landing page

- A landing (`/`) agora decodifica o mesmo `yggdrasil-jwt` do lobby: mostra o email no
  topo, troca Entrar→Sair (limpa o token e recarrega) e aponta o link lateral para a
  conta (`/login?force=1`, padrão do nav.js).
- Corrigido: os DOIS CTAs "Criar universo" são religados quando logado (via
  `[data-criar]`) — antes o CTA do hero ficava de fora por não ter id.

### Docs — Runbook de go-live da federação — YG-121

- `docs/` ganha o runbook de go-live (secrets + deploy + E2E) para ligar o bridge
  Yggdrasil ⇄ CO em produção.

## [2.6.0] — 2026-06-08 — Wave Phase-2: corpus + Caderno, catálogo, versionamento, CI honesto

Wave de 5 lanes paralelas (agentes) + decisão Godot, integrada com reconciliação manual.
Ver `docs/release-pipeline.md` (pipeline) e `docs/adr/` (decisões Godot + domain-layering).

### Added — Superfície de exploração do corpus (Ayvu Rapyta) — YG-111

- Commitada a superfície de corpus antes só-local: `public::corpus_json` (leitor do JSON baked
  `corpus/<slug>.json` do repo `comunicacao`), rota `GET /api/v1/comunicacao/corpus/{slug}` +
  `/universos/corpus`, e o cliente `corpus.html`/`corpus.js` (capítulos → versos Mbyá ⟷ Español,
  glosas/partículas/NOTAS, pan/zoom/inspector/Caderno local-first). Plane `es → spanish`.
- ADR de camadas: `docs/adr/domain-layering-co-yggdrasil-comunicacao.md` — mecanismo (plataforma
  yggdrasil) vs. policy+conteúdo (universo comunicação); conteúdo sobe por valor.

### Added — Caderno do Ayvu Rapyta persistido (servidor) + contrato de federação — YG-112 / YG-114

- Caderno (★ favoritos, ✎ notas, progresso) **durável e cross-device**, por usuário (JWT):
  `GET/PUT/DELETE /api/v1/comunicacao/caderno/...` + `POST .../migrar` (importa o Caderno do
  navegador no 1º login, idempotente). `CadernoStore` espelha o store de revisão per-user.
- As **notas** do Caderno passam pela instância canônica `ayvu-rapyta` → contrato de federação
  travado (YG-114): path `instances/ayvu-rapyta/notes/<slug>.md`, idêntico ao producer (YG-97).
  Federa sem mudanças quando o consumer CO-389 entrar no ar.
- **Backend completo; o wiring do `corpus.js` aos endpoints do Caderno servidor ficou para
  [YG-116]** — o `corpus.js` de exploração (YG-111) segue local-first por ora.

### Added — Catálogo expandido de universos — YG-68 (YG-70/72/69)

- Reframe de "lobby de 6 jogos" → **catálogo de universos abertos**, PT-BR/RPG-BR first.
- `universes/REGISTRY.yaml` + `catalog.rs` (`include_str!`+`serde_yaml`, filtros status/type/
  origin/genre/license/search); `GET /api/v1/universos` faz merge REGISTRY × runtime (compat v1.0
  por default, envelope `{universos,total,by_status}` com filtros). `/universos` ganha badges +
  filtros client-side. ~30 RPGs nacionais planejados + comerciais externos semeados.
- `universe-shandara` (novo): SRD CC-BY-SA "livro vivo" como content-reader WASM (~105 KB,
  dual-license MIT AND CC-BY-SA-4.0); `GET /api/v1/universos/shandara`.

### Added — Versionamento independente por universe — YG-63 (YG-64/65/66/67)

- Cada crate `universes/universe-*` com SemVer/CHANGELOG/licença próprios (snake/tetris/invaders/
  pointset/vim `1.0.0`, poker `0.8.0`, sdk `0.1.0`); macro `pkg_version!()` no SDK;
  `build-universes.sh` imprime tabela `Universe/Version/Size`; `GET /api/v1/universos` expõe a
  versão SemVer por universo. `docs/UNIVERSE-VERSIONING.md` (regras + tag `universe-<id>-v<X.Y.Z>`).

### Fixed — CI honesto de novo (band-aids retirados) — YG-104

- **Crates WASM v1.0 reparadas**: `universe-{snake,tetris,invaders,pointset,poker}` voltam a
  compilar p/ `wasm32` contra a API atual do `universe-sdk` (trait `Universe` + `process` export
  + host imports gated por `target_arch=wasm32` com `#[link(wasm_import_module="env")]`). Os 7
  testes `wasm_runtime` que carregam blobs reais passam.
- **`ci.yml`**: restaurados `cargo install wasm-opt` + `bash scripts/build-universes.sh` (build
  WASM real volta a gatear). O job `godot` segue `continue-on-error` — agora por **decisão de
  escopo** (spike YG-35 descopou os jogos 2D Godot; canvas é produção), não band-aid.

### Decisão — Godot vs Canvas — YG-35

- ADR `docs/adr/YG-35-godot-vs-canvas.md`: **híbrido, sem migração** — canvas/Rust é o cliente de
  produção; Godot só para o `anatomy3d`. YG-32/33/34 **superseded** (pilares de multiplayer/signal
  bus/JWT gateway nunca foram construídos no POC).

## [2.5.0] — 2026-06-07 — Curadoria de léxico — YG-101

### Added — Promover stub → léxico curado — YG-101

- **Curadoria** (`LexiconStore::promote`): um curador promove uma contribuição
  `_users/<u>/<slug>.md` (`seed_status: stub`) ao léxico **curado**
  `terms/<slug>.md` (`seed_status: reviewed` + `reviewed_by`/`reviewed_at`).
  Escrita atômica, **arquiva** (remove) o stub; idempotente (re-promover é no-op);
  `NotFound` se não há stub nem termo curado. Fecha o ciclo contribuir → revisar
  → publicar.
- **Endpoint** `POST /api/v1/comunicacao/lexico/promover` (`{ lang, user, slug }`),
  **curador-autorizado** via `YGGDRASIL_ADMIN_TOKEN` (mesma chave do feedback/
  analytics): 401 sem token configurado, **403 não-curador**, 404 termo inexistente.
- **Fila de revisão repointada**: o item do contribuidor passa a apontar p/ o
  termo curado (não reaparece como stub).
- **Write-back de curadoria** (`Writeback::commit_paths`): versiona o stub
  removido **+** o termo curado num único commit (curador-autorizado, fora do
  filtro `_users/` do write-back automático); não-bloqueante, env-gated.

## [2.4.0] — 2026-06-07 — Apply inbound de notas (P-B, build+unit) — YG-97

### Added — Notas editáveis no CO (round-trip de volta) — YG-97

- **Apply inbound** (`yggdrasil-web/src/co_bridge_inbound.rs`): a metade de volta
  do round-trip. Aplica `entry.{created,updated,deleted}` vindos do CO ao
  `NoteStore` — `created`/`updated` → `save`, `deleted` → `delete` —, tornando as
  notas **editáveis no CO**, não só read-only.
- **Path instance-qualified** (contrato): o path federado das notas passa de
  `notes/<slug>.md` (YG-93) para **`instances/<id>/notes/<slug>.md`**, p/ o
  inbound resolver a instância alvo. `parse_note_path` faz o parse (com defesa
  contra `..`/`/`). **⚠️ Mudança de wire — CO-383/384/385 precisam casar** (sem
  consumidor vivo ainda, então é seguro agora).
- **Loop-guard**: echo da própria escrita (`origin_deployment` nosso +
  `hop_count > 0`) é descartado (`is_own_echo`) — sem laço.
- **Action tree (UPSERT, file-granular)**: `decide_default` é o auto-resolve
  headless (ausente→cria, corpo igual→skip, mudou→update, deleted→remove);
  `apply_with_action` executa o verbo explícito do **CO-385** — `keep-both` grava
  `<slug>-<n>.md` (ambos retidos, sufixo livre incremental), `replace`/`skip`/etc.
- Cria um **shell de instância** se a nota inbound referencia uma instância
  ainda inexistente (idempotente).
- **E2E pende CO-385** (modal/executor de conflito CO-side). Aqui: build + 15
  testes (parse, loop-guard, escopo, auto-resolve, CRUD em disco, cada verbo).

## [2.3.0] — 2026-06-07 — Federação de mensageria (build+unit) — YG-103

### Added — Federar comunicação ao CO — YG-103

- O producer da *federated bus* (YG-93) ganha uma **segunda fonte**
  ([`FederatedSource`]): além das notas (`universe_key=yggdrasil`), emite os
  **termos de léxico** das salas de comunicação **publicadas** como `entry.*`
  `universe_key=comunicacao` `path=<lingua>/terms/<slug>.md` — mesmo envelope,
  auth e `event_log` da YG-93. O código de língua da sala (`yo`, `gn-mbya`) é
  resolvido ao diretório canônico do repo (`yoruba`, `guarani-mbya`) via
  `LexiconStore::lang_dir`, idêntico ao path do write-back (YG-100).
- **Observability de sala** (`ObservabilityEvent`, `yggdrasil.sala.*`): publicar
  uma sala emite `yggdrasil.sala.published` (+ federa seus termos); despublicar
  emite `yggdrasil.sala.unpublished`; evento não-conteúdo p/ o `/agora`, sem
  entry nem replay-log.
- **Cold-start backfill**: ao (re)conectar, `seed_comunicacao_from_store` semeia
  os termos de todas as salas publicadas (paralelo ao seed das notas Fase 0).
- Wiring: `ComunicacaoState::with_bridge` liga os canais (no-op quando o producer
  está desligado por env); emissão no `patch_sala` (transição de publicação).
- **E2E pende CO-389** (consumer CO-side de mensageria). Aqui: build + 10 testes
  de unidade/integração (path/universe por fonte, compat serde, backfill só-de-
  publicadas, observability sem universe/path, publicar↔federar, despublicar).

## [2.2.0] — 2026-06-06 — Content + Messaging GA — épico YG-94

Os dois sistemas que o Yuri enquadrou como "conteúdo / mensageria" graduam de
shadow-shipped para oficialmente rastreados + suportados. (Round-trip editável de
notas — YG-97 — e federação de mensageria — YG-103 — ficam para a v2.3.0 / CO v3.1.)

### Added — Notas de universo (Phase 0) — YG-95

- Notas em **Markdown canônico** (`/data/instances/<id>/notes/<slug>.md`) ligadas por
  `[[wikilinks]]` (semântica idêntica ao `co`), com backlinks e grafo. `NoteStore`
  (escrita atômica) + 4 rotas REST `/api/v1/instances/{id}/notes[/{slug}]` + editor
  (render Markdown, wikilinks clicáveis, arestas de wikilink no canvas, sidebar Notas).
  `SCHEMA_VERSION` 1→2 (aditivo; instâncias v1 carregam sem migração).

### Added — UX jardim — YG-96

- Template `jardim` (notes-first): paleta só-`note`, registrado em `GET /api/v1/templates`;
  busca de notas client-side na sidebar "Notas"; **visão grafo** (toggle) com notas como
  nós e wikilinks como arestas (reusa `state.graph`/`drawWikiEdge`).

### Added — Mensageria (comunicação): write-back + portal + e2e — YG-99/100/102

- **Write-back** (`comunicacao/writeback.rs`, YG-100): versiona as contribuições `_users/`
  no checkout `comunicacao` via git (env-gated por `YGGDRASIL_COMUNICACAO_WRITEBACK`,
  idempotente, disparo não-bloqueante após `publicar` + rede de segurança a cada 5 min),
  para que termos publicados persistam num redeploy.
- **Portal no lobby** (YG-99): verificado — já shipado no merge YG-73–91; pendência
  desatualizada corrigida em `docs/architecture/comunicacao.md`.
- **`scripts/e2e-comunicacao.sh`** (YG-102): review ponta-a-ponta das salas de léxico
  (login → criar sala → publicar termo → revisar) contra servidor real, com casos negativos.

### Added — Bridge event-driven Yggdrasil → CO (Fase P-A, build+unit) — YG-93

- Producer (`yggdrasil-web/src/co_bridge_producer.rs`) que publica notas ao hub do CO via
  `FederatedEvent` (CO-384): hook de emissão no `NoteStore` (canal `tokio::broadcast`),
  `event_log` com `last_delivered_event_id` + cold-start (seed das notas Fase 0), reconnect
  com backoff, loop-guard `hop_count`, **env-gated** (no-op sem `YGG_CO_BRIDGE_URL/TOKEN`).
  Nova dep `tokio-tungstenite` (rustls). **E2E pende CO-384** (hub `events/bridge` ainda não existe).

### Added — Roadmap + specs Content + Messaging — YG-94

- Specs YG-93..103, ADR de sync event-driven (`docs/architecture/event-driven-sync.md`),
  `docs/content-messaging-roadmap.md`, e a convenção `CHANGELOG-PENDING/` para waves paralelas.

## [2.1.0] — 2026-06-01 — Navegação unificada + feedback resolvível — YG-92

Origem: feedback do mural (de *yuri*) — _"unificar UI entre paginas, Universos
nao tem opcao para voltar, muito desconexo por enquanto"_.

### Added — barra de navegação global (`static/nav.js`)

- Nova barra fixa, **auto-injetável e sem dependências** (mesmo padrão de
  `feedback.js`), presente em **todas** as páginas: lobby, `/universos`,
  `/universos/*`, `/neuro` e o mural `/feedback`. Sempre oferece volta para
  **Início**, **Lobby/Mapa**, **Universos** e **Comunidade**, marcando o destino
  atual como ativo. Primeira fatia da visão "site como mapa navegável".
- Detecta páginas com cabeçalho próprio (a landing `/` tem `header.top`) e **não
  duplica** a barra. Slot de conta é ciente de login (`localStorage`): "Entrar"
  para anônimo, "Conta" para logado.

### Added — feedback resolvível — `POST /api/v1/feedback/{id}/resolve`

- Coluna `resolved` na tabela `feedback`, com **migração idempotente** via
  `ALTER TABLE` (bancos antigos ganham a coluna sem perder linhas; re-boot não
  falha).
- Endpoint admin para marcar/desmarcar uma mensagem como resolvida, protegido
  pelo mesmo `YGGDRASIL_ADMIN_TOKEN` do analytics (401 sem token, 404 id
  inexistente). O mural público exibe selo **"✓ resolvido"**; e-mail e `user_sub`
  continuam nunca expostos.

## [1.3.0] — 2026-06-01 — Índice `/universos` filtrável

### Added — `/universos`: catálogo unificado e filtrável — YG-91

- Nova página **`/universos`** — índice de **todos** os universos num só lugar,
  com **busca livre** e filtros por **categoria** (arcade · atlas · língua ·
  autorado), **jogadores** (solo/multiplayer) e **idioma**.
- Agrega no cliente quatro fontes: a lista de arcade (`GET /api/v1/universos`), o
  atlas estático **anatomia** (`/static/anatomia/`), o feed público de **salas de
  comunicação** (`GET /api/v1/comunicacao/salas?published=true`, por língua) e as
  **instâncias autoradas** (`GET /api/v1/instances?published=true`). Fontes
  ausentes degradam sem quebrar o índice.
- Rota `GET /universos` → `static/universos/index.html` + `index.js`; segue o
  padrão de páginas embutidas (`include_str!`) e o visual do `/lobby`.

## [1.2.0] — 2026-06-01 — Editor de universos + Godot/3D + neuro multiescala + Fale conosco

### Added — Landing page "Relic Archive" (UI revamp, fase 1) — YG-90

- Nova **landing page** em `/` (design `docs/mock/` — padrão preto "Relic
  Archive": superfície `#131313`, ouro `#e9c349`, CTA rosa/borgonha, Newsreader +
  Manrope, camadas tonais sem bordas 1px). O mapa em canvas segue em `/lobby`.
- **Visitante (não logado)**: barra esquerda lista os **universos** (com o melhor
  placar de cada), CTA **"Criar universo"**, **stats anônimas** (universos,
  sessões 24h, jogando agora, maior placar) e grade de **placares por jogo** —
  ícone + pontuação + quem fez.
- `GET /api/v1/stats` — endpoint **público** de agregados sem PII (sessões 24h +
  jogando agora); evita expor o `/api/v1/admin/analytics` (que continua com token).

### Fixed — neuro: licenças revisadas (distribuição pública) + deep-link de região

- **Brainstem Navigator removido** de `/neuro`: o `Copyright.txt` (NITRC/MGH)
  proíbe distribuir cópias **ou derivados** fora da organização — incompatível
  com servir publicamente. `convert_atlas.py` não o constrói mais.
- **Licenças corrigidas/atestadas** para distribuição pública: Harvard-Oxford é
  **CC BY-SA 4.0** (não a licença FMRIB restritiva — atribuição+ShareAlike,
  creditado no header de `/neuro`); AAN v2.0 é **CC0**. Registro em
  `docs/anatomia/ATTRIBUTION.md`.
- **Deep-link de região**: `/neuro?focus=<dir>:<segs>&label=` foca um núcleo/
  região específico (resto oculto) — permite índices externos abrirem o atlas
  já focado.

### Added — Fale conosco: canal de feedback por universo (YG-89)

- Botão flutuante **"💬 Fale conosco"** em todos os universos e na raiz/lobby
  (`static/feedback.js`, auto-injetado): feedback, dúvida ou sugestão.
- `POST /api/v1/feedback` — **JWT opcional**: mensagem de usuário logado grava
  `user_sub`; sem token, é anônima. **Nome e e-mail opcionais** ("se gostar de
  receber resposta, deixe seu e-mail"). Validação de tipo/mensagem/e-mail.
- Persistência na mesma SQLite (`YGGDRASIL_DB`): tabela `feedback` criada
  idempotentemente no boot (`feedback.rs`), indexada por universo e data.
- **Mural público** em `/feedback` (`GET /api/v1/feedback`) — mostra tipo,
  universo, **nome** (ou "Anônimo"), mensagem e data; **e-mail nunca** sai do
  banco (a query só lê colunas públicas). Formulário avisa que a mensagem pode
  aparecer no mural.

### Added — Drill-down macro→núcleos + camada do tronco (YG-88)

- **Clicar no encéfalo** no viewer Godot (`/anatomia`) abre o viewer de núcleos
  (`/neuro`) — salto macro→sub-anatômico (web export via `JavaScriptBridge`).
- `/neuro` ganha 2ª camada: **núcleos do tronco — Harvard AAN v2.0 (CC0)**, 16
  núcleos nomeados (DR, MnR, PAG, VTA, LC, LDTg, mRt, PBC, PnO, PTg; L/R),
  alinhada (MNI152 1mm) com a camada subcortical. `convert_atlas.py` constrói as
  duas via Zenodo/nilearn. (Brainstem Navigator avaliado e **não** incluído —
  ver Fixed acima.) Desenvolvido em worktree `feat/YG-88-...`, merge FF.

### Changed — Home/lobby: chips de universos, ícones SVG, painéis colapsáveis

- Legenda estática (desatualizada) substituída por **chips clicáveis** de todos os
  universos (Snake/Tetris/Invaders/Poker/Vim/Neuro/Comunicação) sob o mapa —
  clicar entra direto no universo.
- **Ícones SVG, não emojis** — conjunto de ícones vetoriais (`currentColor`) nos
  chips (inline, herda a cor) e nos portais do canvas (recoloridos via data-URL).
  Emojis removidos também dos rótulos de scores/atividade.
- **HIGH SCORES**, **ATIVIDADE RECENTE** e **REVIEWS** agora são painéis
  **colapsáveis** (`<details>`), fechados por padrão — sidebar mais limpa.

### Added — Neuro multiescala: POC Neuroglancer em /neuro (YG-87)

- `/neuro` agora é um viewer **Neuroglancer** (embarcado) do **atlas de núcleos
  subcorticais** (Harvard-Oxford via nilearn → **Precomputed** com cloud-volume +
  igneous), servido pela própria plataforma; 21 núcleos como segmentos 3D
  selecionáveis (auto-mesh do labelmap). Viewer Godot macro fica em `/anatomia`.
- `CorsLayer::permissive()` no `/static` (NG hosted busca os dados cross-origin).
- `scripts/build-neuro-atlas.sh` + `convert_atlas.py` reproduzem os dados
  (gitignored). Direção: viewer profundo = Neuroglancer/Precomputed, não Godot.
- Licença Harvard-Oxford: **CC BY-SA 4.0** (ver Fixed em [1.2.0]).

### Changed — Melhorias de navegação no viewer 3D (YG-86)

- Scroll de zoom **menos agressivo** (passo suave por notch).
- **Controles na tela** (HUD): girar (yaw ◄►), inclinar (pitch ▲▼), zoom (+/−),
  reset (R) — botões com hold-to-repeat.
- **Atalhos de teclado**: setas/WASD giram·inclinam, +/− zoom.
- **Clicar numa estrutura centraliza** a câmera nela (e mostra o nome).
- **Sem arrastar** para navegar (removido drag-to-orbit/pan) — navegação via
  controles + atalhos + clique.

### Added — Neuro como universo + deploy (YG-85)

- **neuro** (atlas 3D) é um universo de verdade: portal no lobby `(20,10)` (core:
  `lobby/grid.rs`/`portals.rs`/`mod.rs`, símbolo 🧠 no `lobby.js`), entrada no
  catálogo `GET /api/v1/universos` (7 universos), e rota `/universos/neuro` →
  o viewer Godot Web (`/static/anatomia/`).
- Deploy Fly (`yggdrasil-artelonga`, gru) com o bundle 3D na imagem; domínio
  `yggdrasil.artelonga.com.br` via `certs add` (DNS do dono).

### Added — Visualizador 3D servido online (YG-84)

- Export do visualizador 3D para a **web (Godot HTML5/WASM, single-thread)** →
  servível por host estático sem COOP/COEP. Servido pelo `yggdrasil-web` em
  **`/anatomia`** (308 → `/static/anatomia/`).
- `scripts/build-anatomy-web.sh` (export → `static/anatomia/`) +
  `godot.sh install-templates`. Bundle (~67 MB) gitignored, reproduzível.
- Config: preset Web `thread_support=false`, `vram_texture_compression=false`;
  `run/main_scene` → o visualizador 3D.
- Verificado em Chrome real (WebGL 2.0, renderer Compatibility) servido localmente.

### Added — Visualizador 3D interativo de anatomia (YG-83)

Pivô para 3D (estilo TeachMeAnatomy) no cliente Godot, sobre malhas open-source
reais:

- `yggdrasil-godot/scenes/anatomy3d/anatomy_viewer.tscn` + script: câmera orbital
  (girar/zoom/pan), corpo translúcido com **SNC (encéfalo + medula) por dentro**,
  slider de transparência do corpo, e click-to-pick (raycast trimesh) com rótulo.
- Malhas **BodyParts3D / DBCLS (CC-BY-SA 2.1 JP)**: pele, encéfalo (59 sub-malhas
  mescladas), medula — compartilham coordenadas, então o SNC já fica no lugar.
- `scripts/fetch-anatomy.sh` baixa/prepara as malhas (gitignored, ~37 MB);
  `assets/anatomia/ATTRIBUTION.md` credita a fonte.
- Verificado por render headless (corpo translúcido + encéfalo; toggle revela o
  SNC; orbit confirmado em frame de perfil).

## [1.1.0] — 2026-05-29 — Editor de universos data-driven (YG-73) + integração CLI do Godot (YG-82)

### Added — Integração CLI do Godot + lint headless de GDScript (YG-82)

- `yggdrasil-godot/scripts/godot.sh` — provisiona Godot 4.5 headless
  (`install`, sem GUI/sudo; em macOS remove quarentena + assina ad-hoc) e valida
  todo o GDScript (`check`) via harness `scripts/dev/lint.gd` em contexto de
  runtime (autoloads registrados, cobre scripts não referenciados, sem falsos
  positivos). Subcomandos `bin/version/import/editor/run` para dev.
- `build.sh` compartilha a resolução de binário (`$GODOT_BIN` → `.godot-bin/` →
  `$PATH`).
- CI: novo job `godot` (`install` + `check`).
- **Login no cliente Godot** (magic-link): `ApiClient.request_login_code` +
  `verify_login` (espelham `/api/v1/auth/{code,verify}`), token decodificado e
  persistido via `SaveManager`, restaurado no boot (`$GODOT_JWT` tem prioridade
  para dev/headless). `scenes/editor/grid_editor.tscn` ganha painel de login —
  surface Godot do editor agora é interativa ponta a ponta. Autoloads reordenados
  (`SaveManager` antes de `ApiClient`).

### Added — Review E2E (script + CI)

- `scripts/e2e-editor.sh` — review ponta a ponta do editor contra um servidor
  real com **autenticação real** (magic-link): login → criar de template →
  upload de anexo → EditOps → persistência → restart → re-leitura do disco.
  Aceita `$YGGDRASIL_WEB_BIN`. Rodado no CI (job `build`) sobre o binário release.

### Fixed — Três quebras pré-existentes de GDScript sob Godot 4.5 (reveladas pelo lint)

- `api_client.gd`: `_get`/`_post` renomeados para `_http_get`/`_http_post`
  (`_get` colidia com o virtual `Object._get()`).
- `invaders_game.gd` / `tetris_game.gd`: `var color :=` anotado como
  `var color: Color =` (inferência de tipo mais estrita no 4.5).

### Added — Editor de universos estilo Sims/Paralives (base + player/editor web + Godot)

Novo conceito **paralelo** ao runtime WASM: universos autorados por usuário como
**dados** (`UniverseInstance`) carregados em runtime, não crates compilados. Nada
de arcade/WASM/registry foi tocado — tudo aditivo.

- **YG-74** `yggdrasil-core/src/instance/schema.rs` — formato serde
  (`UniverseInstance`, `Layer`, `Block`, `Connection`, `ContentRef`), projeção
  2D/isométrica, `schema_version = 1`.
- **YG-75** `instance/store.rs` — persistência em disco (escrita atômica) +
  anexos content-addressed por SHA-256 com dedupe; disco é a fonte da verdade.
- **YG-76** `yggdrasil-web/src/api/instances.rs` — REST CRUD + PATCH `EditOp`
  granular (place/move/delete block, edit/add layer, add/del connection, attach),
  validação num ponto único, auth por dono (JWT `sub`).
- **YG-77** upload multipart → blob content-addressed (allowlist de MIME + cap de
  tamanho) e serve com `Cache-Control: immutable`.
- **YG-79** `instance/template.rs` — mecânica de templates (`seed` + `palette` +
  `render_hints`) + endpoints `GET /api/v1/templates[/{slug}]`; template `blank`.
- **YG-80** template `neuroanatomia` — silhueta + camada SNC (opacity 0.5 = toggle
  de transparência) + landmarks + conexões; fontes open-source documentadas em
  `docs/architecture/editor.md`.
- Template `neuroanatomia` semeia assets de fundo embutidos (silhueta + SNC,
  SVGs originais CC0) na criação — o toggle de transparência já revela/esconde o
  SNC sobre o corpo sem upload prévio.
- **YG-78/YG-81** player+editor web genérico (`static/universos/instance.html`
  + `instance.js`): render de camadas/projeção/opacity/blocos/conexões, viewers de
  anexo, e modo edição (paleta, place/move/delete, slider de opacity, conexões,
  upload) via os endpoints REST.
- **Godot** — `scripts/editor/instance_api.gd` + `scripts/editor/grid_editor.gd`
  + `scenes/editor/grid_editor.tscn`: editor de grade consumindo o mesmo contrato
  REST host-neutral.

Verificado end-to-end ao vivo: criar de template → place block → toggle de opacity
persiste → upload+serve de anexo → sobrevive a restart do servidor (reindex do
disco). `cargo fmt`/`clippy -D warnings`/`test --workspace` limpos.

---

## [1.0.1] — 2026-05-24

### Changed — Fly machines now suspend instead of stop on idle (CO-285)

Updated `fly.toml` to use `auto_stop_machines = "suspend"` and `min_machines_running = 0`.
Suspend freezes machine state rather than shutting it down — cold-wake is ~250ms instead of ~10s.
Saves ~$1-2/mo at typical low-traffic idle rates.

---

## [1.0.0] — 2026-05-20 — Universe Platform v1.0 (YG-54 epic complete)

### Theme

Marks the completion of the **YG-54 Universe Platform v1.0 epic** — eight user-stories shipped over the day:

- **YG-55** WASM runtime (wasmtime + fuel enforcement)
- **YG-56** Universe SDK (ABI v1 + build tooling)
- **YG-57** Five universos migrated to WASM (snake, tetris, invaders, pointset, poker)
- **YG-58** Universo Vim — modal editor + 10 levels
- **YG-59** Claude hint engine — host-side LLM bridge for Vim
- **YG-60** Unified `/api/v1/universos` API + WebSocket
- **YG-50** OpenAPI 3.x specification at `/openapi.json` + `/openapi.yaml`
- **YG-61** `build-universes.sh` + GitHub Actions CI/CD pipeline
- **YG-62** Telemetry — `funnel_events` + `session_records` + `/api/v1/admin/analytics`

### Acceptance criteria — verified live in prod

- ✅ Single self-contained binary with 6 WASM universos embedded
- ✅ `GET /api/v1/universos` lists all 6: snake, tetris, invaders, pointset, poker, vim (verified at `https://yggdrasil-artelonga.fly.dev/api/v1/universos`)
- ✅ Legacy `/api/v1/games/{game}/start` routes preserved (YG-60 design)
- ✅ Vim universe playable with Claude API hints (YG-58 + YG-59)
- ✅ `cargo test` + `cargo clippy -- -D warnings` clean
- ✅ `Cargo.toml` bumped to **1.0.0**

### Why

Yggdrasil graduates from per-game backend chaos to a unified WASM-embedded universe platform with: typed contracts (OpenAPI), runtime sandboxing (wasmtime + fuel), composable SDK, AI-augmented learning surface (Universo Vim + Claude hints), and observability (funnel + analytics). The next-tier feature — user-uploaded universos via the Component Model — is gated behind v2.x.

## [0.14.0] — 2026-05-20 — Telemetria e funil básico (YG-62)

### Added

- `yggdrasil-web/src/telemetria.rs` — novo módulo `TelemetriaDb`: schema idempotente (`init_telemetry_db`) com tabelas `funnel_events` (event_id, user_id, session_id, universe_id, event_type, properties, created_at) e `session_records` (session_id, universe_id, started_at, ended_at, duration_ms, final_score, abandoned). Índices em `(universe_id, created_at)` e `(user_id, created_at)`.
- Instrumentação em `universos_routes.rs`: `SESSION_CREATE` no `POST /api/v1/universos/{id}/sessoes`, `SESSION_COMPLETE` no `DELETE` e no `tick` com `session_ended=true` (inclui `final_score` e `duration_ms` corretos), `UNIVERSE_VIEW` no `GET /api/v1/universos/{id}`.
- Cleanup job (`spawn_cleanup_job`): `tokio::spawn` com `tokio::time::interval(300s)` que detecta sessões sem tick há > 30min, remove da memória e emite `SESSION_ABANDON` + marca `abandoned=1` no banco.
- `GET /api/v1/admin/analytics` — protegido por `YGGDRASIL_ADMIN_TOKEN` (Bearer header). Retorna JSON com `universos` (sessions_today, completions_today, completion_rate_pct, median_duration_ms, active_now) e `funnel_24h` (universe_views, session_creates, session_completions, conversion_view_to_create_pct, conversion_create_to_complete_pct) para as últimas 24h.
- `[dev-dependencies] tokio = { ..., features = ["test-util"] }` para suporte a `#[tokio::test(start_paused = true)]` nos testes do cleanup job.

### Changed

- `UniversosState` recebe `Arc<TelemetriaDb>` e `admin_token: Option<String>` (lido de `YGGDRASIL_ADMIN_TOKEN` no boot).
- `SessionEntry` armazena `started_at: i64` e `last_tick_at: tokio::time::Instant` para cálculo de duração e detecção de inatividade.

## [0.13.1] — 2026-05-20 — Build pipeline CI/CD (YG-61)

### Added

- `scripts/build-universes.sh` — compila os 6 universos WASM (snake, tetris, invaders, pointset, poker, vim), otimiza com `wasm-opt -O3`, valida budgets individuais (300KB/300KB/300KB/200KB/600KB/250KB) e budget total de 2MB.
- `.github/workflows/ci.yml` — GitHub Actions: `dtolnay/rust-toolchain@stable` + `wasm32-unknown-unknown`, cache de `~/.cargo/registry`, `target/`, `universes/target/` e `~/.cargo/bin/wasm-opt` por hash do `Cargo.lock`, executa `build-universes.sh` → `cargo build --release` → `cargo test` → `cargo clippy -- -D warnings`.
- `yggdrasil-web/build.rs` — referências atualizadas para `scripts/build-universes.sh`.

## [0.13.0] — 2026-05-20 — OpenAPI spec (YG-50)

### Added

- `yggdrasil-web/src/openapi.rs` — `ApiDoc` struct com `#[derive(OpenApi)]` agregando todos os 29 paths da API pública. Schema doc-types concretos para todos os tipos de estado de jogo (SnakeMapDoc, TetrisStateDoc, InvadersStateDoc, VimGameStateDoc) e respostas (SnakeStartDoc, TetrisStartDoc, InvadersStartDoc, PokerHandDoc, etc.).
- `GET /openapi.json` — especificação OpenAPI 3.x serializada como JSON; sem estado, gerada em compile-time via utoipa.
- `GET /openapi.yaml` — mesma spec em YAML (`Content-Type: application/x-yaml`).
- `utoipa = "5.5.0"` e `serde_yaml = "0.9"` adicionados como dependências de `yggdrasil-web`.
- `#[utoipa::path]` em todos os 29 handlers: `auth/magic_link.rs`, `api/me.rs`, `api/scores.rs`, `api/universes.rs`, `games/snake_routes.rs`, `games/tetris_routes.rs`, `games/invaders_routes.rs`, `games/vim_routes.rs`, `games/poker/routes.rs`, `universos_routes.rs`.
- `#[derive(ToSchema)]` em: `CodeRequest`, `VerifyRequest`, `SaldoResponse`, `ScoreRow`, `InputRequest`, `SendKeyRequest`, `SitRequest`, `ActionRequest`, `UniversoMeta`, `TickInput`, `TickBody`.
- 3 novos testes de integração em `openapi::tests`: spec retorna 200 + versão "3.x", ≥20 paths, rotas obrigatórias por prefixo, YAML com Content-Type correto. Total: 127 testes.
- `docs/architecture/api-catalog.md` atualizado: pointer para `/openapi.json`, seção de WebSocket corrigida (antes dizia "none").
- `SecurityAddon` modifier wires `bearerAuth` (HS256 JWT) como security scheme reutilizável.

## [0.12.0] — 2026-05-20 — Universe Platform v1.0 WASM epic + Unified API (YG-54 → YG-60)

### Theme

Bundles eight user-stories that constitute the WASM-embedded universe platform: the runtime (YG-55), the Rust SDK (YG-56), migration of 5 existing universos to WASM (YG-57), the Universo Vim modal editor (YG-58), the Claude hint engine (YG-59), and the unified `/api/v1/universos` session API + WebSocket (YG-60). The remaining v1.0 work (YG-50 OpenAPI, YG-61 CI/CD, YG-62 telemetria) lands separately before the v1.0.0 release commit.

### Highlights

- **WASM runtime** (YG-55): wasmtime + fuel enforcement (10M instructions/tick), per-session sandboxed `Store<HostState>`, host imports for KV/events/hints/random/now.
- **Universe SDK + 5 ported universos** (YG-56, YG-57): snake, tetris, invaders, pointset, poker compiled to WASM.
- **Universo Vim** (YG-58): modal editor in Rust, 10 progressive levels, `vim_routes.rs` HTTP surface, exposed as a path-dep crate from the root workspace.
- **Claude hint engine** (YG-59): `HintEngine` host-side bridge to Anthropic API, rate limited 5/user/hour, fallback per level.
- **Unified API** (YG-60): `GET /api/v1/universos`, session CRUD + WebSocket for real-time state streaming, `UniversoSession` trait adapters for each game, legacy `/api/v1/games/...` routes preserved.
- **YG-58 hotfix**: `universe-vim` added as path dep in `yggdrasil-web/Cargo.toml` after the cross-workspace import was missing post-merge.

### Why

Sets the foundation for v1.0.0: every universo runs in an isolated WASM sandbox addressed by one unified API. Anonymous + authenticated clients consume the same surface. The legacy per-game routes remain as-is for backwards-compatibility during the transition.

## [0.11.0] — 2026-05-20 — Claude hint engine (YG-59)

### Added

- `yggdrasil-web/src/hint_engine.rs` — novo módulo `HintEngine`: LLM bridge host-side para hints adaptativos no Universo Vim.
  - `HintContext` — struct deserializada do JSON emitido pelo WASM via `request_hint`; campos: `puzzle_id`, `puzzle_description`, `buffer_content`, `cursor`, `recent_commands[5]`, `attempt_count`, `description_lang`.
  - `HintApi` trait — abstração sobre a chamada HTTP; injetável em testes via `HintEngine::with_components(api, now_fn)`.
  - `ReqwestHintApi` — implementação real que chama `POST https://api.anthropic.com/v1/messages` com modelo `claude-sonnet-4-6`.
  - `HintEngine::from_env()` — lê `ANTHROPIC_API_KEY` do ambiente; loga `INFO` (não `WARN`) se ausente.
  - `HintEngine::request(user_id, ctx)` — async; retorna hint do Claude ou fallback estático por nível.
  - Rate limiting: máx 5 hints/usuário/hora via `DashMap<user_id, (count, window_start)>`; clock injetável para testes determinísticos.
  - Fallback PT-BR por nível (1–10) do Universo Vim + hint genérico para IDs desconhecidos.
  - Hint em PT-BR quando `description_lang == "pt"`.
  - 8 novos testes: fallback sem API key, rate limit bloqueia 6ª chamada sem chamar API (mock via `CountingApi`), reset de janela de hora, independência de rate limit entre usuários, prompt inclui todos os campos de contexto, idioma EN vs PT-BR, static hints para 10 níveis.
- `HostState::hint_result: Arc<Mutex<Option<String>>>` — campo adicionado em `wasm_runtime.rs`; inicializado em `new()`. Escrito pela task de hint assíncrona; lido pelo tick endpoint (YG-60) para retornar `{ hint: "..." }` no tick seguinte.
- `dashmap = "6"` adicionado como dependência de workspace.
- `docs/DEPLOY.md` — nova seção "Hint engine do Universo Vim" documentando `ANTHROPIC_API_KEY`, comportamento por cenário, e custo estimado ($0,002/hint, máx $0,01/hora/usuário).

## [0.10.0] — 2026-05-20 — WASM Runtime Host (YG-55)

### Added

- `yggdrasil-web/src/wasm_runtime.rs` — novo módulo `WasmRuntime` + `WasmSession`:
  - `WasmRuntime::from_embedded()` — pré-compila 6 módulos WASM ao boot (snake, tetris, invaders, pointset, poker, vim) via `include_bytes!("../embedded/<name>.wasm")`. Tempo < 500ms em hardware commodity.
  - `WasmSession` — envolve `Store<HostState>` + `Instance` com memória linear isolada por sessão. Drop libera toda a memória WASM (RAII).
  - `WasmSession::tick()` — reseta combustível para `FUEL_PER_TICK = 10_000_000` e executa a export `tick`; retorna `Err(WasmError::FuelExhausted)` ao esgotar o orçamento. Loop infinito não trava o servidor.
  - `HostState` — `user_id`, KV namespace por sessão (`HashMap<String, Vec<u8>>`), canal de hint para Claude, PRNG xorshift64.
  - Linker ABI (namespace `"env"`): `wallet_get_balance`, `kv_get`, `kv_set`, `emit_event`, `now_ms`, `random_u64`, `request_hint`.
  - `pub unsafe fn read_memory` / `write_memory` — acesso direto à memória linear com bounds check.
- `yggdrasil-web/build.rs` — gera stubs mínimos (8 bytes WASM válido) em `embedded/` se ausentes; `build-universes.sh` (YG-61) sobrescreve com binários reais.
- `yggdrasil-web/embedded/*.wasm` adicionado ao `.gitignore`; stubs gerados pelo `build.rs` em tempo de compilação.
- 6 novos testes de integração: `from_embedded_loads_all_six_modules`, `tick_with_tight_loop_returns_fuel_exhausted`, `server_continues_after_fuel_exhausted`, `sessions_have_isolated_linear_memory`, `dropping_session_releases_store`, `kv_set_and_get_roundtrip`. Total: 83 testes.
- Dependência `wasmtime = { version = "44.0.1", features = ["cranelift"] }` adicionada a `yggdrasil-web`.

## [0.9.3] — 2026-05-20 — Arquiva repositório universos (YG-53)

### Chore

- `artelonga/universe` renomeado para `artelonga/universos-archive` e arquivado no GitHub.
- Descrição do repo atualizada: `Archived — see artelonga/yggdrasil`.
- `ARCHIVED.md` adicionado ao root do repo arquivado apontando para `artelonga/yggdrasil` como único canônico.
- Todo o stack (engine, jogos, lobby, auth, sementes, universos) vive em `yggdrasil`.

## [0.8.1] — 2026-05-19 — Reconcilia drift game-core (YG-52)

### Fixed

- Reconciliados os arquivos divergidos entre `universos/core/src/` e `co/game-core/src/`: `49a25f9f/{mod,04f8996d,3549b002,8a6cead4}.rs`, `lib.rs`, `plugin.rs` são agora byte-idênticos. Suporte a `GAME_DB_PATH` restaurado em `storage::db_path()`.
- `co/game-core` promovido para 0.2.0; `universos/core/src/universo.rs` (cifrado/ilegível) substituído por implementação legível de `Universo` trait + `UniversoLocal`. `diff -rq` entre os dois `src/` retorna apenas `Only in co/game-core/src: mail.rs`.

## [Unreleased]

## [0.9.3] — 2026-05-20

### Docs (YG-46)

- Criado `docs/architecture/data-model.md` descrevendo o layout real de dois bancos (`yggdrasil.db` SQLite + `yggdrasil-sementes.db` redb), todas as tabelas com seus schemas DDL, e aposentando o mito de "um banco por jogo".

## [0.9.2] — 2026-05-19

### Refactored (YG-42)
- `YggGame` trait ganha `type State: Serialize` e `fn render(&self) -> Self::State`; `render_json() -> String` removido.
- `StartResponse` e `TickResponse` parametrizados como `StartResponse<S>` / `TickResponse<S>`.
- `map_to_value` deletado de `common.rs`; rotas de jogo single-player passam o estado tipado diretamente.
- Estruturas de render (`TetrisRender`, `InvadersRender`, `PokerRender`, `ActivePiece`, `Bullet`, `Alien`) promovidas a `pub` como associated types do trait.

## [0.9.1] — 2026-05-19

### Refactored (YG-40)
- `yggdrasil-web/src/games/poker_routes.rs` (963 LOC) dividido em `games/poker/{mod,state,routes,chip_flow,serialization,tests}.rs`.
- `yggdrasil-core/src/games/poker.rs` + siblings movidos para `games/poker/{mod,adapter,bot,game,lobby}.rs`; aliases backward-compat preservam paths externos.

## [0.9.0] — 2026-05-18

### Added (Godot games, YG-51)
- `yggdrasil-godot/` expandido de 9 para 37 arquivos — lobby 2D com 5 jogos jogáveis.
- `scripts/autoloads/`: `SignalBus`, `ApiClient`, `SaveManager`, `AudioManager`, `GameManager` — autoloads portados do universos/godot e adaptados para o yggdrasil-web.
- `ApiClient` aponta para `http://127.0.0.1:3030/api/v1` (porta padrão do yggdrasil-web); expõe `start_game(name)` e `send_game_input(name, session_id, direction)`.
- Snake, Tetris e Invaders: clientes thin server-driven — chamam `GET /api/v1/games/{game}/start` e `POST /api/v1/games/{game}/{id}/input` a cada tick; pontuação é persistida no SQLite do yggdrasil-web via ação `Quit`.
- PointSet e Poker: jogos client-side portados diretamente do universos/godot; pontuação salva localmente via `SaveManager`.
- `scripts/lobby/`: `player.gd`, `arcade_cabinet.gd`, `lobby.gd` — avatar 2D navegável com 5 armários (Tetris, Invaders, Snake, PointSet, Poker).
- `scripts/games/game_base.gd` — classe base com pause/quit/score partilhada por todos os jogos.
- `scripts/games/poker/`: `card.gd`, `deck.gd`, `poker_player.gd`, `hand_evaluator.gd`, `poker_ai.gd`, `poker_game.gd` — Texas Hold'em completo (AI estilo tight/loose/aggressive, SRS rotação, showdown).
- Cenas `.tscn`: `scenes/main.tscn`, `scenes/lobby/{lobby,player,arcade_cabinet}.tscn`, `scenes/games/{snake,tetris,invaders,pointset,poker}/*.tscn`.
- `project.godot` atualizado: cena principal → `res://scenes/main.tscn`; 5 autoloads registados; viewport 640×360 (janela 1280×720); 7 input actions (WASD+setas, Enter/E, Esc, Q); gravidade 2D = 0; filtro de textura pixel.

### Added (poker, YG-29 — persistência em SQLite)
- `yggdrasil_core::games::poker_game::PokerTableSnapshot { lobby, stacks }` — formato serde-friendly que captura o estado persistível de uma mesa: seating + chip-stacks por usuário (humano ou bot). Mãos em curso (`PokerGame`) NÃO entram no snapshot — um restart no meio de uma mão é forfeit, mas seats e chips sobrevivem. Escolha pragmática: o engine `PokerGame` do `co/game-core` não é serde-friendly, e o custo de uma mão perdida (≤ poucas dezenas de sementes) é muito menor que o custo de perder buy-ins (1k+ sementes).
- `PokerTable::to_snapshot()` / `PokerTable::from_snapshot(snap)` — serialização round-trip. `to_snapshot` chama `snapshot_stacks_from_game` antes para garantir que os chips do engine refletem em `stacks`.
- `SeatOccupant` e `PokerLobby` agora derivam `Deserialize` além de `Serialize`.
- `yggdrasil_web::games::poker_persistence` — novo módulo com:
  - `init_poker_db(path)` — abre conexão SQLite e cria tabela `poker_lobbies (id TEXT PRIMARY KEY, name TEXT, state TEXT)` se ausente. Idempotente.
  - `save_lobby(conn, snap)` — UPSERT do snapshot serializado em JSON.
  - `load_all(conn)` / `load_tables(conn)` — leitura ordenada por id; `load_tables` já materializa como `PokerTable`.
- `PokerState::with_persistence(secret, sementes, db_path)` — novo construtor. No primeiro boot, semeia 3 mesas defaults (Carvalho, Olmo, Heads-Up) e persiste. Em boots subsequentes, restaura mesas persistidas. Falhas de SQLite são logadas mas não abortam o boot — degrada para in-memory.
- `PokerState::new(secret, sementes)` permanece como alias in-memory (sem persistência) para testes legados.
- `main.rs` agora usa `PokerState::with_persistence` apontando para o mesmo `YGGDRASIL_DB` controlado pelas demais rotas (snake/tetris/invaders/scores). Sem nova env var.
- Persistência é acionada após cada `sit_with_sementes`, `stand_with_sementes` e `act` bem-sucedido. Cada save abre/fecha sua própria conexão — locking SQLite (WAL) faz o serializing.
- 8 novos testes (3 em `poker_game.rs`, 4 em `poker_persistence.rs`, 2 em `poker_routes.rs`): round-trip de snapshot, persistência através de reconexão SQLite (simula kill -9), UPSERT, primeiro boot semeia 3 mesas, restart preserva seat e stack. 183 testes no total.

### Added (Godot POC, YG-31)
- `yggdrasil-godot/` — diretório paralelo de POC Godot 4.5 que avalia scene tree + signals + lazy spawn + multiplayer-native como substrato dos universos do Yggdrasil. Trilho B do epic YG-22, abre a sequência YG-31..YG-35. Não modifica `yggdrasil-core` nem `yggdrasil-web`.
- `yggdrasil-godot/project.godot` — projeto Godot 4.5 com `config/features=("4.5", "Forward Plus")`; renderer `gl_compatibility` para web e mobile; cena principal `res://scenes/HelloUniverso.tscn`.
- `yggdrasil-godot/scenes/HelloUniverso.tscn` + `scripts/hello_universo.gd` — Node2D com `Label "Olá universo"` e script que imprime `hello from server` ou `hello from client` conforme `multiplayer.is_server()`. Sanity-check da toolchain; sem lógica de jogo.
- `yggdrasil-godot/export_presets.cfg` — dois export presets:
  - **Web** (HTML5/wasm) — `variant/thread_support=true`, compressão de textura mobile+desktop, target `out/web/index.html`.
  - **Linux/X11** — `dedicated_server=true`, `custom_features="headless"`, target `out/headless/yggdrasil-godot`.
- `yggdrasil-godot/scripts/build.sh` — detecta `godot` ou `godot4` no `$PATH` e falha cedo com mensagem PT-BR clara se ausente; suporta targets `web`, `headless` ou `all` (default); usa `--headless --path` para não exigir GPU.
- `yggdrasil-godot/Dockerfile` — multi-stage: stage 1 (`debian:trixie-slim`) baixa Godot 4.5 + export templates, roda `build.sh`; stage 2 (`debian:trixie-slim`) copia headless binary + assets web, roda como user `godot`, expõe porta `3031` (separada da `:3030` do Rust).
- `yggdrasil-godot/.gitignore` — `.godot/`, `*.tmp.tscn`, `out/`, `.import/`, `*.bak`, OS cruft.
- `yggdrasil-godot/icon.svg` — ícone placeholder do projeto (gold + green sobre fundo `#0d0d12`, paleta do lobby).
- `yggdrasil-godot/README.md` — pré-requisitos (Godot 4.5 + export templates), como abrir no editor, buildar os dois targets, rodar headless localmente, servir build web com COOP/COEP, e build Docker. Documenta os 4 pilares avaliados (scene tree, signals, lazy spawn, multiplayer nativo) e os não-objetivos desta tarefa.

## [0.8.0] — 2026-05-13

Pôquer multiplayer ponta-a-ponta + universos como grafo + SSO via CO. Fecha o epic YG-22.

### Changed (auth, email-signup via CO)
- Yggdrasil `/login` email-código agora roteia para os endpoints `/api/v1/auth/onboard-with-email[/verify]` do CO via CORS (origem `yggdrasil-artelonga.fly.dev` autorizada). Após `verify`, o cliente navega para `https://co.artelonga.com.br/auth/co-handover?return_to=<ygg>/auth/co-handover-receive` — CO assina co_token ES256 e redireciona de volta; receiver existente valida via JWKS e mintar JWT local.
- Mesmo padrão de quilomboaraucaria: usuário ganha uma conta CO ao se cadastrar via email no Yggdrasil. Identidade unificada entre todas as propriedades artelonga.
- Yggdrasil não precisa mais de SMTP — CO entrega os emails. As rotas locais `POST /api/v1/auth/code` e `POST /api/v1/auth/verify` permanecem (dead code, podem ser removidas em refactor futuro) mas frontend não as usa mais.

### Added (universes, YG-37 — variantes parametrizam engines)
- `yggdrasil_core::games::snake::SnakeOptions { walls }` + `YggSnake::with_options` + `YggSnake::walls_pattern` — variante `snake/walls` adiciona paredes internas em 3 colunas determinísticas; colisão e renderização honram.
- `yggdrasil_core::games::tetris::TetrisOptions { sprint_lines }` + `YggTetris::with_options` — variante `tetris/sprint-40` encerra a mão ao limpar N linhas (`sprint_limit` checado em `clear_lines`).
- `yggdrasil_core::games::invaders::InvadersOptions { lives }` + `YggInvaders::with_options` — variante `invaders/swarm` inicia com `lives=1`.
- `yggdrasil_core::games::poker_lobby::PokerLobby::with_max_seats(id, name, max_seats)` — mesa de tamanho customizável; `max_seats` é serializado no JSON. Variante `poker/heads-up` cria mesa de 2 assentos; mínimo de 2 enforced.
- `PokerState::new` agora provisiona 3 mesas: "Mesa Carvalho" + "Mesa Olmo" (cash game, 6 seats) + "Heads-Up Carvalho" (heads-up, 2 seats).
- `yggdrasil_web::games::common::VariantQuery { variant: Option<String> }` — extractor de `?variant=<slug>` compartilhado entre rotas.
- Snake/Tetris/Invaders rotas `start` aceitam `?variant=<slug>` e mapeiam para `SnakeOptions`/`TetrisOptions`/`InvadersOptions`; slug desconhecido → comportamento root (compat retroativa).
- 8 novos testes cobrem cada variante + regressão de cada root. 175 testes no total.

### Added (universes node-graph)
- `yggdrasil_core::universes::{UniverseNode, UniverseRegistry, UniverseKind, ApiContract, UniverseGraph, UniverseEdge}` — modelo recursivo de universos como grafo. Cada nó é Root / Variant / Composition. Variantes não estendem o root — instanciam o engine raiz com overrides de parâmetros (composição sobre herança).
- `default_registry()` semeia 4 roots (snake/tetris/invaders/poker) + 8 variantes de exemplo: `snake/classic`, `snake/walls`, `tetris/classic`, `tetris/sprint-40`, `invaders/classic`, `invaders/swarm`, `poker/cash-game`, `poker/heads-up`. Cada variante leva parâmetros documentados (ex: `lines_to_clear: 40`, `lives: 1`, `max_seats: 2`).
- `yggdrasil-web/src/api/universes.rs` — endpoints públicos (sem auth):
  - `GET /api/v1/universes` — todos os nós em ordem de slug.
  - `GET /api/v1/universes/{*slug}` — um nó individual (slug pode conter `/`).
  - `GET /api/v1/universes/graph` — `{nodes, edges}` para visualizadores.
- Cada nó expõe `ApiContract { start, input, page }` — variantes apontam para a rota da raiz com query string `?variant=<slug>` (parameterização real fica para o próximo task).
- `docs/ARQUITETURA-UNIVERSOS.md` — nova seção "Grafo de universos" com modelo, contrato HTTP, mapeamento futuro para Godot scenes (YG-31..YG-35), exemplo da árvore atual, e regra de quando promover uma variante a root.
- 13 testes unitários de registry + 5 de routes; 167 testes no total.

### Changed (auth, signup via Google)
- Botão de login agora aponta para `https://co.artelonga.com.br/api/v1/auth/google/start` (Google OAuth start), em vez de `/auth/co-handover` que exigia auth prévia no CO. Espelha o padrão de `quilomboaraucaria/web/src/routes/cadastro/+page.svelte`: usuário entra direto via Google, conta CO é criada/atualizada automaticamente, retorna ao Yggdrasil com sessão local.
- Texto do botão: "Entrar com CO" → "Continuar com Google" (com logo Google SVG, fundo branco — mesmo estilo do quilombo).

### Added (auth, CO handover SSO)
- `yggdrasil_web::auth_co::{JwksCache, CoHandoverClaims, verify_co_token, co_login_url}` — módulo de cross-apex SSO via CO. Fetch + cache (TTL 1h) de `https://co.artelonga.com.br/.well-known/jwks.json`; verificação de JWT ES256 com lookup por `kid`.
- `GET /auth/co-login?next=<path>` — redirect 302 para `/api/v1/auth/google/start` do CO; constrói `return_to` a partir do Host header.
- `GET /auth/co-handover-receive?co_token=<jwt>&next=<path>` — recebe JWT ES256 emitido pelo CO, valida via JWKS, e mintar JWT local HS256 com o mesmo `sub` e `email`. Responde com HTML que armazena o JWT em `localStorage.yggdrasil-jwt` e navega para `next` (ou `/lobby`).
- `CO_BASE_URL` env var (default `https://co.artelonga.com.br`) — permite apontar UAT/staging/dev local em testes.
- Login UI ganha botão "Entrar com CO" como CTA principal; email-code permanece como fallback abaixo da divisória "— ou —".
- Sem segredo compartilhado entre CO e Yggdrasil; rotação de chave em CO requer apenas que o cache de JWKS expire (≤ 1h) ou seja invalidado por falta do `kid` no cache.
- Dependências: `reqwest = "0.12"` com features `rustls-tls-native-roots,json` (reaproveita rustls já em uso por lettre); `tokio-util = "0.7"`.

### Added (universos rename)
- Rotas públicas renomeadas: `/games/{slug}` → `/universos/{slug}`. Cada jogo é um universo agora também na URL.
- 301 `Redirect::permanent` de `/games/{snake,tetris,invaders,poker}` → `/universos/{slug}` preservam links externos.
- Static dir `yggdrasil-web/static/games/` movido para `static/universos/`.
- Lobby copy: "Cada jogo é um universo. Caminhe até um portal e entre." + nota "Em breve: criar o seu universo."
- API paths (`/api/v1/games/*`, `/api/v1/poker/*`) **não** mudam — superfície de programador, não usuário.

### Added (lobby UI)
- Auth area no canto superior direito do `/lobby`: botão "Entrar" quando anônimo; email + "Sair" quando autenticado (decodifica JWT do `localStorage`).
- Sidebar com "HIGH SCORES" (top 3 por universo) e "ATIVIDADE RECENTE" (últimas 10 partidas) — alimentadas pelas tabelas que snake/tetris/invaders populam desde YG-7/8.
- `yggdrasil-web/src/api/scores.rs`: `GET /api/v1/scores/top?limit=N` e `GET /api/v1/scores/recent` — anônimos, agregam a tabela `scores`.
- Seção "REVIEWS" placeholder — sistema de ratings por universo virá depois.

### Fixed (sementes)
- `Sementes::saldo/debitar/creditar` agora usam `storage.get_wallet_for_user` + `save_wallet_for_user` — antes encaminhavam para `WalletManager` que ignora `user_id` e opera em uma única wallet global. Multiplayer (YG-27) exigia carteiras por usuário.

### Added (poker, YG-27)
- `yggdrasil_core::games::poker_game::{PokerSitError, PokerStandError, BUY_IN_SEMENTES=1_000}` — erros tipados envolvendo `LobbyError` + `SementesError`; buy-in fixo por sentar.
- `PokerTable::sit_with_sementes(seat, user_id, sementes)` — debita buy-in, ocupa assento, inicializa chip stack; refund automático se o lobby recusar.
- `PokerTable::stand_with_sementes(user_id, sementes)` — fold automático mid-hand, credita stack remanescente de volta na carteira.
- `PokerTable::stacks: HashMap<String, u32>` — chip stacks persistidos entre mãos (incluindo bot, inicializado com buy-in quando entra).
- `POST /api/v1/poker/lobbies/{id}/sit` retorna 402 `Payment Required` quando saldo < buy-in com mensagem PT-BR.
- Frontend `poker.{html,js}`: header com saldo atual do usuário (chama `/api/v1/me/sementes`), atualizado após sit/stand/showdown; erro 402 mostrado como "Saldo insuficiente — buy-in é 1000 sementes".

### Added (poker, YG-26)
- `yggdrasil_core::games::poker_bot` — bot AI escolhe ação aleatória legal com pesos fold=15% / check=35% / call=35% / raise=15%; raise = `big_blind * 2`.
- `auto_step_bots(table)` — chamado nas rotas após `get_hand` e `post_action`; avança a mão enquanto `current_actor == BOT_USER_ID`, com limite de 32 iterações como guarda contra deadlock.
- Humano solo agora joga mão completa contra o bot (regressão coberta por `humano_vs_bot_completa_mao_sem_travar_via_http`).

### Added (poker, YG-25)
- `yggdrasil_core::games::poker_game::{PokerTable, PokerTableError, HandState, CardView, PublicPlayer}` — estado de partida multiplayer: deal → pre-flop → flop → turn → river → showdown.
- `PokerTable::start_hand()` — inicia mão com ≥ 2 ocupantes (humano ou bot); conecta `PokerLobby` com `game_core::PokerGame`.
- `PokerTable::act(user_id, action)` — valida vez do jogador e aplica ação; erros PT-BR: `NaoEhSuaVez`, `AcaoInvalida`, `MesaSemJogadores`, `RoundEncerrado`.
- `PokerTable::hand_state()` — estado público (community cards reveladas por rodada, pot, current_actor, vencedor via `EvaluatedHand`).
- `PokerTable::hole_cards_for(user_id)` — cartas privadas exclusivas do usuário autenticado.
- `GET /api/v1/poker/lobbies/{id}/hand` — estado público da mão; auto-inicia partida quando ≥ 2 ocupantes e nenhuma mão em curso.
- `GET /api/v1/poker/lobbies/{id}/hole-cards` — cartas hole do usuário autenticado (auth-gated).
- `POST /api/v1/poker/lobbies/{id}/action` — aplica ação `{action: "call"|"raise"|"fold"|"check", amount?: u32}`; retorna 409 fora-da-vez, 422 ação inválida.
- `poker.js` — renderiza community cards, hole cards, pot, aposta atual, botões de ação (só na vez do usuário), banner "Sua vez!", animação de carta virada, lista de jogadores.
- Removido badge "EM CONSTRUÇÃO" e seção placeholder de `poker.html`.

### Added (auth, YG-24)
- `yggdrasil-web/src/mail.rs` — `SmtpMailProvider` usando `lettre` (STARTTLS, rustls); configurável via `YGGDRASIL_SMTP_HOST/PORT/USER/PASSWORD/FROM`.
- `build_mail_provider()` — seleciona `SmtpMailProvider` quando `YGGDRASIL_SMTP_HOST` está definido e não-vazio; caso contrário usa `LogMailProvider` com log `WARN smtp not configured — emails go to stdout`.
- `docs/DEPLOY.md` — guia de configuração SMTP para Mailtrap, SendGrid e AWS SES; instruções para `flyctl secrets set` e como rodar o teste de integração.
- `fly.toml` — variáveis SMTP adicionadas como placeholders (valores reais via `flyctl secrets set`).

### Added (deploy)
- Deploy Fly.io: `Dockerfile`, `fly.toml`, app `yggdrasil-artelonga` em `gru`, volume `yggdrasil_data` montado em `/data`, secret `YGGDRASIL_JWT_SECRET`, certificado para `yggdrasil.artelonga.com.br` (pendente DNS).

### Added (poker, YG-23)
- `yggdrasil_core::games::poker_lobby::{PokerLobby, SeatOccupant, LobbyError}` — modelo de mesa multiplayer com 6 assentos, regras de seating (sit/stand), e regra de presença de bot (0/1/2+ humanos → 0/1/0 bots).
- `yggdrasil-web/src/games/poker_routes.rs` — endpoints auth-gated `GET/POST /api/v1/poker/lobbies[/...]` (list, get, sit, stand). Provisiona "Mesa Carvalho" e "Mesa Olmo" no boot.
- `yggdrasil-web/static/games/poker.{html,js}` — frontend com CTA de login, selector de mesa, tabela de assentos clicáveis, CTA "Convide um amigo" quando há bot na mesa, polling 2s.

### Added (auth UI)
- `GET /login` + `static/login.{html,js}` — fluxo email → código (6 dígitos) → JWT armazenado em `localStorage.yggdrasil-jwt`. Aceita `?next=/path` para retornar à página de origem após login.

### Fixed (lobby)
- BFS pathfinding limitado a 200 nós resultava em "Sem caminho" para portais distantes em mapa 40×20. Limite agora é `width * height` (cobertura exaustiva).
- Portal de pôquer retornava 404 (`/games/poker`) — rota registrada e página placeholder substituída pelo lobby multiplayer real.

### Architecture
- `docs/ARQUITETURA-UNIVERSOS.md` — documenta o padrão "cada jogo é um universo": fronteiras de módulo, convenção de commits com escopo de universo (`feat(poker)`, `fix(snake)`), regra de SemVer no workspace e plano de extração futura em crates independentes.

### Roadmap
- Epic YG-22 (Pôquer Multiplayer) aberto com sub-tarefas YG-23..YG-30 cobrindo seating, mail provider, gameplay, bot AI, sementes, WebSocket, persistência, e release v0.8.0.

## [0.7.0] — 2026-05-09

### Added
- `yggdrasil_core::sementes::SaldoInfo` — struct público com `saldo: u64` e `atualizado_em: DateTime<Utc>` (YG-12).
- `Sementes::saldo_info(user_id)` — retorna saldo e timestamp de última atualização por usuário via `Storage::get_wallet_for_user`; usuário sem carteira retorna saldo zero com timestamp atual (YG-12).
- `auth::verify_jwt(token, secret)` — helper público para validação de JWT sem efeito colateral (YG-12).
- `yggdrasil-web/src/api/me.rs` — módulo de rotas `GET /api/v1/me/sementes` com `MeState` (jwt_secret + sementes) (YG-12).
- `GET /api/v1/me/sementes` — retorna `{"saldo": u64, "moeda": "sementes", "atualizado_em": "ISO 8601"}` para JWT válido; 401 `{"erro":"nao_autenticado"}` sem ou com JWT inválido (YG-12).
- Storage de sementes configurável via `YGGDRASIL_SEMENTES_DB` (padrão: `yggdrasil-sementes.db`) (YG-12).

## [0.6.0] — 2026-05-09

### Added
- `yggdrasil-web/src/auth.rs` — módulo de autenticação pública com `Claims` (JWT HS256), `UserId` (extractor Axum), `AuthState`, `require_auth` (middleware Bearer JWT) e helpers `sign_jwt`/`generate_code` (YG-11).
- `POST /api/v1/auth/code` — solicita código de verificação por email; rate limit de 3 pedidos por 15 minutos; sempre retorna 200 para evitar enumeração de emails (YG-11).
- `POST /api/v1/auth/verify` — valida código de 6 dígitos e retorna JWT com TTL de 7 dias; erros tipados: código incorreto (422), esgotado/expirado (410) (YG-11).
- JWT secret lido de `YGGDRASIL_JWT_SECRET`; servidor falha no boot com mensagem clara se a variável estiver ausente (YG-11).
- `game_core::mail::LogMailProvider` usado como provider padrão em dev — imprime o código no stdout em vez de enviar email real (YG-11).
- Todas mensagens de erro da camada de auth em PT-BR (YG-11).

## [0.5.0] — 2026-05-09

### Added
- `yggdrasil_core::sementes::Sementes` — fachada de domínio sobre `WalletManager` usando terminologia Yggdrasil: `saldo`, `creditar`, `debitar`; erro tipado `SementesError::SaldoInsuficiente` com mensagem PT-BR (YG-10).
- `yggdrasil_core::games::YggPoker` — adapter sobre `game_core::PokerGame` com buy-in e cash-out em sementes via `WalletManager`; carteira inicializada com 10.000 sementes na primeira partida; recusa entrada com saldo zero ("Sem sementes para apostar") (YG-9).
- `yggdrasil_core::games::poker::INITIAL_SEMENTES` — constante pública `10_000` alinhada com `INITIAL_BALANCE` em `co-web` (YG-9).
- `yggdrasil_core::games::YggInvaders` — adapter sobre `game_core::InvadersGame` com grade 4×10 de aliens, movimento lateral e descida, tiro de aliens (xorshift64 determinístico), colisão de balas, 3 vidas e pontuação escalonada por linha (YG-8).
- `GET /api/v1/games/invaders/start` — cria sessão de Space Invaders e retorna `{id, state, score}` (YG-8).
- `POST /api/v1/games/invaders/{id}/input` — avança um tick com `direction` (Left/Right/Shoot/Tick/Quit); retorna `{action, state, score}` (YG-8).
- Score Invaders persistido em SQLite (`scores` com chave `user_id, game, score, ts`) ao fim de cada partida (YG-8).
- `GET /games/invaders` — serve `invaders.html`; `GameAction::Quit` redireciona cliente para `/lobby` (YG-8).
- `yggdrasil-web/static/games/invaders.js` — cliente canvas server-driven; loop via `requestAnimationFrame` encoda velocidade do jogador no input enviado ao servidor (YG-8).
- `yggdrasil_core::games::YggTetris` — adapter sobre `game_core::TetrisGame` com lógica completa: spawning de peças (xorshift64 para PRNG determinístico), gravidade, colisão, rotação CW, hard drop, limpeza de linhas e pontuação escalonada por nível (YG-7).
- `GET /api/v1/games/tetris/start` — cria sessão de Tetris e retorna `{id, state, score}` (YG-7).
- `POST /api/v1/games/tetris/{id}/input` — avança um tick com `direction` (Left/Right/Down/Rotate/HardDrop/Drop/Quit); retorna `{action, state, score}` (YG-7).
- Score Tetris persistido em SQLite (`scores` com chave `user_id, game, score, ts`) ao fim de cada partida (YG-7).
- `GET /games/tetris` — serve `tetris.html`; `GameAction::Quit` redireciona cliente para `/lobby` (YG-7).
- `yggdrasil-web/static/games/tetris.js` — cliente canvas server-driven com paleta de 7 cores por tipo de peça, grid decorativo e overlay de fim de jogo em PT-BR (YG-7).
- `yggdrasil-web/src/games/common.rs` — helpers comuns extraídos de snake_routes: `init_db`, `save_score`, `save_score_locked`, `InputRequest`, `map_to_value`, `StartResponse`, `TickResponse`; snake_routes refatorado para reutilizar esses helpers (YG-7).



### Added
- `yggdrasil_core::games::YggGame` — trait Yggdrasil-side com `tick`, `render_json`, `score`, `is_over` (YG-6).
- `yggdrasil_core::games::YggSnake` — adapter sobre `game_core::SnakeGame` com lógica real de cobra: movimento, colisão de parede e auto-colisão, comida, pontuação (YG-6).
- `GET /api/v1/games/snake/start` — cria sessão de Snake e retorna `{id, state, score}` (YG-6).
- `POST /api/v1/games/snake/{id}/input` — avança um tick com `direction`; retorna `{action, state, score}` (YG-6).
- Score persistido em SQLite (`scores` com chave `user_id, game, score, ts`) ao fim de cada partida (YG-6).
- `GET /games/snake` — serve `snake.html`; `GameAction::Quit` redireciona cliente para `/lobby` (YG-6).
- `yggdrasil-web/static/games/snake.js` — cliente canvas tick-based com mesma paleta de cores do lobby (`#0d0d12` fundo, `#1a1a2e` parede, `#d4af37` comida, `#e0505f` cabeça, `#34d399` corpo) (YG-6).

## [0.4.0] — 2026-05-09

### Added
- `canvas.onclick` em `static/lobby.js` — calcula `(tileX, tileY)` a partir das coordenadas do click ajustadas por escala CSS (YG-5).
- BFS no grid (ignorando paredes), limite 200 nós, retorna caminho ou `null` se inacessível (YG-5).
- Animação passo-a-passo (50 ms/tile) percorrendo o caminho BFS antes de parar no destino (YG-5).
- Auto-entrada em portal: se o tile destino for `Portal`, `POST /api/v1/lobby/enter` é disparado ao fim da animação (YG-5).
- Mensagem `"Sem caminho"` no rodapé (`#rodape`, `aria-live="polite"`) quando o tile clicado é parede ou inacessível (YG-5).
- `aria-label` dinâmico no canvas anuncia `"movendo para portal <slug>"` ao clicar em portal (YG-5).
- `cursor: pointer` no canvas; input de teclado bloqueado durante animação de mouse (YG-5).

## [0.3.0] — 2026-05-09

### Added
- `POST /api/v1/lobby/enter` em `yggdrasil-web/src/lobby_routes.rs` — recebe `{"x", "y"}`, devolve `{"slug"}` se houver portal, 404 caso contrário (YG-4).
- `static/lobby.js` mantém estado `{playerX, playerY}`; setas/WASD movem o avatar 1 tile bloqueando paredes; Enter sobre portal chama o backend e redireciona para `/games/<slug>` (YG-4).
- Avatar renderizado como `@` dourado (`#d4af37`) sobre o tile atual (YG-4).
- Enumeração `Direction` espelhando `game-core Direction { Up, Down, Left, Right }` em JS (YG-4).

## [0.2.0] — 2026-05-09

### Added
- `GET /api/v1/lobby` em `yggdrasil-web/src/lobby_routes.rs` — retorna JSON do Universe do lobby (YG-3).
- `yggdrasil-web/static/lobby.html` — página do lobby que carrega `lobby.js`.
- `yggdrasil-web/static/lobby.js` — renderiza grid 40x20 (16px/tile) em `<canvas>` com cores: parede `#1a1a2e`, portal `#d4af37`, vazio `#0d0d12`; legenda PT-BR dos 4 jogos.
- Rota `GET /lobby` no servidor serve o `lobby.html`; `GET /` redireciona para `/lobby`.
- `static/index.html` com redirect via `<meta http-equiv="refresh">` e `window.location.replace`.

## [0.1.1] — 2026-05-09

### Added
- `docs/REWARDS.md` — sistema de 6 tiers em PT-BR (Semente → Yggdrasil) para a campanha de financiamento, com add-ons, custo de entrega e estrutura i18n prevista (YG-21).

## [0.1.0] — 2026-05-09

### Added
- `yggdrasil_core::lobby` — Universe 40x20 com 4 portais (snake/tetris/invaders/poker) em grid 2x2 (YG-2).
- Constantes públicas `lobby::slug::*` e `lobby::pos::*` para uso por adaptadores e testes.
- 8 testes unitários cobrindo dimensões, objetivo PT-BR, posição de cada portal, ausência de pointset, e transição via `Session::teleport_to`.

### Notes
- Pointset removido conforme decisão de produto (ver `YG-2.md`).
- Adaptadores que conectam cada portal aos jogos reais entram em YG-6..YG-9.

## [0.0.1] — 2026-05-09

### Added
- Bootstrap do workspace Rust (`yggdrasil-core`, `yggdrasil-web`).
- Importação do engine `co/game-core` via path dep.
- Estrutura `work/yggdrasil/` compatível com `co-auto`.
- Lista inicial de tarefas YG-1 .. YG-20 mapeando o caminho até `v1.0.0`.
- Documentos de visão (`docs/YGGDRASIL.docx`) e UX (`docs/Yggdrasil — Experiência do Usuário (UX).docx`) movidos para `docs/`.
- Registro de universo (`co-universes.yaml`) para inscrição via `co`.

### Notes
- `pointset` foi excluído do conjunto inicial de jogos (decisão de produto).
- Jogos planejados para o lobby v0.5.0: Snake, Tetris, Space Invaders, Poker.
