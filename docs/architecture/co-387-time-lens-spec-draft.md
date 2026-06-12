# CO-387 time-rendering lens — rascunho de spec (aprendizados do YG-123)

> Protótipo: `Projection::Timeline` no Yggdrasil (v2.13.0, 2026-06-12).
> Gerador: `yggdrasil-core/src/instance/generators/timeline.rs` ·
> Renderer: ramo `timeline` em `static/universos/instance.js` ·
> Template seed: `timeline_template()` (céu de 2026).
> Tarefa-irmã no co: **CO-396** (project timeline lens) — mesma engine de layout.

## Contrato de layout (o que deve virar o crate compartilhado)

Duas funções puras, host-agnósticas — hoje vivem no gerador do Yggdrasil e
devem ser extraídas para o crate do CO-396 (canal path-dep do game-core)
quando ele nascer; **a evolução de layout não deve continuar aqui**:

```rust
/// instante → coluna: interpolação linear em [min, max] sobre `width` colunas;
/// intervalo degenerado → coluna central; última coluna alcançável.
fn x_for(at: DateTime<Utc>, min: DateTime<Utc>, max: DateTime<Utc>, width: u32) -> u32;

/// kinds → faixas (lanes) Y estáveis: família = prefixo até o primeiro '.'
/// ("moon.full" → "moon"); ordenação determinística.
fn lane_rows(kinds: &[String]) -> BTreeMap<String, u32>;
```

Política de colisão: mesma (família, coluna) empilha em Y dentro da faixa
(altura de faixa fixa; excedente satura na última linha). Suficiente para
densidades do universo `time`; para audit logs densos o crate deve ganhar
agregação ("n eventos" num bloco expansível) — primeiro aprendizado real.

## Aprendizados de render (válidos para a lens nativa do co)

1. **Quantizar tempo→colunas no gerador** (não no renderer) deixou o renderer
   trivial: o pipeline de grade existente (blocos/arestas/hit-test) funcionou
   sem fork — só o `cellToScreen` precisou de `x * pitch * scale + off`.
2. **Pan/zoom só no X, sem `ctx.scale`**: escalar o *pitch* das colunas (e não
   o canvas) evita distorcer blocos/texto e mantém o hit-test exato com a
   transformação inversa em `screenToCell`.
3. **Uma layer por família de `kind`** dá filtro/toggle de graça em qualquer
   host que já tenha camadas (o co tem). `moon.*` na mesma faixa funcionou
   melhor que uma faixa por kind completo.
4. **Eixo no rodapé com ticks interpolados** pela MESMA régua do gerador
   (col 0..width-1 ↔ min..max) — qualquer outra régua desalinha rótulo e bloco.
5. **`at_iso`/`kind` ficam canônicos em `props`** do bloco; posição é projeção
   derivada e regenerável — espelha o princípio conteúdo×forma do ecossistema.

## Adendo YG-126 (2026-06-12): timeline virou LENTE de runtime

Validado o padrão do co (`state.view` em views.js): a timeline deixou de ser
projeção fixa e virou view alternável (Mapa | Timeline | Grafo) sobre QUALQUER
universo. A cena é derivada client-side por um **espelho JS** da mesma régua
(`tlScene()` em instance.js): blocos com `props.at_iso` + criação de notas
(fallback created_at, como o co faz com tasks) + criação do universo. O crate
compartilhado (CO-396) deve portanto nascer com dupla face: Rust (gerador) e
contrato espelhável em JS — ou WASM único consumido pelos dois renderers.

## O que falta para a lens co-compatível

- Crate compartilhado com `x_for`/`lane_rows` + política de colisão/agregação.
- Escalas não-lineares (log para telemetria; "fisheye" no cursor).
- Ticks calendáricos (mês/semana exatos) em vez de frações lineares.
- Inbound P-B: entradas do universo `time` fluindo do co pelo bridge para uma
  instância timeline viva (hoje o seed é estático no template).
