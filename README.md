# Yggdrasil

> Conectando ideias. Construindo universos.

Plataforma viva de criação, organização e conexão de universos digitais. Cada
universo é um espaço onde elementos crescem como raízes, galhos e folhas — e
podem se transformar em mundos jogáveis.

Yggdrasil reaproveita o engine 2D do projeto [`co`](https://github.com/artelonga/co)
(`co/game-core`) e expõe os universos como portais navegáveis no lobby.

## Status

`v0.0.1` — bootstrap. Próxima release planejada: `v0.1.0` (lobby + Snake).
Veja `CHANGELOG.md` e o board de tarefas em `work/yggdrasil/`.

## Estrutura

```
yggdrasil/
├── yggdrasil-core/      # Lobby, modelos de universo, integração com game-core
├── yggdrasil-web/       # Servidor HTTP + lobby renderizado em <canvas>
├── work/yggdrasil/      # Tarefas co-auto (YG-1 .. YG-20)
├── docs/                # Pitch, UX, mocks
└── co-universes.yaml    # Registro para inscrição via co
```

## Conceitos

| Termo (PT) | Inglês | Mapeia em game-core |
|---|---|---|
| Universo | Universe | `Universe` |
| Elemento | Element | `Entity` / `Tile::Entity` |
| Conexão | Connection | (a definir — modelo Yggdrasil) |
| Portal | Portal | `Tile::Portal(target)` |
| Semente | Seed (currency) | `WalletManager` (renomeado em YG-10) |
| Modelo | Template / Plugin | `Plugin` + `PluginManifest` |
| Assinatura | Subscription | `co-universes.yaml: visibility` |

## Desenvolvimento

```bash
# Compilar (depende de ../co/game-core via path dep)
cargo build

# Rodar servidor (depois de YG-3)
cargo run -p yggdrasil-web

# Listar tarefas
ls work/yggdrasil/

# Executar próxima tarefa via co-auto
cd ../co && cargo run -p co-auto -- --workdir ../yggdrasil --space yggdrasil
```

## Recompensas (campanha de financiamento)

Tiers definidos em `docs/REWARDS.md` (ver YG-21). Sementes são a moeda
interna; tiers desbloqueiam skins, jogos privados, early access a universos
3D e assinatura vitalícia.

## Licença

MIT — ver `LICENSE`.
