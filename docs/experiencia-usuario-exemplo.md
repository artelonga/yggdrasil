# Uma experiência de usuário — exemplo narrado

> Documento de UX por exemplo: uma pessoa real percorrendo o Yggdrasil como ele
> está em produção (v2.14.0, 2026-06-12). Cada cena registra o que ela vê, o que
> ela faz e **qual princípio de design sustenta o momento**. Serve de referência
> para decisões futuras: se uma mudança quebra uma destas cenas, ela quebra a
> experiência.

**Persona:** Marina, ilustradora e mestre de RPG. Guarda ideias em cadernos
físicos espalhados; quer um lugar onde as ideias tenham *forma* — não uma lista,
um mundo. Usa laptop em casa e celular no ônibus.

---

## Cena 1 — Chegada (visitante, zero fricção)

Marina abre `yggdrasil-artelonga.fly.dev`. Sem cadastro, sem paywall: a landing
mostra os universos vivos na barra lateral, placares, estatísticas anônimas.
Ela clica em Snake, joga uma partida, volta. Clica em "Tagmar" por curiosidade
— e em vez de um erro, cai no **catálogo**, onde Tagmar aparece como
🟡 *planejado*, com link "quero portar".

> **Princípio: explorar antes de pertencer.** Tudo que é público funciona
> deslogado, e nenhum clique termina em beco (slugs sem página redirecionam ao
> catálogo — v2.7.1). O convite a criar fica visível, mas nunca bloqueia a
> exploração.

## Cena 2 — Entrar (a conta que ela já tem)

Ela clica "✦ Criar universo". O login pede só o email; o código chega, ela
digita, e volta **direto para a página de criação** — não para um lobby
genérico (`?next=`, v2.8.0). No topo, agora aparece seu email e o botão "Sair".

> **Princípio: o login serve à intenção.** A autenticação (delegada ao CO, um
> request de distância) devolve a pessoa exatamente para onde ela estava indo.
> Estado de sessão é visível em toda página (v2.7.0).

## Cena 3 — Criar (uma decisão, não cinco)

A página de criação oferece **um único modelo: "Universo"** — nota, pasta e
evento (v2.14.0). Nada de escolher entre "jardim", "timeline" ou "em branco":
Marina não precisa prever a forma do que ainda nem escreveu. Ela digita
"Mundos da Marina" e clica criar. Dois segundos depois está dentro do player —
**já em modo edição**, porque um universo vazio com clique morto parece
quebrado (v2.9.0).

> **Princípio: a forma é uma lente, não um compromisso.** O conteúdo é um só;
> Mapa, Timeline e Grafo são jeitos de olhar — escolhidos depois, mudáveis
> sempre (padrão validado no co: view = toggle de runtime).

## Cena 4 — Escrever (rascunho é branch, salvar é commit)

Marina seleciona **📝 Nota** na paleta e clica numa célula: "O Porto de Vidro".
Clica no bloco → "✎ Editar nota" → um **popup** toma a tela: markdown à
esquerda, **preview ao vivo** à direita (v2.11.0). Ela escreve, liga ideias com
`[[a-maré-baixa]]`, e repara no rodapé: *"rascunho guardado 14:32 · nada
publica até salvar"*.

O ônibus chega. Ela fecha o laptop **sem salvar**.

No celular, abre a mesma nota: *"Há um rascunho de outro dispositivo (14:32) —
Continuar / Descartar"* (v2.12.0). Continua, termina o parágrafo, e só então
**💾 Salvar (commit)**. Agora sim a nota é canônica — e federa ao CO pelo
bridge, em menos de um segundo.

> **Princípio: nada se perde, nada vaza.** Digitar nunca toca o canônico
> (rascunho = branch, local + servidor); salvar é o único commit — e é o
> commit que federa. O rascunho vive **fora** do caminho do bridge: privado
> por construção, não por configuração.

## Cena 5 — Compartilhar (link exclusivo sem segredo na URL)

Marina quer mandar a nota para o co-mestre da campanha. Clica **🔗 link** —
`…/universos/instance/<id>#nota=o-porto-de-vidro&editar=1`. O fragmento `#`
não aparece em log de servidor nem em Referer, e não carrega credencial
nenhuma: o co-mestre, sem ser dono, **não consegue editar** (e nem ver, se o
universo for privado). Quando ela abre o próprio link, cai direto no editor.

> **Princípio: o link é endereço, nunca chave.** Exclusividade vem do modelo
> de permissão (JWT, owner-only), não de URLs obscuras que vazam em histórico
> e screenshot.

## Cena 6 — Organizar (pastas que são lugares)

As notas se acumulam. Marina coloca uma **📁 Pasta** ("Cidades costeiras") e
**arrasta** "O Porto de Vidro" para cima dela — toast: *"📁 Movido para dentro
de Cidades costeiras"* (v2.9.0). A ligação dourada pai/filho aparece no mapa;
arrastar uma nota sobre outra cria uma referência ciano; os wikilinks são as
tracejadas roxas. Na legenda lateral ela liga "Irmãos (mesma pasta)" e vê o
parentesco implícito — ser filho do mesmo pai **é** um tipo de ligação.

> **Princípio: hierarquia sem esconder.** Pastas organizam sem virar
> diretórios opacos: tudo continua visível no plano, e cada tipo de relação
> tem cor, estilo e toggle próprios.

## Cena 7 — Mudar de lente (o tempo que já estava lá)

Marina clica **🕐 Timeline** no topo. Sem configurar nada, o universo se
reconta no eixo do tempo: cada nota aparece **no dia em que nasceu**, junto do
"✦ Mundos da Marina criado". Ela arrasta para percorrer as semanas, dá zoom
com o scroll, clica num evento e a nota abre. Volta para **🕸 Grafo**: as
mesmas notas, agora como constelação de wikilinks. **🗺 Mapa** de novo: tudo
onde ela deixou.

> **Princípio: o conteúdo não sabe da forma.** Datas de criação, `at_iso` de
> eventos, wikilinks — tudo já é dado canônico; as lentes apenas o projetam.
> Nenhuma view exige reorganizar nada (e novas lentes — placares, sessões de
> jogo via bridge — terão onde aterrissar sem migração).

## Cena 8 — O que ela não viu (e é o ponto)

Enquanto Marina escrevia, cada nota salva virou um arquivo markdown canônico
no servidor, emitiu um evento assinado pelo bridge e aterrissou no CO
(`co.artelonga.com.br`) como entrada viva do universo federado — pronta para a
era em que editar lá refletirá aqui (YG-124). Ela não configurou nada disso.

> **Princípio: federação é infraestrutura, não tarefa do usuário.** A
> plataforma move o conteúdo; a pessoa só escreve.

---

## Resumo dos princípios (checklist de regressão de UX)

| # | Princípio | Cena | Quebra se… |
|---|---|---|---|
| 1 | Explorar antes de pertencer | 1 | algo público exigir login, ou um clique terminar em 404 |
| 2 | Login serve à intenção | 2 | pós-login não voltar ao destino |
| 3 | Forma é lente, não compromisso | 3, 7 | criação voltar a pedir "tipo" de universo |
| 4 | Rascunho = branch, salvar = commit | 4 | digitar tocar o canônico, ou fechar perder texto |
| 5 | Link é endereço, nunca chave | 5 | credencial/capability aparecer em URL |
| 6 | Hierarquia sem esconder | 6 | pasta virar container opaco |
| 7 | Conteúdo não sabe da forma | 7 | uma view exigir migração/reorganização |
| 8 | Federação é infraestrutura | 8 | o usuário precisar "ativar sync" |
