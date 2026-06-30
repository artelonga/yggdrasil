# Coevolve — design & foundations

> **Status:** design (pré-código). Decisão do dono (2026-06-30): escrever o doc antes
> de construir; a evolução **cultural** (linguagem, escrita, habilidades) é **fiada aos
> universos de conhecimento reais** (topologia/comunicação). Sucede o protótipo dino
> (YG-180), que vira uma fatia inicial de "organismo navega o mundo".

## 1. Visão

**Coevolve** — *evoluímos com pares*. Um jogo que modela a **evolução** biológica E
cultural: da química replicadora à célula, à multicelularidade, à sociedade, à
**inteligência**, e às **habilidades** (fazer fogo, linguagem, escrita). "Spore, mas
academicamente preciso." O mundo não é cenário — é **informativo**: navegá-lo é
aprender, e a cultura que sua linhagem adquire vem de **corpora reais**.

Dois eixos, um princípio:
- **Coevolução**: espécie↔espécie, gene↔cultura, e **jogador↔jogador** (pares evoluem
  no mesmo mundo, competindo e cooperando).
- **Precisão acadêmica = dado real** (o princípio fundador do Yggdrasil, [[feedback_no_fabricated_data]]):
  nada de tech-tree inventado; a evolução cultural roda sobre o léxico/corpus reais.

## 2. A fundação NÃO é o renderer — é a simulação

A parte durável e difícil é o **modelo**, não o desenho. Logo:

```
┌─ yggdrasil-core::coevolve  (Rust) ── SIMULAÇÃO autoritativa, determinística, testável
│    genoma→fenótipo · mutação/herança · seleção/ecologia · especiação ·
│    transições maiores · camada cultural (memes) ── SEM render, SEM engine
│
├─ yggdrasil-web  (HTTP/WS) ── servidor: estado do mundo, ticks, "com pares" (multiplayer
│    server-authoritative, espelha o bridge CO↔Ygg existente)
│
└─ CLIENTE (renderer) ── desenha + envia intenções. Troca sem tocar no core:
     • Web/Three.js (cedo: testável no navegador, embute no lobby)   ← protótipo dino
     • Godot (depois: cliente rico, export web/WASM p/ o lobby; alinhado ao game-core
       e à POC Godot YG-31..35)
```

**Godot vs web não é a decisão de fundação** — o core Rust é. O cliente é trocável; a
simulação é a espinha. (O dino em Three.js **não** é portável a Godot — código não
migra — mas o *design* e o *core* sim.)

## 3. Modelo de domínio (a simulação)

### 3.1 Herança genética (síntese moderna)
- **Genoma**: vetor de genes — contínuos (tamanho, velocidade, metabolismo) e
  discretos (presença de traços/órgãos). **Fenótipo** = expressão do genoma + ambiente.
- **Variação**: mutação (deriva nos genes) + recombinação (reprodução sexuada).
- **Herança**: descendentes herdam genoma dos pais ± mutação. **Descendência com
  modificação** (Darwin) é o mecanismo central, não um tech-tree.

### 3.2 Seleção & ecologia
- **Ambiente** (o mundo toroidal informativo): recursos, clima, nichos por região.
- **Fitness emergente**: sobreviver + reproduzir; predação, competição, mutualismo.
- **Especiação**: isolamento (regiões do mundo) → divergência → isolamento reprodutivo.
  Filogenia = árvore de linhagens com ancestral comum (real-ish, calibrável).

### 3.3 Transições maiores (o backbone acadêmico)
A espinha de "modelar a evolução **inteira**" é o arcabouço de **Maynard Smith &
Szathmáry, *The Major Transitions in Evolution* (1995)**:
replicadores → compartimentos (células) → cromossomos → eucariotos →
**multicelularidade** → colônias/**sociedade** → **linguagem/cultura**.
Cada transição muda *como a informação é herdada* — é exatamente o gancho de Coevolve.

### 3.4 Evolução cultural (herança dupla)
Inteligência e habilidades **não** são genes — são um **segundo sistema de herança**
(**dual inheritance theory**: Boyd & Richerson 1985; Cavalli-Sforza & Feldman 1981).
- **Memes/habilidades** com descendência própria: aprendidas, transmitidas, variadas,
  selecionadas. Fogo, ferramentas, linguagem, escrita são *traços culturais*.
- **Coevolução gene-cultura**: fogo/cozinhar (Wrangham 2009) muda dieta→biologia;
  linguagem muda cognição (Tomasello 1999).
- Timeline real como calibração: ferramentas Oldowan ~2.6 Ma · controle do fogo
  ~1–1.5 Ma · modernidade comportamental ~50–100 ka · agricultura ~12 ka ·
  **escrita** ~3400–3200 AEC (cuneiforme; Schmandt-Besserat 1992).

## 4. O mundo informativo = universos de conhecimento

O **mundo é navegável conhecimento**, e a evolução cultural roda sobre **corpora reais**
(decisão do dono). Concretamente:
- **Linguagem** como traço cultural: a linhagem "adquire" termos do **léxico real**
  (topologia/comunicação — `LexiconLoader`). Aprender uma palavra = um meme adquirido;
  a topologia de sentido é o espaço de aquisição (a aprendizagem por contexto já feita
  na YG-176/179). Iorubá/Mbyá/Espanhol = lentes/culturas distintas.
- **Escrita** desbloqueia ao acessar o **corpus** (Ayvu Rapytã; fontes/biblio).
- **Habilidades/tecnologias** ancoradas nas **`sources`** (bibliografia real) — cada
  avanço aponta para uma referência acadêmica (fogo, ferramentas…), não fluff.
- Assim o "informativo" é literal: o mapa são os universos; progredir culturalmente é
  percorrer conhecimento real. É o que torna o jogo *academicamente preciso*.

## 5. Loop central

Nasça como organismo → sobreviva, reproduza (passe genes ± mutação) → a seleção poda →
ao longo de **gerações** sua linhagem evolui → atinja **transições** (multicelular →
… → inteligência → cultura) → no estágio cultural, **navegue o mundo informativo** para
adquirir linguagem/escrita/habilidades reais. **Com pares**: mundo compartilhado,
linhagens coevoluindo (predador/presa, simbiose, competição, ensino).

Fases estilo Spore **mapeadas às transições maiores reais** (não inventadas):
célula → multicelular → social → sapiente → cultural.

## 6. Plano por fases (incremental, cada uma testável)

| Fase | Entrega | Onde | Testável por mim? |
|---|---|---|---|
| **P0** | Core: genoma, mutação, herança, fitness, 1 passo de geração | `yggdrasil-core::coevolve` | ✅ testes Rust |
| **P1** | População + seleção por gerações (linhagem evolui) | core + viz simples | ✅ headless + web |
| **P2** | Mundo/ecologia (toroidal informativo, recursos, nichos, especiação) | core + web | ✅ |
| **P3** | Transições maiores (arcabouço Maynard Smith) | core | ✅ |
| **P4** | Camada cultural fiada aos universos (linguagem/escrita do corpus real) | core + `LexiconLoader` | ✅ |
| **P5** | Pares/multiplayer (server-authoritative; coevolução entre jogadores) | web (HTTP/WS) | ✅ |
| **P6** | Cliente rico Godot (export web p/ lobby) | Godot | ⚠️ você verifica local |

O protótipo **dino (YG-180)** entra como teste de *feel* de P1/P2 (organismo navega o
mundo toroidal) — descartável/absorvível; Coevolve **não** cresce dentro dele.

## 7. Decisões em aberto
- **Engine final do cliente** (Godot recomendado p/ a ambição; web cedo p/ iterar/testar).
- **Modelo de multiplayer** (autoritativo no servidor; granularidade de tick).
- **Escala de tempo**: mapeamento tempo-de-jogo ↔ tempo evolutivo real.
- **Guarda-corpos de escopo**: a ambição é enorme — tudo modular e incremental, uma
  transição por vez.

## 8. Fundamentação acadêmica (bibliografia)
- Darwin, C. (1859). *On the Origin of Species.*
- Maynard Smith, J. & Szathmáry, E. (1995). *The Major Transitions in Evolution.* OUP.
- Dawkins, R. (1976). *The Selfish Gene*; (1982) *The Extended Phenotype.*
- Boyd, R. & Richerson, P. J. (1985). *Culture and the Evolutionary Process*; (2005)
  *Not by Genes Alone.*
- Cavalli-Sforza, L. L. & Feldman, M. W. (1981). *Cultural Transmission and Evolution.*
- Tomasello, M. (1999). *The Cultural Origins of Human Cognition.*
- Wrangham, R. (2009). *Catching Fire: How Cooking Made Us Human.*
- Schmandt-Besserat, D. (1992). *Before Writing.*
- (Linguagem/sentido: reaproveita a bibliografia da topologia — Cadogan 1959 Ayvu
  Rapytã; Dooley 2006; fontes Yoruba/Ifá — ver `docs/architecture/yoruba-ifa-references.md`.)

> Precisão acadêmica = estes arcabouços + dado real. Onde simplificarmos por jogabilidade,
> **registrar a simplificação** (mesmo princípio do "sem dado fabricado").
