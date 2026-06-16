// E2E do YG-160: o "🌍 Mundo" é a visão PRIMÁRIA editável (default ao abrir uma
// instância); "📋 Quadro" (kanban) é uma lente SECUNDÁRIA das tarefas (notas com
// status), e criar tarefa é um gesto só (título → nota + bloco no Mundo).
//
// Auto-contido: assina um JWT (mesmo segredo do servidor de teste) e monta a
// instância via API real. Só roda contra servidor local/CI — pula em prod.
const { test, expect } = require('@playwright/test');
const crypto = require('crypto');

const SECRET = process.env.YGGDRASIL_JWT_SECRET || 'ci-test';
const JWT_KEY = 'yggdrasil-jwt';

function b64url(buf) {
  return Buffer.from(buf).toString('base64').replace(/=/g, '').replace(/\+/g, '-').replace(/\//g, '_');
}
function signJwt(sub, email) {
  const now = Math.floor(Date.now() / 1000);
  const head = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const body = b64url(JSON.stringify({ sub, email, exp: now + 3600, iat: now }));
  const data = `${head}.${body}`;
  const sig = b64url(crypto.createHmac('sha256', SECRET).update(data).digest());
  return `${data}.${sig}`;
}
function isLocal(baseURL) {
  return /localhost|127\.0\.0\.1/.test(baseURL || '');
}

test('Mundo é a visão default; Quadro agrupa tarefas por status; criar tarefa é um gesto (YG-160)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados via API — só roda contra servidor local/CI');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  const sub = 'e2e-kanban-' + Date.now();
  const token = signJwt(sub, 'kanban@e2e.test');
  const auth = { Authorization: `Bearer ${token}` };

  // ── monta uma instância real com 2 tarefas (notas com status) ──
  const create = await request.post('/api/v1/instances', { headers: auth });
  expect(create.ok(), 'criar instância').toBeTruthy();
  const inst = await create.json();
  const id = inst.id;
  const layer = inst.layers[0].id;

  async function place(block) {
    const r = await request.patch(`/api/v1/instances/${id}`, {
      headers: { ...auth, 'Content-Type': 'application/json' },
      data: { op: 'place_block', layer, block },
    });
    expect(r.ok(), `place_block ${block.id}`).toBeTruthy();
  }
  async function putTask(slug, title, status) {
    const r = await request.put(`/api/v1/instances/${id}/notes/${slug}`, {
      headers: { ...auth, 'Content-Type': 'application/json' },
      data: { title, markdown: '', status },
    });
    expect(r.ok(), `put tarefa ${slug}`).toBeTruthy();
  }

  await place({ id: 'comecar', block_type: 'note', pos: { x: 2, y: 2 }, label: 'Começar', props: { note_slug: 'comecar' } });
  await place({ id: 'pintar', block_type: 'note', pos: { x: 5, y: 2 }, label: 'Pintar', props: { note_slug: 'pintar' } });
  await putTask('comecar', 'Começar', 'todo');
  await putTask('pintar', 'Pintar', 'doing');

  // ── abre o instance view como dono ──
  await page.addInitScript(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, token]);
  await page.goto(`/universos/instance/${id}`, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#title')).toHaveText('Novo universo', { timeout: 10_000 });

  // ── DEFAULT = Mundo: a view abre já no Mundo (editável), não na timeline ──
  await expect(page.locator('#view-mundo')).toHaveClass(/active/);
  await expect(page.locator('#mundo-ui')).toBeVisible();
  await expect(page.locator('#kanban-view')).toBeHidden();

  // ── Quadro: lente secundária — tarefas caem na coluna do seu status ──
  await page.locator('#view-kanban').click();
  await expect(page.locator('#kanban-view')).toBeVisible();
  await expect(page.locator('#mundo-ui')).toBeHidden();
  await expect(page.locator('#kanban-view .kcol').nth(0)).toContainText('Começar'); // A fazer
  await expect(page.locator('#kanban-view .kcol').nth(1)).toContainText('Pintar');  // Fazendo
  await expect(page.locator('#kanban-view .kcol .khd.todo span')).toHaveText('1');
  await expect(page.locator('#kanban-view .kcol .khd.doing span')).toHaveText('1');

  // ── criar tarefa num gesto: "+ tarefa" → prompt → aparece na coluna ──
  page.once('dialog', (d) => d.accept('Revisar')); // prompt('Nova tarefa:')
  await page.locator('#kanban-view .kcol').nth(2).locator('.kadd').click(); // coluna "Feita"
  await expect(page.locator('#kanban-view .kcol').nth(2)).toContainText('Revisar');
  await expect(page.locator('#kanban-view .kcol .khd.done span')).toHaveText('1');

  // a tarefa virou nota+bloco real: existe via API e tem status 'done'
  const got = await request.get(`/api/v1/instances/${id}/notes/revisar`, { headers: auth });
  expect(got.ok(), 'tarefa criada persiste como nota').toBeTruthy();

  // ── clicar num card abre o editor da nota (tarefa = nota com status) ──
  await page.locator('#kanban-view .kcol').nth(0).locator('.kc').first().click();
  await expect(page.locator('#note-editor')).toHaveClass(/open/, { timeout: 5000 });

  expect(erros, 'sem exceção de página').toEqual([]);
});
