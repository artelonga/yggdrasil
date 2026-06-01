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

## Atlas de núcleos (viewer Neuroglancer, /neuro)

- **Harvard-Oxford subcortical atlas** (`sub-maxprob-thr25-1mm`), via `nilearn`
  (FSL / FMRIB, Harvard CMA). Labelmap NIfTI → Precomputed (cloud-volume+igneous).
- ⚠ **Licença**: distribuído com o FSL sob a licença FMRIB — uso/redistribuição
  têm condições (academia/pesquisa). Revisar antes de uso comercial público.
