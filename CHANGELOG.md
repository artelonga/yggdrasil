# Changelog

Todas as mudanças relevantes ao projeto Yggdrasil. Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versionamento: [SemVer](https://semver.org/lang/pt-BR/).

## [Unreleased]

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
