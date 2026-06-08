# Catálogo de universos — REGISTRY.yaml + filtros dinâmicos

> YG-70 / YG-72. Posiciona o Yggdrasil como **catálogo de universos abertos**
> (PT-BR first, foco em RPG brasileiro), não só um lobby de mini-games.

## Visão

Antes: "Yggdrasil é um lobby com 6 jogos."
Depois: "Yggdrasil é um catálogo de universos abertos — alguns já jogáveis aqui,
dezenas mapeados como planejados ou externos, mais entrando."

O catálogo tem três classes de entrada:

| status     | Significado                                   | `playable` | Ação no card           |
|------------|-----------------------------------------------|------------|------------------------|
| `embedded` | WASM/engine em produção, jogável aqui         | `true`     | "Jogar"                |
| `planned`  | Mapeado, sem código ainda (placeholder)       | `false`    | "Em breve — contribuir"|
| `external` | Existe fora da plataforma (link out)          | `false`    | "Saiba mais"           |

## Fonte da verdade — `universes/REGISTRY.yaml`

Declara TODAS as entradas do catálogo. É lido em **compile-time** por
`yggdrasil-web/src/catalog.rs` via `include_str!` + `serde_yaml` — mudou o YAML,
recompila.

### Schema de uma entrada

```yaml
- slug: tagmar              # (obrigatório) identificador único, kebab-case
  status: planned           # (obrigatório) embedded | planned | external
  type: rpg                 # (obrigatório) rpg | arcade | puzzle | atlas | lingua | ferramentas | mesa
  title: "Tagmar 3"         # (obrigatório) nome humano (PT-BR)
  description: "..."        # (obrigatório) descrição curta (PT-BR)
  genre: [fantasy, medieval]# lista de gêneros — usada no filtro multi
  origin: brazilian         # brazilian | international | original
  license: open-source      # open-source | commercial | MIT | "MIT AND CC-BY-SA-4.0" | ...
  creators: ["Comunidade Tagmar"]  # autores/criadores
  year: 1991                # ano de criação/publicação
  external_url: https://...        # obrigatório quando status: external
  target_release: v1.5.0    # versão do Yggdrasil em que planejamos portar (planned)
  port_difficulty: medium   # easy | medium | hard (chute inicial)
  versions_tracked: true    # crate tem CHANGELOG próprio via git pathspec (YG-71)
```

Campos obrigatórios: `slug`, `status`, `type`, `title`, `description`.
Todos os demais são opcionais (serializados apenas quando presentes).

## API — `GET /api/v1/universos`

O endpoint faz o **merge** do REGISTRY com o runtime real dos universos
embedados (que têm `max_players`, `api_version`, `version`).

### Formatos de resposta

- **Sem filtros** (compat v1.0): devolve um **array** JSON, cada item enriquecido
  com os campos novos (`status`, `type`, `playable`, `origin`, …). Os campos
  legados (`id`, `name`, `max_players`, `api_version`) são preservados nos
  embedados — clientes antigos não quebram.
- **Com qualquer filtro, ou `?format=catalog`**: devolve um **envelope**:

```json
{
  "universos": [ { "slug": "snake", "status": "embedded", "playable": true, ... } ],
  "total": 40,
  "by_status": { "embedded": 7, "planned": 30, "external": 3 }
}
```

### Filtros (query string)

| Param     | Valores                                    | Semântica                         |
|-----------|--------------------------------------------|-----------------------------------|
| `status`  | `embedded` `planned` `external` `all`      | default `all`                     |
| `type`    | `rpg` `arcade` `puzzle` …                   | match exato                       |
| `origin`  | `brazilian` `international` `original`      | match exato                       |
| `genre`   | lista separada por vírgula                  | match se tiver **qualquer** um    |
| `license` | `open-source` `commercial` `all`           | match exato                       |
| `search`  | texto livre                                | substring em `title`+`description`|
| `format`  | `catalog`                                  | força o envelope                  |

Tudo case-insensitive; `all`/vazio desliga o filtro do campo.

## Frontend — `/universos`

`static/universos/index.html` + `index.js`:

- Consome `GET /api/v1/universos?format=catalog` e agrega salas de comunicação
  e instâncias autoradas publicadas (fontes vivas).
- Grid de cards com badge de **situação** (🟢 jogável / 🟡 planejado / 🔗 externo)
  e badge de tipo.
- Filtros client-side: busca, situação, tipo, origem, gênero. Dropdowns
  populados dinamicamente a partir dos dados presentes.
- Card embedado → página do universo. Planejado → template de issue
  "Quero portar". Externo → `external_url` em nova aba.
- Acessível por teclado (selects/inputs nativos com `aria-label`), alto
  contraste no tema escuro.

## RPGs brasileiros no catálogo (YG-72)

~40 sistemas de `docs/RPGs Brasileiros.md` entram como entradas placeholder.
Critérios de classificação:

| Critério                                          | status no REGISTRY |
|---------------------------------------------------|--------------------|
| Open source declarado (ex.: Tagmar)               | `planned`          |
| Indie publicado livremente (download grátis)      | `planned`          |
| Comercial mas com SRD/lore aberto                 | `external` (link)  |
| Comercial proprietário (Tormenta, Ordem Paranormal)| `external` (link) |

Cada entrada `planned` é candidata a um port WASM (`universe-<slug>` + SRD em
markdown), aberto como task YG-? quando priorizado. O `port_difficulty` é um
chute inicial, a confirmar com os mantenedores. Logos/cover art ficam como
placeholder por gênero até confirmarmos licenças.

Para contribuir um port, use o template
`.github/ISSUE_TEMPLATE/portar-rpg.md` ("Quero portar &lt;slug&gt;").

## Versionamento por universo

Universos embedados com `versions_tracked: true` têm CHANGELOG próprio derivado
de git pathspec — ver `scripts/universe-changelog.sh` (YG-71) e
`docs/UNIVERSE-VERSIONING.md` (YG-67).
