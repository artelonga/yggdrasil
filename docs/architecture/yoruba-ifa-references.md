# Yoruba — Odù Ifá (corpus) + léxico: referências e bibliografia

> Fontes verificadas (deep-research 2026-06-28) para a lane Yoruba da topologia.
> O corpus canônico Yoruba é o **Odù Ifá** — **não** a Bíblia (texto colonial/cristão,
> não o cânone iorubá). Análogo direto do Ayvu Rapytã para o Mbyá.

## 1. Estrutura do Odù Ifá (o que são os "termos")

- O corpus de Ifá tem **256 odù**, cada um subdividido em versos chamados **ẹsẹ**
  (número aberto, centenas por odù; cresce continuamente). [UNESCO]
- 256 = **16 Olódù** (principais / *Méjì* / Oju Odù, auto-combinações) **+ 240 Ọmọ
  Odù** (filhos), formados combinando os 16 principais 16×16. [Wikipedia: Odu Ifa;
  ScienceDirect 2023]
- Revelado (na crença iorubá) por **Ọ̀rúnmìlà** (= Ifá), divindade da sabedoria;
  interpretado pelo **babaláwo** (sacerdote de Ifá). Cada odù tem uma assinatura de
  adivinhação. [UNESCO]
- **Ifá** é Obra-Prima do Patrimônio Oral e Imaterial (UNESCO, 2005) e está na Lista
  Representativa (2008). [UNESCO ich.unesco.org/en/RL/ifa-divination-system-00146]

**Os 16 Olódù principais** (ortografia com tom/diacrítico; ordem canônica conforme
Abimbola 1976 / Bascom 1969):

1. Èjì Ogbè  2. Ọ̀yẹ̀kú Méjì  3. Ìwòrì Méjì  4. Òdí Méjì  5. Ìrosùn Méjì
6. Ọ̀wọ́nrín Méjì  7. Ọ̀bàrà Méjì  8. Ọ̀kànràn Méjì  9. Ògúndá Méjì  10. Ọ̀sá Méjì
11. Ìká Méjì  12. Òtúrúpọ̀n Méjì  13. Òtúrá Méjì  14. Ìrẹtẹ̀ Méjì  15. Òsé Méjì
16. Òfún Méjì

> Os "termos" do corpus, no nosso esquema, seriam: os **16 Olódù** (+ 240 Ọmọ Odù)
> como nós estruturais, e o **vocabulário dos ẹsẹ** (nomes de orixás, refrães,
> fórmulas) como o léxico vivo — mas **enumerá-los exige o texto**, que não temos
> aberto (ver §2). Epítetos em inglês ("The Supporter" etc.) são interpretativos,
> **não** scholarly — não usar como glosa.

## 2. Existe Odù Ifá digitalizado aberto? — NÃO

**Nenhum corpus Yorùbá de Ifá / ẹsẹ Ifá abertamente licenciado (CC / domínio
público) existe.** As edições autoritativas contêm o texto Yorùbá original mas são
**sob copyright**; as cópias no Internet Archive são **controlled digital lending**
(empréstimo, `inlibrary`/`printdisabled` — sem download público, sem licença CC).

→ Consequência: **não se pode ingerir o Odù Ifá abertamente.** Além do copyright, é
**conhecimento sagrado vivo** → governança das custódias (babaláwo) sob os
**CARE Principles** (as custódias definem acesso/uso, não nós). O gate de soberania
(`scripts/ingest/source-gate.py`) mantém `ifa-odu` **BLOCK**. Caminho real:
relação + consentimento das custódias, e/ou uma edição licenciável.

## 3. Bibliografia — corpus de Ifá

- **Abimbola, Wande.** *Ìjìnlẹ̀ Ohùn Ẹnu Ifá: Apá Kìíní* / *Sixteen Great Poems of
  Ifá*. UNESCO (Niamey), 1975. **Bilíngue (Yorùbá + inglês)** + anotação sacerdotal;
  16 poemas (um por Olódù) — subconjunto curado, não os 256. [archive.org:
  sixteen-great-poems-of-ifa-wande-abimbola — cópia *não* aberta]
- **Abimbola, Wande.** *Ifá: An Exposition of Ifá Literary Corpus*. Ibadan: Oxford
  University Press, 1976. **Bilíngue**; texto Yorùbá original. *Em copyright.*
- **Abimbola, Wande.** *Ifa Divination Poetry*. New York: NOK Publishers, 1977.
  ISBN 9780883570234 (x+170 pp). *Em copyright.*
- **Bascom, William R.** *Ifa Divination: Communication Between Gods and Men in West
  Africa*. Bloomington: Indiana University Press, 1969 (~575 pp; ISBN 0253328900 /
  reissue 9780253328908). **Bilíngue** — Parte II "The verses of Ifa" traz Yorùbá +
  tradução (a coleção publicada mais completa de ẹsẹ recitados). *Em copyright (CDL).*
- **Bascom, William R.** *Sixteen Cowries: Yoruba Divination from Africa to the New
  World*. Bloomington: Indiana University Press, 1980. *Em copyright (CDL).*
- **Maupoil, Bernard.** *La Géomancie à l'ancienne Côte des Esclaves*. Paris:
  Institut d'Ethnologie (Travaux et Mémoires 42), 1943; reed. 1981; 3ª ed. 1988.
- (a verificar / complementar:) Epega & Neimark, *The Sacred Ifa Oracle* (1995);
  Karenga, *Odù Ifá: The Ethical Teachings* (1999); Verger, *Notes sur le culte des
  Orisa*; Salami; McClelland, *The Cult of Ifá among the Yoruba*.

## 4. Léxico Yorùbá — proveniência e bibliografia

- **O que usamos hoje:** `comunicacao/yoruba/lexicon.yo.json` vem do **kaikki.org**,
  que é o **Wiktionary inglês extraído via `wiktextract`** (base Wiktionary
  CC BY-SA / CC0; extração automática, *crowd-sourced e irregular*). É **aberto** mas
  **não é dicionário scholarly** — bom para arrancar, fraco como autoridade.
  [kaikki.org/dictionary/Yoruba]
- **Alternativas scholarly (restritas, não-CC):**
  - **Abraham, R.C.** *Dictionary of Modern Yoruba*. London: University of London
    Press, 1958 (~xli+776 pp; Yorùbá + inglês, com tom). *Em copyright; IA = CDL.*
  - **Awoyale, Yiwola.** *Global Yoruba Lexical Database v.1.0* (LDC2008L03).
    Philadelphia: Linguistic Data Consortium, 2008. ISBN 1-58563-500-6. **>450 mil
    palavras** (Yorùbá-inglês 142.389 + inglês-Yorùbá 226.585 + Gullah/Lucumí/
    Trinidad), Toolbox/XML, **decomposição morfêmica**. *Licença LDC (paga), não CC.*
    [catalog.ldc.upenn.edu/LDC2008L03]
  - **Crowther, Samuel Ajayi.** *A Vocabulary of the Yoruba Language* (1843) /
    *A Dictionary of the Yoruba Language* (CMS). Domínio público (ortografia antiga).

## 5. Recomendação para a pipeline

1. **Léxico:** seguir com **kaikki** (aberto) como base; marcar a proveniência
   honestamente; buscar acesso ao **Awoyale/LDC** (decomp morfêmica = nosso `decomp`)
   se houver orçamento; usar **Crowther** (domínio público) como ponte histórica.
2. **Corpus (Odù Ifá):** **não ingerir** até (a) consentimento das custódias e
   (b) uma edição licenciável. Documentar a estrutura (256 odù / 16 Olódù) sem
   reproduzir os ẹsẹ. O gate permanece `BLOCK`.

## Fontes (links)
- UNESCO Ifá: https://ich.unesco.org/en/RL/ifa-divination-system-00146
- Odu Ifa (estrutura): https://en.wikipedia.org/wiki/Odu_Ifa
- kaikki Yorùbá: https://kaikki.org/dictionary/Yoruba/
- LDC2008L03: https://catalog.ldc.upenn.edu/LDC2008L03
- Bascom 1969 (IA, CDL): https://archive.org/details/ifadivinationcom0000basc_h9f3
- Abraham 1958 (IA, CDL): https://archive.org/details/dictionaryofmode0000abra
