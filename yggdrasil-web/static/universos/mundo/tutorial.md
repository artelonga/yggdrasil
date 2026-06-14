# Guia do Mundo

Conteúdo determinístico do NPC (YG-146). Cada `##` é um tópico/missão; o NPC
monta o menu a partir daqui e, sem LLM, responde casando palavras-chave da
pergunta com estes tópicos. Com Ollama, isto vira o *system prompt* (contexto +
tutoriais) e o LLM responde livre — com fallback pra cá.

## Como ando e entro em salas?
Use **WASD** ou as **setas** pra andar — **segure** pra acelerar, e **duas teclas
juntas** (ex.: W+D) andam na **diagonal**. Ou **clique** num tile que eu vou até
lá quase na hora. Pra **entrar numa sala**, pise na **porta** (cada pasta é uma
sala). Pra **voltar**, pise no tile **↑ voltar**.

## Como interajo com notas?
Pise no **objeto** da nota (ou aperte **Enter** em cima dela) — abre o painel com
o conteúdo. Lá você pode **✏️ editar** ou **🗑 excluir**. Também dá pra **clicar no
item no menu** (à direita) pra abrir a nota direto, sem caminhar até ela.

## O que é uma sala ou pasta?
Cada **pasta é uma sala** que você entra de verdade. As notas dentro dela são
objetos no chão; outras pastas são portas pra salas mais fundas. É a **hierarquia**
do seu universo — navegável como um mundo, e espelhada na **árvore** ao lado.

## Como organizo (arrastar e soltar)?
**Arraste** uma nota (segura o clique e move) pra **reposicioná-la** onde quiser —
a posição é o "estado" da sala. Se você **soltar a nota sobre uma porta**, ela é
**movida pra dentro daquela pasta** (reparent), e a **árvore atualiza na hora**.
Criar uma **+ nova sala** (botão no topo) também aparece na árvore imediatamente.

## Como edito o texto de uma nota?
No editor, a dica embaixo mostra os atalhos (e tem variantes A/B com `‹ ›`):
normalmente **Enter salva** e **Shift+Enter quebra linha** — ou o inverso. Tem
sempre um botão **💾 Salvar**. No produto, ao confirmar, a mudança vira um commit
(em lote) no `.md` canônico e replica pro CO.

## O que muda nos temas?
O tema muda **forma e arte**, não só cor: medieval (castelo/taverna), jardim
(floresta/zen) e moderno têm chão, paredes, portas e personagens próprios. Troque
no seletor no topo — estamos mantendo os 5 pra você escolher pela experiência.

## Quem é você (NPC)?
Sou o **Guia**. Tenho estes tutoriais sempre à mão e, quando o **LLM local
(Ollama)** estiver ligado, respondo perguntas livres usando o contexto da sala
em que você está. Sem o LLM, caso na resposta determinística destes tópicos.
