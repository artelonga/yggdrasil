/* mundo/sample.js — hierarquia de salas de exemplo (YG-146 protótipo).
 *
 * Demonstra o modelo recursivo: universo = sala raiz; pasta = porta → sub-sala;
 * nota = objeto pisável; NPC = personagem. No produto isto vem do InstanceStore
 * (blocos com posição + props.note_slug + hierarquia) — aqui é mock. */
import { TileKind } from './engine.js';

// Constrói uma sala retangular com borda de parede e entidades posicionadas.
function room(id, title, w, h, ents) {
  const tiles = [];
  for (let y = 0; y < h; y++) {
    const row = [];
    for (let x = 0; x < w; x++) {
      row.push(x === 0 || y === 0 || x === w - 1 || y === h - 1 ? TileKind.WALL : TileKind.FLOOR);
    }
    tiles.push(row);
  }
  return Object.assign({ id, title, w, h, tiles }, ents);
}

export const ROOMS = {
  raiz: room('raiz', 'Meu Universo', 15, 11, {
    spawn: { x: 7, y: 8 },
    npcs: [{ x: 7, y: 6, name: 'Guia' }],
    doors: [
      { x: 4, y: 2, label: 'Jardim', target: 'jardim' },
      { x: 10, y: 2, label: 'Projetos', target: 'projetos' },
    ],
    notes: [
      { x: 7, y: 4, title: 'Bem-vindo', kind: 'artigo', slug: 'bem-vindo', body: '# Bem-vindo ao seu universo\n\nCaminhe com WASD ou clique. Pise nas **portas** (pastas) pra entrar, nos **objetos** (notas) pra abrir. Fale com o **Guia**.' },
      { x: 12, y: 7, title: 'Sobre', kind: 'artigo', slug: 'sobre', body: 'Um universo é um espaço vivo de notas e salas. Organize arrastando (no produto).' },
    ],
  }),
  jardim: room('jardim', 'Jardim', 13, 9, {
    spawn: { x: 6, y: 6 },
    exit: { x: 6, y: 7, target: 'raiz' },
    notes: [
      { x: 3, y: 3, title: 'Plantas', kind: 'indice', slug: 'plantas', body: 'Índice de plantas do jardim.' },
      { x: 9, y: 3, title: 'Sementes', kind: 'artigo', status: 'doing', slug: 'sementes', body: 'Tarefa em andamento: catalogar sementes.' },
      { x: 6, y: 4, title: 'Ideias', kind: 'artigo', status: 'todo', slug: 'ideias', body: 'A fazer: plantar ideias novas.' },
    ],
    doors: [{ x: 10, y: 2, label: 'Estufa', target: 'estufa' }],
  }),
  estufa: room('estufa', 'Estufa', 9, 7, {
    spawn: { x: 4, y: 4 },
    exit: { x: 4, y: 5, target: 'jardim' },
    notes: [
      { x: 3, y: 2, title: 'Orquídeas', kind: 'artigo', status: 'done', slug: 'orquideas', body: 'Concluído: orquídeas floresceram.' },
      { x: 5, y: 2, title: 'Mudas', kind: 'artigo', slug: 'mudas', body: 'Mudas em observação.' },
    ],
  }),
  projetos: room('projetos', 'Projetos', 13, 9, {
    spawn: { x: 6, y: 6 },
    exit: { x: 6, y: 7, target: 'raiz' },
    notes: [
      { x: 3, y: 3, title: 'Roadmap', kind: 'indice', status: 'doing', slug: 'roadmap', body: 'Roadmap do trimestre.' },
      { x: 9, y: 3, title: 'Tarefa X', kind: 'artigo', status: 'done', slug: 'tarefa-x', body: 'Feito: tarefa X entregue.' },
      { x: 6, y: 4, title: 'Notas', kind: 'pasta', slug: 'notas', body: 'Coletânea de notas soltas.' },
    ],
    doors: [{ x: 10, y: 2, label: 'Campanha', target: 'campanha' }],
  }),
  campanha: room('campanha', 'Campanha', 9, 7, {
    spawn: { x: 4, y: 4 },
    exit: { x: 4, y: 5, target: 'projetos' },
    notes: [
      { x: 4, y: 2, title: 'Tiers', kind: 'artigo', slug: 'tiers', body: 'Tiers de recompensa da campanha.' },
    ],
  }),
};

// Mapa de parentesco (pra breadcrumb/árvore viva).
export const PARENT = { jardim: 'raiz', projetos: 'raiz', estufa: 'jardim', campanha: 'projetos' };
