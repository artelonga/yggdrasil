# Dino — assets (open-source, drop-in)

O jogo (`/universos/dino`) carrega **`dino.glb`** deste diretório se existir; senão
cai num **placeholder procedural low-poly**. Para usar modelos reais, coloque um
glTF aqui como `dino.glb` (mesma espécie para jogador e NPCs — luta justa).

## Fontes open-source recomendadas (livres p/ uso comercial)

- **Quaternius — "Ultimate Animated Dinosaur Pack"** — **CC0** (domínio público),
  glTF/FBX animados. https://quaternius.com  (espelho: Poly Pizza https://poly.pizza)
- **Poly Pizza** — modelos CC0/CC-BY de baixa poligonagem. https://poly.pizza
- **Kenney** — assets CC0 (estilo low-poly). https://kenney.nl
- **Sketchfab** — filtrar por licença CC0/CC-BY (atribuir quando CC-BY).

> Verifique a licença antes de commitar. Para CC-BY, registre a atribuição neste
> README. CC0 não exige atribuição (mas registre a fonte mesmo assim).

## Como integrar

1. Baixe um modelo de dino (glTF binário `.glb` preferido; ou `.gltf` + texturas).
2. Salve como `dino.glb` neste diretório.
3. Recarregue a página — o `GLTFLoader` (em `dino.js`, `tryLoadModel()`) usa o modelo
   real automaticamente; sem alterar código.

> Animações por skeleton (andar/atacar) e modelos por espécie ficam para uma
> próxima fatia — hoje o placeholder/anim é procedural.
