## fix — `/api/health` no caminho de spec, com versão

O health-check ficava em `GET /health` e devolvia apenas o texto `ok` — fora do
caminho de spec (`/api/health`) e sem expor a versão do binário, dificultando
verificar qual release está no ar via health-check.

### Added
- **`GET /api/health`**: JSON `{"ok":true,"version":"<CARGO_PKG_VERSION>","service":"yggdrasil"}`.

### Notas
- `GET /health` (texto `ok`) permanece **inalterado** para back-compat.
