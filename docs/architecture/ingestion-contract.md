# Ingestion contract — o esquema canônico que todo texto emite

> Contrato da Fase A do `docs/text-ingestion-roadmap.md`. Todo adaptador de fonte
> (dicionário, corpus) emite **estes** artefatos; o `yggdrasil` consome só isto.
> Mbyá é o adaptador de referência (já produz tudo abaixo).

## Artefato 1 — Léxico  `comunicacao/<lang>/lexicon.<code>.json`

Lista JSON ordenada por **popularidade desc** (`pop`). Uma entrada por termo.
Lido por `yggdrasil_core::comunicacao::public` (`LexEntry`).

```jsonc
{
  "word":  "àṣẹ",          // o termo, COM tom/diacrítico (display)
  "lang":  "yo",            // código de língua (gn-mbya | yo | …)
  "gloss": "força vital; …",// glosa/definição PT (ou da língua-ponte)
  "pron":  "à.ʃɛ́",          // pronúncia/IPA (opcional)
  "pop":   23,              // rank: nº de exemplos no corpus (ou proxy de riqueza)
  "decomp":"a 'x' + ṣẹ 'y'",// decomposição morfêmica (opcional)
  "examples": [             // opcional; alimenta o overlay léxico (se presente)
    {"gn":"<na língua>", "pt":"<tradução>"}
  ]
}
```
Campos extras são ignorados (serde tolerante). `pron`/`decomp`/`examples` opcionais.

## Artefato 2 — Corpus  `comunicacao/corpus/<work>.json`

Texto canônico hierárquico (gêmeo do Ayvu Rapytã), com **alinhamento token↔termo**.

```jsonc
{
  "work":  {"slug":"ayvu-rapyta","title":"…","author":"…","year":1959},
  "chapters": [
    {"n":1, "title":"…",
     "verses":[
       {"lang":"gn-mbya","verse":6,"text":"A'e va'e rakygue…",
        "tokens":[{"form":"va'e","entry":"gn-mbya:va-e"}]}  // entry = NodeId 1:1
     ]}
  ],
  "alignments": [{"a":"<verso gn>","b":"<verso es>"}],   // bitext (opcional)
  "notes":      ["comentário etnográfico/etimológico…"]   // opcional
}
```
`tokens[].entry` é o `NodeId` (`{lang}:{slug}`) do léxico — a ligação 1:1 que faz o
termo ter **instâncias reais** e habilita o overlay de **co-ocorrência** (corpus).

## Como o `yggdrasil` consome
- **Nós** = `lexicon.<code>.json` (posição por espiral de phyllotaxis, rank=`pop`).
- **Contexto léxico** = `gloss + decomp + examples`.
- **Contexto corpus** = co-ocorrência via `tokens[].entry` nos versos.
- **Instâncias** (no inspector) = versos (com cap·verso) + examples.
- Hoje o Mbyá vem do `mbya_lexicon.db`; o contrato **abstrai a fonte** — qualquer
  adaptador que emita estes JSONs entra no grafo sem mudar o consumidor.

## Invariantes
Sem dado fabricado · `word` mantém tom/diacrítico (a dobra é no `slugify` a jusante)
· `entry` sempre um NodeId resolvível · fonte declara manifesto + consentimento
(ver `scripts/ingest/` + `_sources.yaml`) antes de emitir.
