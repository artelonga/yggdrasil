# Changelog

Todas as mudanças relevantes ao projeto Yggdrasil. Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versionamento: [SemVer](https://semver.org/lang/pt-BR/).

## [Unreleased]

### Fixed (usernames in scores)
- **Novos scores ainda apareciam como `user_id`**: o user_profiles upsert só rodava no `/auth/co-handover-receive`, então usuários com JWT antigo (anterior ao deploy) nunca tinham perfil criado. Agora cada `send_input` de snake/tetris/invaders faz lazy-upsert via `common::user_info_from_jwt` + `common::lazy_upsert_profile` — qualquer ação autenticada cria/atualiza o perfil.
- **Poker seats mostravam user_id opaco**: `PublicPlayer` ganha campo `username` resolvido em `enrich_usernames`. Lobby endpoints retornam `usernames` map (user_id → username) para popular os assentos no selector.

### Added (perfis públicos)
- `yggdrasil_web::api::profiles` — novo módulo com `GET /api/v1/users/{username}` retornando perfil público: high scores (top por jogo) + mãos favoritas. Email não é exposto. 2 testes (perfil existente, 404 para usuário inexistente).
- `/perfil/{username}` — página HTML renderizando o perfil. Linkada a partir do lobby (high scores e atividade recente — clique no nome para abrir o perfil).
- Resolução `username → user_id` via index existente em `user_profiles.username`.

### Added (sobre)
- `/sobre` — página pública com README do projeto: visão geral, universos disponíveis, arquitetura (grafo + 3 camadas), sementes, identidade unificada via CO, API pública, link para repositório. Em PT-BR, sem framework.
- Mapa do site em `/sobre`: tabela com todas as URLs públicas agrupadas (páginas, universos, perfil/histórico, API de dados, API autenticada, jogos, auth, operacional). Substitui a lista resumida de API endpoints. Não há link no header do lobby — descoberta acontece via `/sobre`.

### Added (leaderboard usernames)
- `yggdrasil_web::api::user_profiles` — schema SQLite `user_profiles(user_id, email, username, updated_at)` populado automaticamente em cada `/auth/co-handover-receive`. Username é slug do email (`yuri@artelonga.com.br` → `yuri`).
- Endpoints de scores (`/api/v1/scores/top` + `/api/v1/scores/recent`) agora retornam `username` em cada row via `LEFT JOIN user_profiles` + `COALESCE(username, user_id)`. Frontend `lobby.js` renderiza `s.username || s.user_id` no painel de high scores e atividade recente.
- `GET /api/v1/me` retorna `{user_id, email, username}` (era apenas user_id + email).
- Cobertura: 3 testes unitários do módulo `user_profiles`.

### Added (poker, favoritos)
- `yggdrasil_web::games::poker_favorites` — módulo novo com schema SQLite (`poker_recent_hands` TTL 1h + `poker_favorite_hands` permanente), funções `save_recent`, `latest_for_table`, `favorite`, `list_favorites`. 4 testes cobrem round-trip, idempotência, isolamento por usuário, TTL.
- `PokerTable.current_hand_id` — ID único por mão (`{table_id}-{millis}`), regenerado em `start_hand`.
- `capture_hand_snapshot()` em `poker_routes` — chamado após cada `act` que termina mão. Captura community cards, pot, players (com hole cards revelados), winner_message no `poker_recent_hands`.
- `POST /api/v1/me/favorites/hands/{table_id}` — marca a última mão dessa mesa como favorita do usuário autenticado.
- `GET /api/v1/me/favorites/hands` — lista mãos favoritadas (até 50, ordem decrescente).
- `GET /favoritos` — página de visualização: lista mãos salvas com community cards + jogadores + hole cards revelados. Acessível para usuário autenticado.
- Botão "★ Salvar esta mão" no winner banner do `/universos/poker` após cada showdown.

### Fixed (lobby selector)
- **Contagem de assentos não atualizava** quando outros browsers sentavam/levantavam: lista de mesas só era recarregada via `loadLobbies` na primeira visita. Agora `startListPolling` (4s) roda enquanto na vista de seleção; `stopListPolling` quando o usuário entra numa mesa.
- **Bug do `6 -` hardcoded**: cálculo de vagas usava `6 - humans - bots`, errado para heads-up (mesa de 2 seats mostrava número negativo). Agora usa `l.seats.length`.
- Display "(N max)" ajuda a distinguir mesas Carvalho/Olmo (6) de Heads-Up (2).

### Fixed (poker)
- **Showdown não saía do lugar**: `get_hand` só chamava `start_hand` quando `table.game.is_none()`. Após o fim de uma mão, `game.game_over=true` mantinha a mesma mão para sempre. Agora `PokerTable` registra `hand_ended_at`; passados `HAND_END_RESTART_DELAY_SECS` (5s), o route handler substitui o game por uma nova mão. Usuários veem o vencedor por 5s e a próxima mão começa automaticamente.
- **Cartas piscando** durante polling: o `renderGame` zerava `innerHTML` das `community_cards` / `hole_cards` a cada poll (2s), criando flicker visual. Agora caches `lastCommunityKey` e `lastHoleKey` evitam rebuild quando o conteúdo não mudou.

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
