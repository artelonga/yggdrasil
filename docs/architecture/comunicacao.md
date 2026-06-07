# Universo `comunicacao` — salas interativas de léxico

> Universo autorado (não-arcade) onde cada usuário tem **salas**: mapas
> navegáveis (pan/zoom) onde posiciona palavras em língua nativa, liga-as a
> conceitos âncora e **publica** termos novos de volta no léxico
> cross-linguístico do repositório `comunicacao`. Termos publicados entram numa
> fila de **revisão espaçada**.

## Por que native, não WASM

O ABI v1 dos universos WASM (`universes/universe-*`) é sandbox: só
`kv_get/kv_set`, `emit_event`, sem filesystem nem rede. O requisito central —
**escrever Markdown de volta no repo `comunicacao` atribuído ao usuário** — não
cabe nessa sandbox. Logo o universo é **native**: módulo de domínio em
`yggdrasil-core` + rotas Axum em `yggdrasil-web` + frontend canvas, com a escrita
de léxico acontecendo host-side.

## Camadas

| Camada | Caminho | Responsabilidade |
|---|---|---|
| Domínio | `yggdrasil-core/src/comunicacao/` | `room` (estado + edits), `store` (disco), `lexicon` (read/write do repo Markdown), `templates` (yoruba/mbya), `review` (SRS-lite) |
| HTTP | `yggdrasil-web/src/comunicacao_routes.rs` | `/api/v1/comunicacao/*`, auth via `sub` do JWT |
| Frontend | `yggdrasil-web/static/universos/comunicacao.{html,js}` | canvas pan/zoom, inspector, publicar, revisão |
| Registro | `yggdrasil-core/src/universes.rs` | root `comunicacao` + variantes `/yoruba` e `/mbya` |

Auto-contido: depende só de `serde`/`chrono` e de `crate::auth::verify_jwt`. Não
acopla ao editor de instâncias (YG-73) nem ao runtime WASM — reusa apenas os
princípios (disco = fonte da verdade, escrita atômica temp+rename).

## Contrato HTTP

```text
POST   /api/v1/comunicacao/salas?template=yoruba|mbya|blank&title=&lang=
GET    /api/v1/comunicacao/salas                  (do dono; ?published=true = feed)
GET    /api/v1/comunicacao/salas/{id}
PATCH  /api/v1/comunicacao/salas/{id}             (RoomEdit: add/move/edit/delete_element, set_viewport, set_published)
DELETE /api/v1/comunicacao/salas/{id}
POST   /api/v1/comunicacao/salas/{id}/elementos/{eid}/publicar
GET    /api/v1/comunicacao/lexico?lang=&q=
GET    /api/v1/comunicacao/templates
GET    /api/v1/comunicacao/revisao
POST   /api/v1/comunicacao/revisao/nota           ({ term_path, correct })
```

## O "PUT" sala → léxico geral

`publicar` é o elo entre o universo do usuário e o léxico compartilhado:

1. Busca a palavra (por slug com fold de diacríticos/tom) em
   `<lingua>/terms/<slug>.md`.
2. **Existe** → liga (`LexiconLink::Linked`), não duplica.
3. **Não existe** → cria `<lingua>/terms/_users/<usuario-slug>/<slug>.md` com
   frontmatter (`type: term`, `language_code`, `author`,
   `contributed_via: yggdrasil-comunicacao`, `seed_status: stub`),
   atribuído ao usuário (`LexiconLink::Contributed`).
4. Em ambos os casos, enfileira o termo para revisão (idempotente por
   `term_path`).

Mapa de língua → plano: `yo→yoruba`, `gn-mbya→guarani-mbya`, `pt→portuguese`.

## Configuração (env)

| Var | Default | Uso |
|---|---|---|
| `YGGDRASIL_COMUNICACAO_DIR` | `data/comunicacao` | salas + filas de revisão em disco |
| `COMUNICACAO_DIR` | `../comunicacao` | checkout do repo de léxico (write-back) |

## Pendências / próximos passos

- **Portal no lobby** ✅ **shipado** (merge YG-73–91, `70914c2`): `pub const COMUNICACAO`
  em `lobby/portals.rs::slug` + posição em `lobby/grid.rs::pos` + `Tile::Portal("comunicacao")`
  no grid + card em `static/lobby.js`, com o teste `lobby_has_comunicacao_portal_at_documented_pos`.
  (Verificado e fechado por YG-99.)
- **Commit do write-back**: hoje o servidor só *escreve* o arquivo no checkout
  `COMUNICACAO_DIR`. Fazer `git add`/commit (ou abrir PR) das contribuições é um
  passo separado — candidato a um job que sincroniza `_users/` periodicamente.
- **Revisão de curadoria**: promover `_users/<u>/<slug>.md` (`seed_status: stub`)
  a `terms/<slug>.md` (`reviewed`) após confirmação de falante/curador.
- **e2e**: `scripts/e2e-editor.sh` é o molde para um `e2e-comunicacao.sh`
  (login magic-link → criar sala → publicar → revisar) contra servidor real.
