# Pôquer Multiplayer no Yggdrasil

> Como uma mesa de pôquer chega na tela do jogador: rastreamento de estado,
> processamento em tempo real, e mensageria autenticada entre o cliente, o
> servidor Yggdrasil, e o servidor de identidade CO.

Este documento é a **referência arquitetural** do universo `poker`. Cada
seção liga para os métodos que implementam o conceito — abra o arquivo no
editor e o link `path/arquivo.rs#L<n>` cai exatamente na função.

Para o contrato HTTP cru (request/response shapes, status codes), veja
[`docs/API.md`](API.md). Para a arquitetura geral de "cada jogo é um
universo", veja [`docs/ARQUITETURA-UNIVERSOS.md`](ARQUITETURA-UNIVERSOS.md).

---

## Onde está a lógica do jogo?

**Resposta curta**: em **dois lugares**, em repos diferentes.

```
┌─────────────────────────────────────────────────────────────────────┐
│ REPO `co` (../co/game-core/)                                        │
│                                                                     │
│   game_core::PokerGame   ← a engine. Owns:                          │
│     • deck shuffling                                                │
│     • dealing hole cards + community cards                          │
│     • BettingRound state machine (PreFlop→Flop→Turn→River→Showdown) │
│     • hand evaluator (pair, two pair, …, royal flush)               │
│     • pot resolution                                                │
│                                                                     │
│   game_core::games::poker::{PokerAction, BettingRound, GameConfig,  │
│                             PlayerStatus, deck::{Card, Suit, Rank}} │
│                                                                     │
│   Yggdrasil NÃO modifica nem lê o source deste crate. Importa via   │
│   `path = "../co/game-core"` (a virar git rev pin em YG-17).        │
└─────────────────────────────────────────────────────────────────────┘
                                  │ usado por
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│ REPO `yggdrasil` (este repo)                                        │
│                                                                     │
│   yggdrasil-core/src/games/poker_engine.rs   ◄── ÚNICA fronteira    │
│     • pub use PokerGame, PokerAction, BettingRound, GameConfig,     │
│       PlayerStatus, Player, SelectedAction, Card, Suit,             │
│       create_poker_universe                                         │
│     • Todo módulo do subuniverso poker importa DAQUI, nunca         │
│       direto de game_core::*. Mudança no engine só toca este arq.   │
│                                                                     │
│   yggdrasil-core/src/games/poker_lobby.rs                           │
│     • SeatOccupant { Empty, Human{user_id,sat_at}, Bot }            │
│     • PokerLobby::sit / stand / human_count / bot_count             │
│     • Regra de bot-fill (0 humanos → 0 bots; 1 humano → 1 bot;      │
│       2+ humanos → 0 bots)                                          │
│                                                                     │
│   yggdrasil-core/src/games/poker_game.rs                            │
│     • PokerTable — composição lobby × engine                        │
│     • sit_with_sementes — debita buy-in da carteira                 │
│     • stand_with_sementes — credita stack remanescente              │
│     • start_hand — chama PokerGame::deal, regenera current_hand_id  │
│     • act — valida current_actor, chama PokerGame::apply_action     │
│     • hand_state — projeta HandState público (sem hole cards)       │
│     • hole_cards_for — busca cartas do user_id autenticado          │
│     • stacks: HashMap<user_id, u32> — chips entre mãos              │
│                                                                     │
│   yggdrasil-core/src/games/poker_bot.rs                             │
│     • auto_step_bots — quando current_actor é bot, escolhe ação    │
│     • pick_action — política aleatória ponderada (fold/check/call)  │
│                                                                     │
│   yggdrasil-web/src/games/poker_routes.rs                           │
│     • Handlers HTTP que orquestram tudo acima                       │
│                                                                     │
│   yggdrasil-web/src/games/poker_persistence.rs                      │
│     • Snapshot PokerTable → SQLite (apenas lobby + stacks)          │
│                                                                     │
│   yggdrasil-web/src/games/poker_favorites.rs                        │
│     • Snapshot pós-showdown (community + hole + winner)             │
└─────────────────────────────────────────────────────────────────────┘
```

### Mapa concreto: "se eu quero mexer em X, abro qual arquivo?"

| Quero mexer em… | Arquivo | Posso? |
|---|---|---|
| Como cartas são embaralhadas | `co/game-core` (interno) | ❌ Não. Engine fechado. |
| Ranking de mão (full house > flush) | `co/game-core` (interno) | ❌ Não. Engine fechado. |
| Quando preflop vira flop | `co/game-core` (interno) | ❌ Não. Engine fechado. |
| Quem é dealer / blinds | `co/game-core` (interno) | ❌ Não. Engine fechado. |
| Regra de bot fill (1 humano → 1 bot) | [`poker_lobby.rs`](../yggdrasil-core/src/games/poker_lobby.rs) | ✅ Sim |
| Buy-in (1000 sementes) | [`poker_game.rs`](../yggdrasil-core/src/games/poker_game.rs#L17) — `BUY_IN_SEMENTES` | ✅ Sim |
| Auto-restart delay (5s) | [`poker_game.rs`](../yggdrasil-core/src/games/poker_game.rs#L101) — `HAND_END_RESTART_DELAY_SECS` | ✅ Sim |
| Política do bot (fold/check/call/raise %) | [`poker_bot.rs`](../yggdrasil-core/src/games/poker_bot.rs) | ✅ Sim |
| Status HTTP de "não é sua vez" | [`poker_routes.rs`](../yggdrasil-web/src/games/poker_routes.rs#L230) — `table_error` | ✅ Sim |
| Frequência de polling (2s) | [`static/universos/poker/state.js`](../yggdrasil-web/static/universos/poker/state.js) — `POLL_MS` | ✅ Sim |
| Aparência de uma carta | [`static/universos/poker/cards.js`](../yggdrasil-web/static/universos/poker/cards.js) | ✅ Sim |

> ⚠️ **Há um arquivo legacy** em `yggdrasil-core/src/games/poker.rs`
> (`YggPoker`, ~323 linhas). É um adapter **single-player** de YG-9, hoje
> **morto** — só seus próprios testes o usam. O multiplayer real
> (YG-23/25) é o caminho `PokerLobby → PokerTable` descrito acima.
> Ignore `poker.rs` ao caçar lógica do jogo.

---

## TL;DR

- Pôquer multiplayer é um **estado autoritativo no servidor** (Rust) +
  **um espelho no cliente** (JS) que é refrescado por **polling HTTP** a
  cada 2 segundos.
- A **engine de regras** (deck, dealer, hand evaluator, betting rounds)
  vem inteira do crate `game_core` em [`co/game-core`](https://github.com/artelonga/co).
  Yggdrasil **não duplica** essa lógica — só compõe ao redor dela.
- Identidade chega via **CO SSO**: o handover recebe um JWT ES256
  assinado pelo `co.artelonga.com.br`, valida via JWKS, e mintar um JWT
  HS256 local de curto prazo para uso nas chamadas de poker.
- **Mensageria de gameplay** é HTTP/JSON direto contra o `yggdrasil-web`.
  O CO não está no caminho quente do jogo — só na autenticação inicial.
- O cliente é hoje **7 módulos ES** sob [`yggdrasil-web/static/universos/poker/`](../yggdrasil-web/static/universos/poker/),
  refatoração descendente do `poker.js` monolítico (549 linhas, agora
  arquivado).

---

## Camadas

```
┌─────────────────────────────────────────────────────────────────────┐
│ Browser (cliente)                                                   │
│ ┌─────────────────────────────────────────────────────────────────┐ │
│ │ static/universos/poker/   (ES modules, vanilla JS)              │ │
│ │   main.js  state.js  api.js  cards.js  views.js  actions.js     │ │
│ └─────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────┬────────────────────────────────────┘
                                 │ HTTP/JSON + Authorization: Bearer
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ yggdrasil-web (Axum)                                                │
│ ┌─────────────────────────────────────────────────────────────────┐ │
│ │ src/games/poker_routes.rs        — handlers HTTP + auth gate    │ │
│ │ src/games/poker_persistence.rs   — snapshot ↔ SQLite (YG-29)    │ │
│ │ src/games/poker_favorites.rs     — recent + favorited hands     │ │
│ └────────────────────┬────────────────────────────────────────────┘ │
└──────────────────────┼──────────────────────────────────────────────┘
                       │ chama
                       ▼
┌─────────────────────────────────────────────────────────────────────┐
│ yggdrasil-core (composição)                                         │
│ ┌─────────────────────────────────────────────────────────────────┐ │
│ │ src/games/poker_engine.rs  — fronteira única de import engine   │ │
│ │ src/games/poker_lobby.rs   — seating, bot-fill rule             │ │
│ │ src/games/poker_game.rs    — PokerTable: lobby × game + chips   │ │
│ │ src/games/poker_bot.rs     — auto-step do Bot Carvalho          │ │
│ │ src/sementes.rs            — buy-in / cash-out (wallet)         │ │
│ └────────────────────┬────────────────────────────────────────────┘ │
└──────────────────────┼──────────────────────────────────────────────┘
                       │ depende de (SÓ via poker_engine.rs)
                       ▼
┌─────────────────────────────────────────────────────────────────────┐
│ co/game-core (engine reutilizado)                                   │
│   PokerGame, Deck, BettingRound, PlayerStatus, PokerAction,         │
│   GameConfig, hand evaluator (showdown).                            │
│   Yggdrasil NUNCA modifica esse crate — só compõe.                  │
└─────────────────────────────────────────────────────────────────────┘
```

Cada camada tem **uma responsabilidade única**:

| Camada | Responsabilidade | NÃO sabe sobre |
|---|---|---|
| `co/game-core` | Regras do pôquer (rank, suit, evaluate, betting) | Usuários, HTTP, persistência |
| `yggdrasil-core::poker_engine` | Re-export do engine — único ponto de import | Tudo. É só `pub use`. |
| `yggdrasil-core::poker_lobby` | Quem está em qual assento | Cartas, apostas, HTTP |
| `yggdrasil-core::poker_game` | Composição lobby + game; chips entre mãos | HTTP, JWT, SQLite |
| `yggdrasil-core::poker_bot` | Política de ação do bot | UI, polling |
| `yggdrasil-web::poker_routes` | Endpoints + autenticação + persistência via callbacks | Regras de pôquer |
| `yggdrasil-web::poker_persistence` | Snapshot → SQLite + load | Regras, HTTP |
| `static/universos/poker/*.js` | Renderização + polling + input do usuário | Regras (delega ao server) |

> **Regra de import**: nenhum arquivo da camada `yggdrasil-core::poker_*`
> ou `yggdrasil-web::poker_routes` deve ter `use game_core::games::poker::*`
> ou `use game_core::PokerGame`. Tudo passa por `poker_engine.rs`.
> Verificável com `grep -rn "use game_core::games::poker\|use game_core::PokerGame" yggdrasil-{core,web}/src` — deve retornar vazio.

---

## State Tracking

### Autoridade: o servidor

A única fonte de verdade do jogo é
[`PokerState.tables`](../yggdrasil-web/src/games/poker_routes.rs#L66) — um
`Mutex<Vec<PokerTable>>` em memória. Cada `PokerTable` carrega:

- `lobby: PokerLobby` — quem está sentado, com `sat_at` e `kind` (Human/Bot/Empty).
- `game: Option<PokerGame>` — a engine de `co/game-core`. `None` entre mãos.
- `stacks: HashMap<user_id, chips>` — chip stack que sobrevive entre mãos.
- `current_hand_id: String` — ID único `{table_id}-{millis}` para indexar snapshots.
- `hand_ended_at: Option<DateTime<Utc>>` — quando a mão atual terminou
  (`game_over = true`). Habilita o auto-restart após 5s.

Funções-chave:

- [`PokerTable::new`](../yggdrasil-core/src/games/poker_game.rs#L117) —
  constrói mesa vazia a partir de um `PokerLobby`.
- [`PokerTable::sit_with_sementes`](../yggdrasil-core/src/games/poker_game.rs) —
  debita buy-in da carteira, marca o assento, atualiza `stacks`.
- [`PokerTable::stand_with_sementes`](../yggdrasil-core/src/games/poker_game.rs) —
  credita stack remanescente de volta à carteira.
- [`PokerTable::start_hand`](../yggdrasil-core/src/games/poker_game.rs) —
  instancia novo `PokerGame`, regenera `current_hand_id`, distribui hole cards.
- [`PokerTable::act`](../yggdrasil-core/src/games/poker_game.rs) — valida que
  é a vez do `user_id` e aplica `PokerAction` (fold/check/call/raise).
- [`PokerTable::hand_state`](../yggdrasil-core/src/games/poker_game.rs) —
  projeta `HandState` (público, sem hole cards de adversários).
- [`PokerTable::hole_cards_for`](../yggdrasil-core/src/games/poker_game.rs) —
  retorna as duas cartas do usuário autenticado.
- [`PokerTable::should_auto_restart`](../yggdrasil-core/src/games/poker_game.rs#L132) —
  `true` quando `(now - hand_ended_at) ≥ 5s`. Sem isso, o showdown ficava preso
  indefinidamente.

### Persistência: snapshots em SQLite

[`poker_persistence.rs`](../yggdrasil-web/src/games/poker_persistence.rs)
salva apenas **seating + chip stacks** — `PokerGame` em curso é
descartado num crash. A justificativa está na docstring de
[`PokerTableSnapshot`](../yggdrasil-core/src/games/poker_game.rs#L110):

> Engine `PokerGame` não é serde-friendly, e o custo de uma mão perdida
> (≤ algumas dezenas de sementes) é muito menor que perder buy-ins (1k+).

A persistência roda **após cada mutação relevante** via
[`PokerState::persist_table`](../yggdrasil-web/src/games/poker_routes.rs#L146):

- após `sit` / `stand` (mudou seating)
- após `act` (mudou chips)

### Espelho cliente

O cliente mantém um **subset** do estado do servidor em
[`state.js`](../yggdrasil-web/static/universos/poker/state.js):

```js
{
  token, userId,            // identidade decodificada do JWT local
  lobbies, activeLobby,     // lista refrescada por polling
  meSeated,                 // derivada de lobby.seats
  pollTimer, listPollTimer, // handles dos setInterval
  lastCommunityKey,         // chave anti-flicker: rank+suit das community
  lastHoleKey,              // idem para hole cards
  lastRound, handEndedAcked // gating de animações + saldo refresh debounce
}
```

O cliente **nunca decide regras** — manda a intenção via `POST /action`
e re-renderiza a partir do `HandState` que volta. Quando o cliente
discorda do servidor (race entre polling e ação), a próxima poll reconcilia.

---

## Processamento em Tempo Real

### Polling, não WebSocket (ainda)

O modelo em produção é **polling HTTP** com dois ritmos:

| Onde | Intervalo | O que recarrega |
|---|---|---|
| Lobby selector (lista de mesas) | **4s** | `GET /api/v1/poker/lobbies` |
| Mesa ativa (jogador sentado) | **2s** | `GET /api/v1/poker/lobbies/{id}` + `/hand` + `/hole-cards` |

Implementação no cliente:

- [`startListPolling` / `stopListPolling`](../yggdrasil-web/static/universos/poker/views.js) —
  4s, ativo enquanto o seletor está visível.
- [`startTablePolling` / `stopTablePolling`](../yggdrasil-web/static/universos/poker/actions.js) —
  2s, ativo enquanto a mesa está aberta.

A troca de uma vista para outra `stop`a o timer da vista anterior. Isso é
crítico: sem `stop`, os dois timers somam carga ao backend e ainda
disputam a renderização.

**Por que polling e não WS hoje?** Decisão pragmática. Polling cabe em
HTTP/Axum + qualquer infra (sem upgrades, sem sticky sessions, sem
proxies WS-aware). A latência de 2s é adequada para pôquer turn-based, e
o backend é trivialmente horizontal-friendly. A migração para WebSocket
está planejada como [YG-28](../work/yggdrasil/YG-28.md) — não é
prioridade enquanto o jogo escala em dezenas, não milhares, de mesas.

### Bot auto-step: o servidor age dentro do request

Quando é a vez de um bot, o servidor **não espera** uma chamada externa
para destravar a mesa. Em cada `GET /hand` e `POST /action`, o handler
chama
[`auto_step_bots`](../yggdrasil-core/src/games/poker_bot.rs) imediatamente
depois da mutação humana. Isso garante que o `current_actor` retornado
ao cliente é **sempre humano** (ou `None` em fim de mão).

Sem essa convergência síncrona, o cliente humano teria que pollar até o
bot agir — UX ruim e desperdício de RTTs. O teste
[`humano_vs_bot_completa_mao_sem_travar_via_http`](../yggdrasil-web/src/games/poker_routes.rs#L1031)
é a regressão dessa decisão.

### Anti-flicker: cache de chaves no cliente

O polling de 2s reescreveria o DOM 30×/min com o mesmo HTML, causando
piscar das cartas. Solução em
[`views.js::renderGame`](../yggdrasil-web/static/universos/poker/views.js):

```js
const communityKey = hand.community_cards.map((c) => `${c.rank}${c.suit}`).join(',');
if (communityKey !== state.lastCommunityKey) {
  // só rebuild quando muda
  state.lastCommunityKey = communityKey;
  el.communityCards.innerHTML = '';
  // ...
}
```

Mesma ideia para `lastHoleKey`, e `handEndedAcked` para evitar refrescar
o saldo a cada poll após o showdown.

### Auto-restart após showdown

Após `game_over = true`, [`PokerTable::hand_ended_at`](../yggdrasil-core/src/games/poker_game.rs#L92)
é marcado. Em cada `GET /hand`, [`get_hand`](../yggdrasil-web/src/games/poker_routes.rs#L351)
checa se passou `HAND_END_RESTART_DELAY_SECS` (5s):

```rust
let needs_new_hand = table.game.is_none()
    || (table.game.as_ref().map(|g| g.game_over).unwrap_or(false)
        && table.should_auto_restart());
if needs_new_hand {
    let _ = table.start_hand();
}
```

5 segundos é tempo bastante para o vencedor ser visto + curto o
suficiente para o jogo fluir.

---

## Mensageria através dos servidores CO

Há **dois** servidores na história, com responsabilidades disjuntas:

| Servidor | URL | Responsabilidade na mesa de pôquer |
|---|---|---|
| **CO** (Identity) | `co.artelonga.com.br` | Emite JWT ES256 para o `yggdrasil-web` via handover OAuth Google |
| **Yggdrasil** (Game) | `yggdrasil-artelonga.fly.dev` | Toda mensageria de gameplay (sit/stand/act/poll) |

Em outras palavras: **CO autentica, Yggdrasil joga**. Depois que o JWT
local foi mintado, o CO sai do caminho quente — todas as 2s-polls e
posts de ação batem apenas no Yggdrasil.

### Handover de identidade (one-time, no login)

1. Cliente clica "Entrar com Google" no lobby/poker → redireciona para
   `/auth/co-login?next=/universos/poker` em [`receive_co_handover`](../yggdrasil-web/src/main.rs)
   handlers.
2. CO faz OAuth com o Google. No sucesso, redireciona de volta com
   `?co_token=<JWT-ES256>` para `/auth/co-handover-receive`.
3. Yggdrasil **valida o ES256** usando o JWKS público do CO
   ([`auth_co::JwksCache`](../yggdrasil-web/src/auth_co.rs)), extrai
   `sub` (user_id) + `email`, e **mintar** um JWT HS256 local válido por
   ~horas.
4. JWT local é guardado em `localStorage` como `yggdrasil-jwt`. A partir
   daqui, todas as chamadas de poker vão com `Authorization: Bearer <local-jwt>`.
5. (Lazy upsert) Em cada handover bem-sucedido, [`user_profiles::upsert`](../yggdrasil-web/src/api/user_profiles.rs)
   grava `user_id → username` (slug do email) para que o leaderboard
   mostre nomes legíveis em vez de IDs opacos.

Por que dois JWTs? **Isolamento de superfícies**. CO emite ES256 (par de
chaves assimétricas) porque é um issuer cross-domain. Yggdrasil emite
HS256 (segredo simétrico) porque só ele precisa verificar suas próprias
chamadas — não há terceiro consumidor. Esse split mantém o segredo HS256
trancado dentro do Yggdrasil, e a chave privada ES256 trancada dentro
do CO.

### Lifecycle de uma ação (request quente)

Exemplo: humano dá raise.

```
[Browser]                              [Yggdrasil]                      [SQLite]
poker/actions.js::sendAction('raise')
  POST /api/v1/poker/lobbies/carvalho/action
  Authorization: Bearer <local-jwt>
  { "action": "raise", "amount": 80 }
                                  ┌── require_user → verify_jwt (HS256)
                                  ├── tables.lock()
                                  ├── PokerTable::act → engine valida + muta
                                  ├── auto_step_bots(table)  (bot responde se for sua vez)
                                  ├── persist_table → save_lobby ──────► UPDATE poker_lobbies
                                  ├── capture_hand_snapshot (se game_over)
                                  │                          ──────► UPSERT poker_recent_hands
                                  ├── enrich_usernames (LEFT JOIN user_profiles)
                                  └── HandState JSON ─────────┐
  ←──────────────────────────────────────────────────────────┘
poker/views.js::renderGame(hand, holeCards)
  • diff anti-flicker
  • atualiza pot, current_bet, players
  • mostra/oculta action bar baseado em current_actor
```

Tudo dentro de um Mutex, num único request. O cliente nunca segura lock;
o servidor garante atomicidade da transição.

### Componentes da mensagem

| Header / Campo | Valor | Quem produz |
|---|---|---|
| `Authorization` | `Bearer <local-jwt>` | Cliente (lido de localStorage) |
| `Content-Type` | `application/json` | Cliente |
| `sub` no JWT | `usr_<8-hex>` (formato CO) | CO no handover |
| `email` no JWT | string | CO no handover |
| Status `401` | "JWT ausente/inválido/expirado" | Yggdrasil |
| Status `402` | "Saldo insuficiente" no sit | Yggdrasil (sementes) |
| Status `409` | "Não é sua vez" | Yggdrasil (engine) |
| Status `422` | "Ação inválida" | Yggdrasil (engine) |
| Resposta sucesso | `HandState` JSON | Yggdrasil |

### Sem WebSocket — explicitamente

Não há canal duplex. Toda mensageria é request-response client-initiated.
Consequências:

- ✅ Reconexão é trivial: próximo poll, ou próxima ação.
- ✅ Auth é stateless: o JWT cabe em cada request.
- ✅ Backend horizontal-friendly: nenhuma sticky session.
- ❌ Latência mínima é 2s (período do polling).
- ❌ Eventos de servidor (ex: outro jogador entrou) só chegam no
  próximo poll.

YG-28 substitui isso por WS quando a base de jogadores justificar.

---

## Single Responsibility: onde aplicamos, onde estamos devendo

### O que está bom

- **`co/game-core` vs `yggdrasil-core::poker_*`**: regra de pôquer NÃO
  vaza para a camada de orquestração. `PokerTable` compõe `PokerGame`,
  não estende.
- **`poker_lobby.rs` vs `poker_game.rs`**: seating é um módulo separado
  de gameplay. Você pode rodar lobby standalone (existe na fase YG-23
  antes de YG-25 plugar o engine).
- **`poker_persistence.rs` vs `poker_routes.rs`**: snapshots não sabem
  HTTP. Routes não escrevem SQL direto.
- **`poker_bot.rs` vs `poker_game.rs`**: política do bot é um módulo;
  pode ser substituída sem tocar no engine.

### O que estava devendo (corrigido nesta entrega)

- **`poker.js` 549 linhas**: misturava DOM refs, fetch, JWT decode,
  rendering, polling lifecycle, anti-flicker e composição num único
  global state. **Agora split em 7 ES modules sob `poker/`** com
  imports explícitos. Veja [Mapa de módulos do cliente](#mapa-de-módulos-do-cliente).

### O que ainda deve

- **`poker_routes.rs` é grande (1186 linhas)**: handlers HTTP +
  `enrich_usernames` + `capture_hand_snapshot` + módulo inteiro de
  favoritos. Pode ser dividido em `poker_routes/seating.rs`,
  `gameplay.rs`, `favorites.rs`. Não é urgente — cada handler é
  pequeno e a coesão interna ajuda navegação.
- **Bot policy é fixa (`auto_step_bots`)**: não há estratégia
  configurável. Quando YG-26 priorizar bots mais inteligentes,
  introduzir trait `BotPolicy` em `yggdrasil-core` permite swap.
- **Polling acoplado a `setInterval`**: ideal era um `Poller` reutilizável
  parametrizado por endpoint + intervalo, compartilhado entre snake/tetris/
  invaders/poker. Pendente.

---

## Mapa de Módulos do Cliente

Após o split, `static/universos/poker/` tem **sete arquivos pequenos e
explícitos**, importados por ES modules nativos (sem build step):

| Módulo | Responsabilidade | Exporta |
|---|---|---|
| [`state.js`](../yggdrasil-web/static/universos/poker/state.js) | Singleton `state` + DOM refs `el` + constantes | `state`, `el`, `STORAGE_KEY`, `POLL_MS`, `LIST_POLL_MS`, `BUY_IN_SEMENTES` |
| [`cards.js`](../yggdrasil-web/static/universos/poker/cards.js) | Renderização pura de carta (frente/verso). Zero deps. | `cardEl`, `cardBackEl` |
| [`ui.js`](../yggdrasil-web/static/universos/poker/ui.js) | Banner de erro, status, CTA de login, hideGameArea | `setStatus`, `showError`, `showLoginCta`, `hideGameArea` |
| [`api.js`](../yggdrasil-web/static/universos/poker/api.js) | Fetch wrapper com auth, decode JWT, 401 → logout | `api`, `decodeJwt` |
| [`views.js`](../yggdrasil-web/static/universos/poker/views.js) | Rendering: lobby list, table seats, game state + list polling | `renderLobbyList`, `renderTable`, `renderGame`, `setViewHandlers`, `startListPolling`, `stopListPolling` |
| [`actions.js`](../yggdrasil-web/static/universos/poker/actions.js) | Comandos + polling da mesa: sit/stand/sendAction/loadLobbies/refreshLobby/refreshHand | `sit`, `stand`, `sendAction`, `loadLobbies`, `enterLobby`, `leaveLobby`, `refreshLobby`, `refreshHand`, `refreshSaldo`, `saveFavoriteHand`, `startTablePolling`, `stopTablePolling` |
| [`main.js`](../yggdrasil-web/static/universos/poker/main.js) | Composition root: boot + event wiring + injeta callbacks em `views.setViewHandlers` | (entry; importado pelo `<script type="module">` em `poker.html`) |

Grafo de dependências (sem ciclos — callbacks injetados quebram a circularidade entre views↔actions):

```
main.js (boot)
  ├── state.js   ← raiz, sem deps
  ├── ui.js                ← deps: state
  ├── api.js               ← deps: state, ui
  ├── cards.js   ← folha, sem deps
  ├── views.js             ← deps: state, ui, cards.  Recebe callbacks via setViewHandlers().
  └── actions.js           ← deps: state, ui, api, views, cards
```

Cada módulo é importável em isolamento — `cards.js` em particular não
tem dependência alguma e poderia ser reusado por outro universo de
cartas no futuro.

---

## Mapa de Métodos (Server-side)

### Endpoints HTTP

| Método + Path | Handler | Auth | Efeito |
|---|---|---|---|
| `GET /api/v1/poker/lobbies` | [`list_lobbies`](../yggdrasil-web/src/games/poker_routes.rs#L255) | required | Lista as 3 mesas com `usernames` map |
| `GET /api/v1/poker/lobbies/{id}` | [`get_lobby`](../yggdrasil-web/src/games/poker_routes.rs#L272) | required | Detalhe da mesa + usernames |
| `POST /api/v1/poker/lobbies/{id}/sit` | [`sit`](../yggdrasil-web/src/games/poker_routes.rs#L300) | required | Debita buy-in, ocupa assento, persiste |
| `POST /api/v1/poker/lobbies/{id}/stand` | [`stand`](../yggdrasil-web/src/games/poker_routes.rs#L324) | required | Credita stack remanescente, persiste |
| `GET /api/v1/poker/lobbies/{id}/hand` | [`get_hand`](../yggdrasil-web/src/games/poker_routes.rs#L351) | required | Auto-start + auto-step bots + `HandState` público |
| `GET /api/v1/poker/lobbies/{id}/hole-cards` | [`get_hole_cards`](../yggdrasil-web/src/games/poker_routes.rs#L383) | required | Apenas as cartas do `sub` autenticado |
| `POST /api/v1/poker/lobbies/{id}/action` | [`post_action`](../yggdrasil-web/src/games/poker_routes.rs#L410) | required | Valida vez, aplica fold/check/call/raise, persiste |
| `POST /api/v1/me/favorites/hands/{table_id}` | [`favorite_last_hand`](../yggdrasil-web/src/games/poker_routes.rs#L534) | required | Move último snapshot para favoritas |
| `GET /api/v1/me/favorites/hands` | [`list_favorite_hands`](../yggdrasil-web/src/games/poker_routes.rs#L592) | required | Lista até 50 mãos favoritadas |

### Funções auxiliares (não HTTP)

| Função | Local | Responsabilidade |
|---|---|---|
| [`enrich_usernames`](../yggdrasil-web/src/games/poker_routes.rs#L31) | poker_routes | Resolve `user_id → username` em `HandState` |
| [`lobby_usernames`](../yggdrasil-web/src/games/poker_routes.rs#L45) | poker_routes | Constrói mapa `{user_id: username}` para todas as mesas |
| [`capture_hand_snapshot`](../yggdrasil-web/src/games/poker_routes.rs#L471) | poker_routes | Grava `HandSnapshot` em `poker_recent_hands` (TTL 1h) |
| [`PokerState::persist_table`](../yggdrasil-web/src/games/poker_routes.rs#L146) | poker_routes | Snapshot table → SQLite (idempotente) |
| [`PokerState::with_persistence`](../yggdrasil-web/src/games/poker_routes.rs#L110) | poker_routes | Boot: load do DB ou seed defaults |
| [`auto_step_bots`](../yggdrasil-core/src/games/poker_bot.rs) | poker_bot | Bot age enquanto for `current_actor` |
| [`PokerTable::should_auto_restart`](../yggdrasil-core/src/games/poker_game.rs#L132) | poker_game | True se `now - hand_ended_at ≥ 5s` |
| [`PokerTable::to_snapshot`](../yggdrasil-core/src/games/poker_game.rs#L143) | poker_game | Snapshot serializável (sem `PokerGame` em curso) |
| [`init_poker_db`](../yggdrasil-web/src/games/poker_persistence.rs) | poker_persistence | Cria tabela `poker_lobbies` se não existe |
| [`load_tables`](../yggdrasil-web/src/games/poker_persistence.rs) | poker_persistence | Restaura `Vec<PokerTable>` do disco |
| [`save_lobby`](../yggdrasil-web/src/games/poker_persistence.rs) | poker_persistence | UPSERT snapshot por `lobby.id` |
| [`save_recent`](../yggdrasil-web/src/games/poker_favorites.rs) | poker_favorites | Grava `HandSnapshot` em `poker_recent_hands` |
| [`favorite`](../yggdrasil-web/src/games/poker_favorites.rs) | poker_favorites | Move snapshot para `poker_favorite_hands` (permanente) |

---

## Roadmap

- **YG-26**: bot AI mais forte (substituir greedy policy por algo
  consciente do pot odds).
- **YG-28**: substitui polling por WebSocket. `views.js` ganha um
  handler de evento; `actions.js` mantém HTTP para escritas.
- **YG-34**: gateway Godot ↔ Rust com JWT, permite que a mesma mesa
  rode tanto no web quanto num cliente Godot 4 nativo.
- **Bot Policy trait** (sem ticket ainda): extrair política em
  `yggdrasil-core::games::bot_policy` para suportar perfis (tight,
  loose, agressivo) e composição de bots de outros universos.
