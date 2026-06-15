## YG-153 (follow) — vault do CO como portal cross-universe + CI concurrency

### Added
- **Universos do CO viram portais no Mundo unificado** (instance view, MundoView):
  na fronteira (sala-raiz) aparecem portais `co:<key>` para universos do CO
  visíveis ao usuário (logado → `/me/universes`; anon → públicos). Atravessar um
  portal CO carrega o vault via `co-vault.js` (federação inbound client-side) e o
  torna navegável como qualquer outro — read-only no Mundo (CRUD fica no `/co-mundo`);
  a pilha de universos (YG-157) volta normalmente. `abrirNota` lê a entry do CO
  quando o vault atual é do CO. Fecha o "import-from-CO" do YG-153 dentro do fluxo
  normal do Mundo, não só na página standalone.

### Changed
- **CI `concurrency`** (`.github/workflows/ci.yml`): cancela runs superseded do
  mesmo PR/branch (builds chegaram a ~46min de fila com sessões paralelas). `main`
  usa o SHA no group p/ não cancelar deploys legítimos em sequência.
