# Framework NLP de corpus — documento como árvore, corpora como álgebra

> Visão (2026-06-13, conversa de redesenho). Referências: **portuNLP** (wrapper
> spaCy: doc/corpus → tokens/lemmas/POS + keywords por frequência) e
> **SensorySpeech** (pipeline em camadas + módulos pedagógicos/"game").
> Status: design — épico **YG-139**.

## 1. O documento é uma árvore (set de sets)

Um documento em qualquer língua é uma hierarquia. O Ayvu Rapyta **já é** essa
árvore — só falta generalizá-la e nomeá-la:

```
Corpus            (nomeado: "ayvu-rapyta", "mbya-lexico", "odu-ifa")
└─ Document       (capítulo; ou conjunto de exemplos do léxico)
   └─ Sentence    (verso) — alinhada a traduções { lang → texto }
      └─ Token    (palavra, forma de superfície)         lang
         ├─ Lemma (forma canônica → entrada do léxico)
         └─ Root[] (morfemas/partículas — as "raízes")
```

Cada nó carrega `lang`. **Sentenças** carregam alinhamento entre línguas
(Mbyá ⟷ Español hoje; Português no futuro). **Tokens** ligam ao léxico (lemma)
e às raízes. Mapeamento ao que já temos:

| Conceito do framework | Já existe como |
|---|---|
| Corpus "ayvu-rapyta" | `comunicacao/corpus/ayvu-rapyta.json` (19 caps) |
| Document | `chapter` (n, roman, title) |
| Sentence + alinhamento | `verse` com `gn` (Mbyá) + `es` (Español) |
| Token | `word.w` |
| Lemma | `word.n` (`~lema`) → léxico (YG-134) |
| Root[] | `word.seg` (partículas) |
| Camada de termo (gloss/pron/decomp) | léxico Mbyá 4.837 / Iorubá |

Ou seja: a forma de árvore está pronta; o que falta é o **modelo de corpus
nomeado** e a **álgebra** sobre ele.

## 2. Corpora intercambiáveis + frequência

Um **corpus** é um conjunto nomeado de documentos. O Mbyá começa com um corpus
de **um** documento (a transcrição inteira do Ayvu Rapyta); cresce somando, por
exemplo, todas as frases-exemplo do léxico ("natural language" corpus). O
usuário **troca o corpus** para análise e comparação:

- palavras mais usadas no **Ayvu Rapyta** vs. **corpus Mbyá inteiro** vs.
  **Odu Ifá** (Iorubá).

Primitivo central — **tabela de frequência**: `(corpus, lemma|root, count, rank)`.
Ordenar por `rank` = "por popularidade" (o que a UI já insinua). Trocar o corpus
troca a tabela. (É o `extract_keywords` por frequência do portuNLP, mas por
corpus e por nível — lemma ou raiz.)

## 3. Álgebra de corpora (joins) — o coração da ideia

Existe uma **tabela de links cross-linguísticos**: `(lang_a, termo_a, lang_b,
termo_b, tipo)` — tradução, cognato, raiz comum. Com ela, comparar dois corpora
é literalmente um **join SQL**:

- **inner**: top palavras do Iorubá **só se** têm equivalente em Mbyá (tradução).
- **left/right**: inclui as sem equivalente de um lado.
- **full**: todas.

O resultado de um join **é, ele próprio, um corpus** — nomeável, salvável,
requeríbrel. Daí "esses dois resultados são dois corpora que podem ser salvos e
consultados". A álgebra fecha sobre si mesma.

## 4. Backend — recomendação: **DuckDB embarcado**, com interim em memória

Por que DuckDB encaixa exatamente nesta visão:
- **Embarcável** (sem servidor, como SQLite) → cabe no binário único do deploy.
- **Colunar** → agregação de frequência e os joins do item 3 são o forte dele.
- **SQL nativo** → left/right/inner/full são uma linha; "salvar como corpus" é
  `CREATE TABLE … AS SELECT …`.
- **Parquet** → corpora viram artefatos salváveis; podem migrar para o CO/S3
  quando escalar, sem trocar o modelo.
- Crate Rust `duckdb` existe.

**Interim (sem dependência nova ainda):** os dados são minúsculos (~4.7k tokens
no Ayvu, ~4.8k lemas no léxico). Dá para shipar o modelo em **structs em memória
serializáveis em protobuf** + uma camada fina de frequência/join em Rust, e
**promover para DuckDB** quando os joins/escala justificarem — mesmo padrão do
índice do léxico (derivado, reconstruível do canônico). Canônico continua sendo
o markdown/JSON; DuckDB/protobuf é índice derivado.

**Onde os dados vivem:** local no yggdrasil agora; federáveis ao CO depois
(o bridge já existe) se a escala pedir — exatamente como o usuário previu.

## 5. Plano por fases (épico YG-139)

- **F1 — Modelo + registro de corpora + frequência.** Generalizar a árvore;
  corpora nomeados (ayvu-rapyta, mbya-lexico, odu-ifa); tabela de frequência por
  corpus/nível; API `GET /api/v1/corpus/{nome}/freq?nivel=lemma|root`. UI: lente
  "palavras mais usadas", com seletor de corpus. (Reaproveita dados existentes.)
- **F2 — Links cross-linguísticos + joins + salvar-como-corpus.** Tabela de
  links (semeada de glosas/traduções); `GET …/freq?join=inner|left|full&com=<corpus>`;
  persistir corpus-resultado. UI: comparar dois corpora lado a lado.
- **F3 — DuckDB** (quando escala/consulta justificar): freq/join migram para
  DuckDB + Parquet; opção de federar ao CO.
- **F4 — Módulos "game" (estilo SensorySpeech):** camada pedagógica/exploratória
  sobre o pipeline (ver sentença → paralelos → raízes → sugerir tradução).

## 6. Frase no Ayvu Rapyta, o alvo de UX

Ver um verso e, ao lado: a tradução tradicional (Español), as conexões aos termos
Mbyá e suas raízes, e (futuro) sugestão de Português — sempre **ligando cada
tradução às palavras e às raízes em cada língua**. As peças já existem (YG-134:
verso `es`, lemma→léxico, raízes, concordância); F1–F2 dão a moldura de corpus e
a comparação que faltam.
