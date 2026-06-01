# Atribuição de assets — template neuroanatomia (YG-80)

O template `neuroanatomia` é **data-driven**: as imagens (silhueta do corpo,
overlay do SNC) e os anexos dos landmarks são carregados em runtime pelo usuário,
não embutidos no binário. Cada anexo é referenciado por um `ContentRef`
content-addressed (SHA-256) e pode carregar metadados de licença/atribuição.

## Modelo de atribuição

A licença/fonte de cada asset é gravada e exibida em dois lugares:

- **`ContentRef`** / **`Block.props.attribution`** — string livre com fonte +
  licença (ex. `"OpenStax A&P 2e — CC-BY 4.0"`). O player web
  (`static/universos/instance.js`) exibe esse crédito no inspetor ao abrir o anexo.
- **`render_hints.attribution_required: true`** no template — sinaliza ao editor
  que assets não-CC0 exigem preenchimento do crédito.

## Fontes recomendadas (ver `docs/architecture/editor.md` para detalhes)

| Fonte | Licença | Uso |
|---|---|---|
| FreeSVG.org · SVG Silh | **CC0** | Silhueta/contornos — sem atribuição obrigatória (começar por aqui) |
| OpenStax Anatomy & Physiology 2e | CC-BY 4.0 | Figuras rotuladas do sistema nervoso — **crédito obrigatório** |
| Wikimedia Commons (SVG human anatomy) | PD / CC-BY-SA | Verificar licença por arquivo |
| SlicerDMRI / IIT Human Brain Atlas | ver projeto | Tratos/conexões (fase posterior) |

## Placeholders embutidos (CC0)

O template `neuroanatomia` já vem com dois SVGs **originais** (autoria própria,
**CC0** — sem atribuição obrigatória) semeados na criação da instância, para o
toggle de transparência ser significativo antes de qualquer upload:

- `yggdrasil-web/static/universos/assets/neuroanatomia/corpo.svg` — silhueta.
- `yggdrasil-web/static/universos/assets/neuroanatomia/snc.svg` — encéfalo + medula.

São esquemáticos (não anatomicamente precisos) e foram feitos para serem
substituídos por assets reais (OpenStax/Wikimedia/atlas) via upload, com o
crédito preenchido no `ContentRef`/`props`.

## Regra

Nenhum asset sem licença clara entra no repositório. Assets CC0 podem ser
incluídos diretamente; assets CC-BY/CC-BY-SA exigem o campo de atribuição
preenchido no `ContentRef`/`props` antes de publicar a instância.

## Atlas de núcleos (viewer Neuroglancer, /neuro) — APENAS fontes redistribuíveis

> Revisão de licenças para **distribuição pública** (2026-06-01). Só entram em
> `/neuro` atlas cuja licença permite redistribuir derivados publicamente.

- **Harvard-Oxford subcortical atlas** (`sub-maxprob-thr25-1mm`), via `nilearn`
  (Harvard CMA / Makris et al.). Labelmap NIfTI → Precomputed (cloud-volume+igneous).
- **Licença: CC BY-SA 4.0.** O atlas Harvard-Oxford, embora distribuído junto do
  FSL, **não** está sob a licença FMRIB restritiva — a própria página de licença
  do FSL declara que "the Cerebellum and Harvard-Oxford atlases … are released
  under the CC BY-SA 4.0 licence". → redistribuição pública permitida **com
  atribuição** e **ShareAlike**.
- **ShareAlike**: nossos derivados (`static/neuro-data/ho-sub`) ficam sob
  **CC BY-SA 4.0**. Crédito exibido no header de `/neuro`.

## Atlas de núcleos do tronco (camada aan-brainstem)

- **Harvard Ascending Arousal Network (AAN) Atlas v2.0** — núcleos do tronco
  encefálico (DR, MnR, PAG, VTA, LC, LDTg, mRt, PBC, PnO, PTg; L/R).
- **Licença: CC0** (domínio público) — via Zenodo (record 8161638).
- MNI152 1mm → alinha com `ho-sub`. Labelmap NIfTI → Precomputed
  (`scripts/convert_atlas.py`).

## ❌ EXCLUÍDO — Brainstem Navigator (NITRC)

- **Brainstem Navigator v1.0** (Bianciardi lab / Massachusetts General Hospital,
  via NITRC) — atlas in-vivo 7T, 76 máscaras de núcleos do tronco.
- **NÃO redistribuível.** O `Copyright.txt` do atlas é explícito:
  - cláusula 1: "use … without charge for **non-commercial research purposes
    only**";
  - cláusula 2: "**YOU MAY NOT DISTRIBUTE COPIES** of the Brainstem Navigator
    files, **or copies of files or of information derived from them, to others
    outside your organization**".
- Servir os derivados (labelmap/meshes Precomputed) publicamente em `/neuro`
  violaria a cláusula 2. **Removido** da camada `bsn-brainstem` em 2026-06-01.
- O zip baixado fica de fora do repo (gitignored); uso permitido apenas para
  pesquisa não-comercial local, não para o site público.
