# Roadmap — **Consumir os textos** (pipeline de ingestão)

> Como levar uma fonte bruta (dicionário, corpus, PDF escaneado, JSONL) até os
> **artefatos canônicos** que a topologia consome — generalizando o pipeline Mbyá
> **que já existe e funciona**. Para revisão. 2026-06-28.

## Princípio

Cada texto é uma **fonte** com um **adaptador**. Adicionar uma língua/corpus novo é
escrever um adaptador, **não** reescrever o pipeline. Tudo emite o **mesmo esquema
canônico**, que o `yggdrasil` já lê (`public.rs` + `topologia::db_context`). Sem dado
fabricado; texto sagrado só com consentimento das custódias (CARE).

## O pipeline que JÁ existe (referência: Mbyá)

Fonte → artefato, comprovado no repo `mbya/`:

| Etapa | Mbyá (real) | Saída |
|---|---|---|
| **1. Adquirir** | Dicionário Dooley (PDF) · Ayvu Rapytã Cadogan (`GML00018.pdf`) | PDFs em `mbya/` |
| **2. Extrair** | `extract_pdf.py` (pdftotext + PyMuPDF p/ vogais nasais) · `scripts/extract-ayvu.py` (bbox 2-colunas → versos paralelos + NOTAS) | texto/JSON estruturado |
| **3. Parsear** | `crates/parser/` (Rust: clean/extract/langid/parser → `db.rs`) | `mbya_lexicon.db` (`entries`, `examples`) |
| **4. Carregar corpus** | `scripts/load-ayvu.py` (work→chapters→`corpus_sentences` gn+es, `alignments`, `notes`) | tabelas de corpus |
| **5. Alinhar** | `load-ayvu.py` passo 2: tokeniza versos → `corpus_tokens`, **liga 1:1** ao `entries` por headword normalizado | `corpus_tokens.entry_id` |
| **6. Decompor (étimo)** | `scripts/bake-lexicon-decomp.py` (NOTAS de Cadogan → morfemas → `decomp`) | `decomp` por termo |
| **7. Emitir artefatos** | `corpus-to-json.py` → `comunicacao/corpus/ayvu-rapyta.json`; `lexicon-to-markdown.py`/`corpus-to-markdown.py`; lexicon baked → `guarani-mbya/lexicon.mbya.json` (`pop` = nº de exemplos) | artefatos no universo |
| **8. Consumir** | `yggdrasil` (`public::lexicon_slice` + `topologia::db_context`) | grafo + cosseno |

**O esquema canônico (alvo de todo adaptador):**
- **Léxico:** `{ word, lang, gloss, pron, pop, decomp }` (→ `lexicon.<code>.json`).
- **Corpus:** `work → chapters → verses(lang,text,verse_num)`; `tokens(form, entry_id)`
  ligando 1:1 ao léxico; `alignments` (bitext) e `notes` (comentário).

## Fases (consumir mais textos)

### Fase A — Formalizar o contrato de ingestão *(doc + 1 schema)*
Extrair o esquema canônico (acima) para `docs/architecture/ingestion-contract.md` e
um `schema.sql` único. Mbyá vira o **adaptador de referência**. Nada de código novo
de dado — só nomear o contrato que já emerge do pipeline Mbyá.
- **DoD**: contrato escrito; Mbyá mapeado a ele 1:1.

### Fase B — **Source manifest** + GATE de soberania *(por texto)*
Cada fonte declara um manifesto (`comunicacao/<lang>/_source.yaml`):
`{ titulo, autor, ano, formato, url, licença, consentimento_custodia: sim|nao|na,
parser }`. O build **recusa** consumir fonte sagrada sem `consentimento_custodia`.
- **DoD**: manifesto p/ Mbyá (Dooley = dicionário; Ayvu Rapytã = consentimento
  registrado) e Yoruba; gate ativo.

### Fase C — **Yoruba: léxico aberto (kaikki)** *(1º texto novo)*
Adaptador `scripts/ingest-kaikki.py`: JSONL Wiktextract (do **rawdata**, não o JSONL
*deprecated*) → `comunicacao/yoruba/lexicon.yo.json` (`word/gloss/pron/pop`), com
normalização de **tom** (mesma dobra do `slugify`). ~4.865 termos, CC-BY-SA.
Estende os 16 stubs já em `comunicacao/yoruba/terms/`.
- **DoD**: `lexicon.yo.json` real no universo; nós Yoruba reais no grafo.

### Fase D — **Yoruba: corpus aberto (Bíblia Yorùbá)** *(alinhamento token↔termo)*
Adaptador: Bíblia Yorùbá (sem copyright; base do UD Treebank) → `corpus/yoruba-bible.json`
no esquema work→livros→versículos; tokeniza e liga ao `lexicon.yo.json`. Substrato de
co-ocorrência p/ o overlay `corpus` em Yoruba.
- **DoD**: versos Yoruba como instâncias reais por termo; overlay corpus Yoruba.

### Fase E — **Ifá Odù — GATED** *(só com custódias)*
O gêmeo do Ayvu Rapytã (256 odù → versos). Adaptador idêntico em forma, MAS
**bloqueado** até consentimento das custódias (CARE; edição Abimbola é copyright +
sagrada). Documentar o caminho; **não** ingerir ainda.
- **DoD**: manifesto Ifá com `consentimento_custodia: nao` → build pula; trilha de
  relação/consentimento aberta.

### Fase F — **Reuso recursivo** *(qualquer língua/corpus)*
Novo texto = novo manifesto + adaptador que emite o esquema canônico. O grafo, o
cosseno e o viz não mudam. Embeddings (roadmap local-first, Fase 3) consomem o mesmo
artefato.

## Prioridade p/ revisão
1. **A+B** (contrato + manifest/gate) — barato, destrava o resto com governança.
2. **C** (kaikki Yoruba) — abre o 2º universo de língua com dado real, já.
3. **D** (Bíblia Yorùbá) — dá contexto/alinhamento real ao Yoruba.
4. **E** (Ifá) — só atrás do gate de soberania.

## Invariantes
Sem dado fabricado · esquema canônico único · texto sagrado só com consentimento ·
normalização de tom/diacrítico no ingest · artefatos versionados no universo
`comunicacao` (conteúdo), consumidos pelo `yggdrasil` (forma).
