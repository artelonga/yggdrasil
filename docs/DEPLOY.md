# Deploy — Yggdrasil

Instruções para configurar o ambiente de produção no Fly.io.

## Variáveis de ambiente obrigatórias

| Variável | Descrição | Como definir |
|---|---|---|
| `YGGDRASIL_JWT_SECRET` | Segredo para assinar JWTs | `flyctl secrets set` |
| `YGGDRASIL_DB` | Caminho do banco SQLite principal | `fly.toml [env]` |
| `YGGDRASIL_SEMENTES_DB` | Caminho do banco de sementes | `fly.toml [env]` |

## Hint engine do Universo Vim (Claude API)

O Universo Vim oferece hints adaptativos gerados pela Claude API. A variável é **opcional** — sem ela, o servidor usa hints estáticos por nível (PT-BR) sem nenhum erro.

| Variável | Obrigatório | Descrição |
|---|---|---|
| `ANTHROPIC_API_KEY` | não | Chave da API Anthropic para hints gerados por Claude |

### Configurar no Fly.io

```bash
# Sempre via secrets — nunca expor a chave em fly.toml
flyctl secrets set ANTHROPIC_API_KEY=sk-ant-...
```

### Comportamento

| Cenário | Comportamento |
|---|---|
| `ANTHROPIC_API_KEY` configurado | Hints gerados por `claude-sonnet-4-6` via API Anthropic |
| `ANTHROPIC_API_KEY` ausente | Hints estáticos por nível (PT-BR), sem erro no log |
| Rate limit excedido (5 hints/usuário/hora) | Hint estático por nível, sem chamada à API |
| Erro na API Anthropic | Log `WARN` + hint estático por nível |

### Custo estimado

- ~500 tokens entrada + ~100 tokens saída por hint ≈ $0,002/hint
- Rate limit de 5 hints/hora/usuário → custo máximo por usuário ativo: ~$0,01/hora

## Configuração de SMTP (envio de email)

O servidor detecta automaticamente se SMTP está configurado. Se `YGGDRASIL_SMTP_HOST` não estiver definido (ou estiver vazio), os emails são impressos no stdout — útil para desenvolvimento.

### Variáveis SMTP

| Variável | Obrigatório | Padrão | Descrição |
|---|---|---|---|
| `YGGDRASIL_SMTP_HOST` | sim | — | Hostname do servidor SMTP |
| `YGGDRASIL_SMTP_PORT` | não | `587` | Porta SMTP (STARTTLS) |
| `YGGDRASIL_SMTP_USER` | não | `""` | Usuário de autenticação |
| `YGGDRASIL_SMTP_PASSWORD` | não | `""` | Senha (definir via secrets) |
| `YGGDRASIL_SMTP_FROM` | não | `Yggdrasil <noreply@artelonga.com.br>` | Remetente |

### Configurar no Fly.io

```bash
# Variáveis não-secretas (podem ir em fly.toml [env])
flyctl secrets set YGGDRASIL_SMTP_HOST=sandbox.smtp.mailtrap.io
flyctl secrets set YGGDRASIL_SMTP_USER=<usuario>

# Senha — sempre via secrets (nunca em fly.toml)
flyctl secrets set YGGDRASIL_SMTP_PASSWORD=<senha>
```

---

### Exemplo: Mailtrap (sandbox para testes)

Mailtrap intercepta emails sem entregá-los — ideal para homologação.

1. Criar conta em [mailtrap.io](https://mailtrap.io) e acessar **Email Testing → Inboxes**.
2. Copiar credenciais SMTP da inbox.

```bash
flyctl secrets set YGGDRASIL_SMTP_HOST=sandbox.smtp.mailtrap.io
flyctl secrets set YGGDRASIL_SMTP_PORT=587
flyctl secrets set YGGDRASIL_SMTP_USER=<mailtrap-user>
flyctl secrets set YGGDRASIL_SMTP_PASSWORD=<mailtrap-pass>
```

#### Teste de integração local com Mailtrap

```bash
export YGGDRASIL_SMTP_HOST=sandbox.smtp.mailtrap.io
export YGGDRASIL_SMTP_USER=<mailtrap-user>
export YGGDRASIL_SMTP_PASSWORD=<mailtrap-pass>
export YGGDRASIL_SMTP_TEST_TO=<email-destino>

cargo test smtp_envia_email_via_sandbox -- --ignored --nocapture
```

---

### Exemplo: SendGrid

```bash
flyctl secrets set YGGDRASIL_SMTP_HOST=smtp.sendgrid.net
flyctl secrets set YGGDRASIL_SMTP_PORT=587
flyctl secrets set YGGDRASIL_SMTP_USER=apikey
flyctl secrets set YGGDRASIL_SMTP_PASSWORD=<sendgrid-api-key>
flyctl secrets set YGGDRASIL_SMTP_FROM="Yggdrasil <noreply@artelonga.com.br>"
```

> Pré-requisito SendGrid: verificar domínio `artelonga.com.br` em **Sender Authentication**.

---

### Exemplo: AWS SES

```bash
flyctl secrets set YGGDRASIL_SMTP_HOST=email-smtp.sa-east-1.amazonaws.com
flyctl secrets set YGGDRASIL_SMTP_PORT=587
flyctl secrets set YGGDRASIL_SMTP_USER=<ses-smtp-user>
flyctl secrets set YGGDRASIL_SMTP_PASSWORD=<ses-smtp-password>
flyctl secrets set YGGDRASIL_SMTP_FROM="Yggdrasil <noreply@artelonga.com.br>"
```

> Pré-requisito SES: verificar endereço `noreply@artelonga.com.br` e sair do sandbox (solicitar aumento de limite para envio em produção).

---

## Deploy

```bash
# Do diretório pai (necessário para incluir co/game-core no build context)
cd /caminho/para/projects
flyctl deploy --config yggdrasil/fly.toml \
              --dockerfile yggdrasil/yggdrasil-web/Dockerfile
```

## Logs

```bash
flyctl logs --app yggdrasil-artelonga
```

Quando SMTP não está configurado, o log mostra:

```
WARN smtp not configured — emails go to stdout
```

Quando SMTP está configurado corretamente:

```
INFO SMTP configurado host=sandbox.smtp.mailtrap.io port=587
```
