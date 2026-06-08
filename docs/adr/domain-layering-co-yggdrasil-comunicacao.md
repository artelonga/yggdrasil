# ADR — Domain layering: co (hub) / yggdrasil (platform) / comunicacao (universe)

- **Status:** Proposto (2026-06-08)
- **Contexto:** descoberto ao integrar a wave Phase-2 — a superfície do corpus (YG-111) é
  policy de *conteúdo* (comunicação) vivendo dentro do *core* da plataforma (yggdrasil), e o
  Caderno (YG-112) foi escrito como comunicação-específico quando é uma primitiva genérica.
- **Norte:** [[recursive-universe-architecture]] — sub-universo ⇄ universo ⇄ unidade-deployável
  deve ser **promoção sem re-plumbing**.

## Princípio

> **Separe *mecanismo* (plataforma) de *policy + conteúdo* (universo). Cada camada depende só
> para baixo, por uma interface estável. Conteúdo sobe por valor (payload opaco); forma pertence
> à camada que define o mecanismo.**

| Camada | Conteúdo | Forma / mecanismo | Interface p/ baixo |
|---|---|---|---|
| **co** (hub) | entries federadas (vault), grafo cross-universe, op-log | bus hub, spine de sync/merge, hosting — **agnóstico de universo** (vê `universe_key`, não "comunicacao") | `FederatedEvent` / vault API |
| **yggdrasil** (plataforma) | instance store, stores genéricos | **primitivas genéricas reusáveis**: `NoteStore`, camada de anotação por-usuário (Caderno generalizado), motor SRS/revisão, leitor de conteúdo baked, write-back git, **o producer de federação**, host WASM | universe SDK (ABI WASM) + convenção de content-dir |
| **comunicacao** (universo) | lexicon/corpus/SRD (JSON+MD, no repo `comunicacao`) | **policy de domínio**: curadoria (stub→reviewed), UI de exploração do corpus, planes de língua, semântica de sala | compõe as primitivas do yggdrasil |

## Decisões

1. **Conteúdo já está no lugar certo.** Lexicon/corpus/SRD vivem no repo `comunicacao`
   (`COMUNICACAO_DIR`); yggdrasil os lê genericamente (`corpus_json`, `lexicon_slice`). Manter.
   co os recebe como `universe_key=comunicacao` — sem conhecer o domínio.

2. **Caderno é uma primitiva genérica da plataforma, não de comunicação.** É uma camada de
   anotação/progresso por-usuário sobre *qualquer* conteúdo de universo (favoritar, anotar,
   retomar). Mora em yggdrasil (plataforma), parametrizada por universo; comunicação é o
   **primeiro consumidor**. As *notas* do Caderno são `NoteStore` sob a instância do universo →
   federam pelo producer genérico (path instance-qualified). Refactor incremental: o
   `CadernoStore` (PR #37) sobe de `comunicacao/` para um módulo de plataforma quando um 2º
   consumidor aparecer; até lá fica isolado atrás de uma interface estreita.

3. **A exploração do corpus (`corpus.js`/`corpus.html`) é forma do universo `comunicacao`.**
   É a UI daquele universo, hospedada pelo yggdrasil. Commitar como a superfície de YG-111
   (hoje é trabalho local não-commitado, em risco). Long-term: asset servido via universe-hosting.

4. **Curadoria, SRS e planes de língua são policy de comunicação.** Já estão em
   `yggdrasil-core/src/comunicacao/` — aceitável como *o módulo daquele universo dentro da
   plataforma*. A fronteira a preservar: esse módulo só pode **consumir** primitivas genéricas
   (NoteStore, producer, baked-reader), nunca o contrário (a plataforma nunca importa policy de
   comunicação). Isso mantém comunicação extraível.

5. **A regra de dependência (o que torna a unidade composável):** `comunicacao` → depende de →
   `yggdrasil` (primitivas) → federa para → `co`. Nunca o inverso. co não conhece comunicação;
   yggdrasil não hard-coda conteúdo de comunicação na plataforma; comunicação não reimplementa
   mecanismo. Promoção (sala → universo → deployável) = mover conteúdo + apontar o producer,
   sem reescrever forma.

## Aplicação imediata (a wave Phase-2)

- **YG-111 (corpus surface):** commitar o trabalho local (leitor backend + `corpus.html` +
  `corpus.js` 329-linhas) como a forma do universo comunicação. De-risca + vira a base.
- **YG-112 (Caderno, PR #37):** aceitar o backend (genérico-ish, isolado); **reconciliar** os
  hooks de Caderno do `corpus.js` do agente (167 linhas, só-Caderno) **dentro** do `corpus.js`
  real de exploração — não substituir. Marcar o `CadernoStore` como candidato a subir p/
  plataforma.
- **YG-114 (federation):** já correto — notas do Caderno via `NoteStore` sob `ayvu-rapyta`,
  federadas pelo producer genérico.

## Consequências

- Ganho: comunicação vira uma unidade extraível/promovível; yggdrasil fica reusável p/ outros
  universos de conteúdo (Shandara/YG-69, futuros); co permanece agnóstico.
- Custo: um refactor incremental (Caderno → plataforma) quando houver 2º consumidor; disciplina
  de import (a plataforma nunca importa policy de universo).
- Não-objetivo agora: extrair comunicação p/ um crate/WASM separado — só **preparar a fronteira**.
