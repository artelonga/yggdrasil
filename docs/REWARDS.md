# Recompensas — Campanha Yggdrasil

> Sistema de tiers para a campanha de financiamento. Os nomes seguem a metáfora da árvore (Yggdrasil): da semente ao topo. Sementes (`sementes`) são também a moeda interna da plataforma — o mesmo conceito serve para apostar no Poker, comprar skins na loja interna e desbloquear universos privados.

## Visão geral

| # | Tier | Faixa | Recompensa principal | Limite |
|---|---|---:|---|---:|
| 1 | Semente | R$ 25 | Acesso à v1.0 + nome nos créditos | sem limite |
| 2 | Raiz | R$ 60 | + skin "raiz dourada" + 1.000 sementes | sem limite |
| 3 | Galho | R$ 120 | + early access (3 meses) + universo privado de mestres | sem limite |
| 4 | Folhagem | R$ 250 | + 3 packs de skins temáticos + early access vitalício | sem limite |
| 5 | Tronco | R$ 500 | + closed beta de universos 3D + NPC nomeado em universo público | 50 vagas |
| 6 | Yggdrasil | R$ 1.500 | + universo 3D personalizado co-criado + 1h de mentoria + sem cobrança de assinaturas premium futuras | 10 vagas |

## Detalhamento por tier

### 1. Semente — R$ 25 (`reward.tier.semente`)

Apoio simbólico. Tudo o que o tier "Semente" entrega:

- Acesso à plataforma a partir da release `v1.0.0`.
- Nome listado em `docs/CREDITS.md` (público), agrupado em ordem alfabética.
- Newsletter mensal com atualizações de desenvolvimento (em PT-BR).

**Custo de entrega:** zero. É uma flag `tier=semente` na conta do usuário e uma linha em um arquivo Markdown.

### 2. Raiz — R$ 60 (`reward.tier.raiz`)

Tudo do tier anterior, mais:

- Skin exclusiva **"raiz dourada"** para o portal personagem do usuário no lobby (cosmético, único).
- 1.000 sementes adicionadas ao saldo inicial (saldo padrão é 10.000; este tier começa com 11.000).
- Avatar de apoiador: pequeno selo "raiz" exibido ao lado do nome em fóruns futuros.

**Custo de entrega:** baixo. Skin é um SVG/PNG produzido uma vez. Sementes são uma operação de banco interno.

### 3. Galho — R$ 120 (`reward.tier.galho`)

Tudo dos tiers anteriores, mais:

- **Early access** (3 meses antes do público): cada release `0.x.0` aparece para Galho 3 meses antes da liberação ampla.
- Acesso ao **universo privado de mestres**: um universo público-com-assinatura hospedado pela equipe Yggdrasil, com módulos de RPG curados, soundboards e mapas que rotacionam mensalmente.
- Sala privada (Discord ou similar) para feedback direto.

**Custo de entrega:** moderado. Curadoria do universo privado consome tempo, mas escala (todos compartilham o mesmo conteúdo).

### 4. Folhagem — R$ 250 (`reward.tier.folhagem`)

Tudo dos tiers anteriores, mais:

- 3 packs de skins temáticos:
  - **Medieval** (paleta terra, sigilos, tochas)
  - **Cyberpunk** (neon, glitch, hud futurista)
  - **Folclore brasileiro** (saci, curupira, cabaça, taipa)
- Cada pack contém: skin de portal, skin de avatar, paleta de tile, soundboard de 8 faixas.
- **Early access vitalício**: enquanto a conta existir, todas as features novas chegam aqui antes do público.

**Custo de entrega:** médio. Os 3 packs são produção fechada (artistas), mas pagos uma vez.

### 5. Tronco — R$ 500 (`reward.tier.tronco`)

Tudo dos tiers anteriores, mais:

- **Closed beta de universos 3D**: acesso ao cliente Godot (YG-16) antes do público; testes exclusivos a cada novo universo 3D; canal direto de bug report.
- **NPC nomeado** em um universo público oficial: nome do apoiador vira personagem (com sprite custom) em um dos universos curados pela equipe.
- Brinde físico opcional (a definir por logística): adesivo + carta de agradecimento impressa.

**Limite:** 50 vagas. Cria escassez, valoriza o compromisso e limita o backlog de NPCs nomeados.

**Custo de entrega:** médio-alto. Sprites custom + logística de envio físico (se aplicável).

### 6. Yggdrasil — R$ 1.500 (`reward.tier.yggdrasil`)

Tudo dos tiers anteriores, mais:

- **Universo 3D personalizado co-criado**: 1 sessão de descoberta (1h) com a equipe + entrega de um template 3D temático com a estética e nome escolhidos pelo apoiador. Inclui: paleta, ambientação sonora, 3 NPCs, 1 portal de mídia.
- **Mentoria de 1h** sobre uso avançado da plataforma (criação de universos próprios, plugin authoring, integração com ferramentas externas).
- **Lifetime sem cobrança** de assinaturas premium futuras: se a plataforma um dia tiver um plano pago mensal, esse tier nunca pagará.

**Limite:** 10 vagas. É o tier de patrocínio; cada um demanda atenção pessoal.

**Custo de entrega:** alto. Cada universo personalizado consome semanas de produção. Sem o limite de 10, a campanha vira oficina de design por encomenda em vez de plataforma.

## Add-ons (opcionais, não tiers)

Itens que qualquer tier pode adicionar:

| Add-on | Preço | Descrição |
|---|---:|---|
| Pack de assets de mapa | R$ 30 | 50 tiles + 20 sprites + 10 portais para uso em qualquer universo |
| NPC com sua cara | R$ 80 | Sprite 32x32 personalizado em um universo público da escolha do apoiador |
| Soundboard pack | R$ 40 | 20 faixas temáticas (combate, viagem, taverna, mistério) |
| Modelo (template) de universo | R$ 60 | Template editável com 1 mapa, 5 NPCs, 3 portais e roteiro de uma sessão |

## Mapeamento técnico

| Recompensa | Como vai ser entregue |
|---|---|
| Acesso à v1.0 | Conta criada com flag `tier` populada |
| Skins | Asset estático servido pelo `yggdrasil-web` por tier |
| Sementes | `yggdrasil_core::sementes::Sementes::creditar` (ver YG-10) |
| Early access | Feature flag por user_id; releases pré-públicas em branch `next` |
| Universo privado | Universe Yggdrasil com `visibilidade: público-com-assinatura` filtrado por tier |
| Universos 3D | Cliente Godot (YG-16) + protobuf streaming |
| NPC nomeado | Sprite custom + `EntityConfig` com `kind: "npc-apoiador"` |

## Estrutura i18n

As chaves vão em `i18n/pt.yaml` quando esse arquivo for criado (parte de YG-14). Estrutura prevista:

```yaml
reward:
  tier:
    semente:
      titulo: Semente
      preco: 25
      descricao: Acesso à v1.0 + nome nos créditos
      beneficios:
        - acesso_v1
        - nome_creditos
        - newsletter_mensal
    raiz:
      titulo: Raiz
      preco: 60
      # ...
  add_on:
    pack_mapa:
      titulo: Pack de assets de mapa
      preco: 30
```

A versão EN (`i18n/en.yaml`) só será preenchida após a release `v1.0.0`.

## Promessas auditáveis

Cada tier define o que está e o que **não** está coberto. A campanha não promete:

- Datas específicas de release além das declaradas em `CHANGELOG.md`.
- Universos 3D antes da `v1.0.0`.
- Mais skins que as 4 declaradas (Raiz + 3 packs Folhagem) sem novo financiamento.
- Migração de dados de outras plataformas (Notion, Obsidian) — pode existir, mas não é entregável de tier.

Mudanças de escopo só ocorrem com aviso público e sempre **adicionando** valor, nunca retirando.
