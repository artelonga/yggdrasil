# Roadmap — Topologia de Sentido pelo paradigma **local-first**

> Como levar a **topologia de sentido cross-linguística** (YG-175/172/173) de onde
> está (rodando em localhost sobre dados reais) até um deploy maduro, **sem nunca
> sair do paradigma local-first**. Última atualização: 2026-06-27.

## Por que local-first (não é preferência — é requisito)

1. **Soberania de dados.** O **Ayvu Rapytã** é texto sagrado Mbyá Guaraní (Cadogan
   1959) e o léxico tem 4.837 termos com glosa/exemplos. Esse corpus — e qualquer
   **embedding** dele — **não pode ser enviado a APIs externas**. O sentido mora na
   máquina.
2. **Sem dependência de endpoint de embedding pago.** A Anthropic não tem endpoint
   de embedding; a alternativa externa (Voyage) sairia do paradigma. Já temos o
   substrato local: **`tools/local-inference/`** (Ollama, `server.sh`,
   memory-guarded no M4 Max 36 GB).
3. **Sem dado fabricado** (princípio fundador): tudo se liga ao corpus real
   (`mbya_lexicon.db`, `comunicacao/`).
4. **Já temos as peças**: `scripts/pr-localhost.sh` (serve isolado por PR), os dados
   locais, e o runner de inferência local.

## Estado atual — FEITO

- **YG-175/172/173**: grafo cross-linguístico (nó = termo do léxico **real**),
  arestas por exploração/relação/cosseno; **duas fontes** (léxico vs Ayvu Rapytã) e
  **dois overlays** de cosseno (corpus / léxico); viz "galáxia" de ~4.2k nós;
  instâncias reais (versos + exemplos) por termo. Roda em localhost via env
  (`COMUNICACAO_DIR`, `YGGDRASIL_MBYA_DB`). Vetorizador atual = **TF-IDF** (será
  trocado por embeddings na Fase 3).

---

## Fases

### Fase 0 — Consolidar e versionar  *(1 PR)*
Cortar `feat/YG-175-topologia` de `origin/main`; mover os 3 módulos core/web + a viz
+ `scripts/pr-localhost.sh`/`pr-conflicts.sh` + specs + `CHANGELOG-PENDING/YG-17{1,2,3}`.
- **DoD**: PR aberto, verde (clippy/fmt/tests), revisável por `pr-localhost.sh <PR>`;
  limpa a deriva do branch `feat/YG-165` (23 atrás de main).

### Fase 1 — Deploy LOCAL (o paradigma)  *(infra, leve)*
Um comando sobe a topologia isolada, com os dados reais:
```bash
scripts/pr-localhost.sh <PR#>        # worktree + porta + DB próprios
# env do paradigma local (defaults já apontam para os repos irmãos):
#   COMUNICACAO_DIR=../comunicacao   YGGDRASIL_MBYA_DB=../mbya/mbya_lexicon.db
curl -X POST localhost:<porta>/api/v1/topologia/semantica/recomputar \
     -H "Authorization: Bearer $YGGDRASIL_ADMIN_TOKEN"   # recomputa os 2 overlays
```
- **DoD**: qualquer PR roda a topologia localmente com 1 comando, dados reais, sem
  rede externa. Documentar no README do `scripts/`.

### Fase 2 — Artefato de dados **portátil**  *(remove o acoplamento a ../mbya)*
Hoje o contexto vem do `mbya_lexicon.db` em `../mbya` — fora do deploy. Gerar um
**artefato auto-contido** (build offline, determinístico):
- `scripts/build-topologia-data.sh` → lê `mbya_lexicon.db` + `comunicacao/` e emite
  **`data/topologia.json`** (ou SQLite enxuto): por termo `{gloss, def, exemplos[],
  versos[(cap,verso,text)], cooc[]}`.
- `yggdrasil` lê via **`YGGDRASIL_TOPOLOGIA_DATA`** (cai no DB cru se ausente, p/ dev).
- Nós continuam vindo de `lexicon.mbya.json` (já versionado no universo).
- **DoD**: topologia roda **sem `../mbya`**, só com o artefato — pré-requisito de prod.

### Fase 3 — Embeddings **neurais locais** (Ollama)  *(INICIADO — overlay `neural` no ar)*
Trocar o vetorizador TF-IDF por embeddings de um modelo **local**:
- `ollama pull nomic-embed-text` (ou `mxbai-embed-large`); servir via
  `tools/local-inference/server.sh` (`/api/embeddings`, `localhost:11434`,
  memory-guarded, auto-unload).
- **Pré-computar offline** (job, na Fase 2 build): um vetor por termo para CADA fonte
  (léxico e corpus) → os **dois overlays seguem**. Cosseno sobre embeddings.
- Resolve a **dominância de partículas** do TF-IDF e habilita **cross-language por
  sentido real** (yoruba ↔ mbyá), não só sobreposição de glosa PT.
- Prod serve **vetores já calculados** (no artefato da Fase 2) — **sem Ollama em
  prod**. O local-first vale na *geração*, não no request.
- **Interface model-agnostic (extensível a modelos custom):** um trait
  `Embedder { fn embed(&self, text: &str) -> Vec<f32> }` com backends plugáveis —
  (a) Ollama (`nomic-embed`/`mxbai`), (b) modelos Yoruba/africanos abertos
  (**AfroXLM-R / AfriBERTa / AfroLM / YorùbáBERT**, via runner local), (c) um
  **modelo fine-tuned próprio** (treinado nos córpora reais: Ayvu Rapytã + Odù Ifá
  quando consentido + léxicos). Trocar de modelo **não toca** o modelo de aresta nem o viz —
  só re-gera os vetores no artefato. O `method` da aresta passa a carregar o
  modelo usado (proveniência).
- **DoD**: overlay neural; `va'e~va'ekue` mais limpo; partículas deixam de dominar;
  ao menos um par yoruba↔mbyá emergindo por sentido; trocar o `Embedder` por outro
  modelo é uma flag, não um refactor.

> **Fase 3.5 (interim, se o neural atrasar):** pesagem **PMI** + stopwords Guaraní no
> TF-IDF para amenizar partículas já no overlay atual.

### Fase 4 — Caminho a **prod** (Fly), quando maduro
- Embarcar `data/topologia.json` (Fase 2) na imagem; setar `YGGDRASIL_TOPOLOGIA_DATA`
  no `fly.toml`.
- **Commit de release** (`chore(release): X.Y.Z`) consolida `CHANGELOG-PENDING/*` e
  bumpa `Cargo.toml` (disciplina de release).
- **Deploy só de um commit de release**; `curl prod/version == X.Y.Z`;
  `/universos/topologia` viva.

---

## Lane: Yoruba (2º universo de língua)

Mesmo padrão Mbyá (léxico + corpus canônico ligado token↔termo), aplicado ao Yoruba.
Fonte: deep-research 2026-06-27 (relatório citado).

- **Léxico (abrir já):** **kaikki.org Yoruba** (extrato Wiktextract do Wiktionary,
  ~4.865 palavras com POS/glosa/exemplos/tom, CC-BY-SA) — gêmeo direto de
  `lexicon.mbya.json` (Mbyá tem 4.837). Processar do **rawdata** do wiktextract
  (o JSONL pronto está *deprecated*). → vira `comunicacao/yoruba/lexicon.yo.json`.
- **Léxico (fallback rico, NÃO-aberto):** LDC2008L03 *Global Yoruba Lexical Database*
  (450k palavras, Toolbox/XML, **decomposição morfêmica** como nosso `decomp`) —
  licença paga LDC. Só se houver orçamento.
- **Corpus canônico — Odù Ifá (256 odù = 16 Olódù 16×16; ẹsẹ Ifá):** o gêmeo
  estrutural do Ayvu Rapytã, e o cânone Yoruba (**não a Bíblia** — texto colonial,
  não iorubá). MAS (1) as edições de referência (Abimbola 1976/1977; Bascom 1969)
  estão **sob copyright** e (2) **é conhecimento sagrado vivo** → **GATE de
  soberania** abaixo. Não ingerir sem governança das custódias + edição licenciável.
  Bibliografia em `docs/architecture/yoruba-ifa-references.md`.
- **Enquanto o Ifá é gated:** a lane Yoruba roda **só com o léxico** (kaikki); sem
  corpus substituto — não se troca o cânone sagrado por um texto colonial.
- **Tom/diacrítico:** normalização de tom é de primeira ordem; texto
  normalizado **melhora** scores intrínsecos de embedding → valida nosso `slugify`
  (dobra de tom/diacrítico).

## GATE de soberania (corpora sagrados) — não violável

Antes de digitalizar/ingerir qualquer corpus sagrado vivo (Ifá Odù; e o próprio Ayvu
Rapytã), aplicar os **CARE Principles** (Collective benefit, **Authority to control**,
Responsibility, Ethics; Carroll 2020): as **custódias** — não nós — definem acesso,
licença e protocolo. IP convencional é encaixe imperfeito (precisa proteção
*sui generis*). **Começar pela necessidade da comunidade e por relação de longo
prazo, não pelo dataset.** O Ifá entra no grafo **só com consentimento das custódias**;
até lá, a lane Yoruba roda **só com o léxico (kaikki)** — sem corpus substituto.

## Trilhas paralelas (independentes das fases)
- **Telemetria de co-visitação → `/explorar`** (ponte YG-145 → aresta): wiring do
  tracker client-side, p/ as arestas de exploração nascerem do caminhar real.
- **Promover sugestões**: cosseno (corpus/léxico) → relação `user` nomeada (já no viz).
- **Mais línguas**: cada novo léxico em `comunicacao/<lang>/lexicon.<code>.json` entra
  no grafo sem mudar o modelo.

## Invariantes (não violar)
- Sem dado fabricado · conteúdo×forma (CO=conteúdo, Yggdrasil=forma) · memory-light ·
  engine-neutro · o corpus sagrado e seus embeddings **não saem da máquina**.
