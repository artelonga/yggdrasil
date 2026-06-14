// E2E da view "🌍 Mundo" (YG-148): abrir uma instância REAL → andar → entrar
// numa pasta → abrir uma nota (lê o .md real do NoteStore, sem mock/sample.js).
//
// Auto-contido: assina um JWT (mesmo segredo do servidor de teste) e monta a
// instância via API real, depois exercita a UI. Só roda contra servidor local —
// pula em alvos remotos (prod), onde não há o segredo nem se deve criar dados.
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

test('Mundo: instância real → andar → entrar numa pasta → abrir uma nota (YG-148)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados via API — só roda contra servidor local/CI');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  const sub = 'e2e-mundo-' + Date.now();
  const token = signJwt(sub, 'mundo@e2e.test');
  const auth = { Authorization: `Bearer ${token}` };

  // ── monta uma instância real: raiz com 1 nota e 1 pasta; pasta com 1 nota ──
  const create = await request.post('/api/v1/instances', { headers: auth });
  expect(create.ok(), 'criar instância').toBeTruthy();
  const inst = await create.json();
  const id = inst.id;
  const layer = inst.layers[0].id; // "base"

  async function place(block) {
    const r = await request.patch(`/api/v1/instances/${id}`, {
      headers: { ...auth, 'Content-Type': 'application/json' },
      data: { op: 'place_block', layer, block },
    });
    expect(r.ok(), `place_block ${block.id}`).toBeTruthy();
  }
  async function putNote(slug, title, markdown) {
    const r = await request.put(`/api/v1/instances/${id}/notes/${slug}`, {
      headers: { ...auth, 'Content-Type': 'application/json' },
      data: { title, markdown },
    });
    expect(r.ok(), `put nota ${slug}`).toBeTruthy();
  }

  await place({ id: 'bemvindo', block_type: 'note', pos: { x: 2, y: 2 }, label: 'Bem-vindo', props: { note_slug: 'bem-vindo' } });
  await place({ id: 'jardim', block_type: 'pasta', pos: { x: 5, y: 2 }, label: 'Jardim', props: { note_slug: 'jardim' } });
  await place({ id: 'plantas', block_type: 'note', pos: { x: 8, y: 2 }, label: 'Plantas', props: { note_slug: 'plantas' } });

  await putNote('bem-vindo', 'Bem-vindo', '# Bem-vindo\n\nAo seu universo real.');
  await putNote('jardim', 'Jardim', ''); // sem corpo + com filhos → pasta (sala)
  await putNote('plantas', 'Plantas', 'Catálogo de **plantas** reais do jardim.');

  // plantas é filha de jardim → jardim vira pasta navegável (porta na raiz)
  const conn = await request.patch(`/api/v1/instances/${id}`, {
    headers: { ...auth, 'Content-Type': 'application/json' },
    data: { op: 'add_connection', connection: { id: 'p-j', from: 'plantas', to: 'jardim', props: { kind: 'parent' } } },
  });
  expect(conn.ok(), 'add_connection parent').toBeTruthy();

  // ── abre o instance view como dono (token no localStorage) ──
  await page.addInitScript(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, token]);
  await page.goto(`/universos/instance/${id}`, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#title')).toHaveText('Novo universo', { timeout: 10_000 });

  // entra na view Mundo
  await page.locator('#view-mundo').click();
  await expect(page.locator('#mundo-ui')).toBeVisible();
  await expect(page.locator('#mundo-tree')).toContainText('Jardim'); // pasta = porta
  await expect(page.locator('#mundo-tree')).toContainText('Bem-vindo'); // nota = objeto

  // a engine derivou salas reais da instância (raiz + jardim), não do mock
  const roomIds = await page.evaluate(() => window.MundoView.rooms);
  expect(roomIds).toContain('jardim');

  // ── ANDAR: setas direita e o avatar muda de tile (a engine escuta no window;
  // o foco está no botão da view, não num campo de texto → não sequestra) ──
  const x0 = await page.evaluate(() => window.MundoView.pos.x);
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(350);
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(350);
  const x1 = await page.evaluate(() => window.MundoView.pos.x);
  expect(x1, 'avatar andou para a direita').toBeGreaterThan(x0);

  // ── ENTRAR numa pasta (Jardim) ──
  await page.locator('#mundo-tree .mt-folder', { hasText: 'Jardim' }).click();
  await expect(page.locator('#mundo-crumb .mc-crumb.on')).toHaveText('Jardim');
  await expect(page.locator('#mundo-tree')).toContainText('Plantas');
  expect(await page.evaluate(() => window.MundoView.cur)).toBe('jardim');

  // ── ABRIR uma nota → conteúdo .md REAL do NoteStore ──
  await page.locator('#mundo-tree .mt-note', { hasText: 'Plantas' }).click();
  await expect(page.locator('#mundo-panel')).toBeVisible();
  await expect(page.locator('#mundo-panel .mp-body')).toContainText('plantas reais do jardim');

  expect(erros, 'sem exceção de página').toEqual([]);
});

test('Mundo: drag-drop persiste no .md → reload mantém posição; reparent move de pasta (YG-154)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados via API — só roda contra servidor local/CI');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  const sub = 'e2e-mundo-dd-' + Date.now();
  const token = signJwt(sub, 'mundodd@e2e.test');
  const auth = { Authorization: `Bearer ${token}` };

  // ── monta uma instância real: raiz com 1 nota (bem-vindo) e 1 pasta (jardim) ──
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
  async function putNote(slug, title, markdown) {
    const r = await request.put(`/api/v1/instances/${id}/notes/${slug}`, {
      headers: { ...auth, 'Content-Type': 'application/json' },
      data: { title, markdown },
    });
    expect(r.ok(), `put nota ${slug}`).toBeTruthy();
  }

  await place({ id: 'bemvindo', block_type: 'note', pos: { x: 2, y: 2 }, label: 'Bem-vindo', props: { note_slug: 'bem-vindo' } });
  await place({ id: 'jardim', block_type: 'pasta', pos: { x: 5, y: 2 }, label: 'Jardim', props: { note_slug: 'jardim' } });
  await place({ id: 'plantas', block_type: 'note', pos: { x: 8, y: 2 }, label: 'Plantas', props: { note_slug: 'plantas' } });
  await putNote('bem-vindo', 'Bem-vindo', 'Ao seu universo real.');
  await putNote('jardim', 'Jardim', '');
  await putNote('plantas', 'Plantas', 'Catálogo de plantas.');
  const conn = await request.patch(`/api/v1/instances/${id}`, {
    headers: { ...auth, 'Content-Type': 'application/json' },
    data: { op: 'add_connection', connection: { id: 'p-j', from: 'plantas', to: 'jardim', props: { kind: 'parent' } } },
  });
  expect(conn.ok(), 'add_connection parent').toBeTruthy();

  // ── abre o instance view como dono e entra na view Mundo ──
  await page.addInitScript(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, token]);
  await page.goto(`/universos/instance/${id}`, { waitUntil: 'domcontentloaded' });
  await page.locator('#view-mundo').click();
  await expect(page.locator('#mundo-ui')).toBeVisible();
  await expect(page.locator('#mundo-tree')).toContainText('Bem-vindo');

  // ── ARRASTAR (reposicionar) bem-vindo para uma célula livre → commit ao .md ──
  await Promise.all([
    page.waitForResponse((r) => r.url().includes(`/instances/${id}/layout`) && r.request().method() === 'POST'),
    page.evaluate(() => window.MundoView.drag('bem-vindo', 3, 4)),
  ]);
  const moved = await page.evaluate(() => window.MundoView.posOf('bem-vindo'));
  expect(moved).toMatchObject({ x: 3, y: 4 });

  // ── RELOAD → posição persistida no `.md` é mantida (não voltou ao auto-layout) ──
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.locator('#view-mundo').click();
  await expect(page.locator('#mundo-ui')).toBeVisible();
  const afterReload = await page.evaluate(() => window.MundoView.posOf('bem-vindo'));
  expect(afterReload, 'posição mantida após reload').toMatchObject({ x: 3, y: 4 });

  // ── REPARENT: arrastar bem-vindo para a pasta Jardim → nota muda de sala ──
  await Promise.all([
    page.waitForResponse((r) => r.url().includes(`/instances/${id}/layout`) && r.request().method() === 'POST'),
    page.evaluate(() => window.MundoView.reparent('bem-vindo', 'jardim')),
  ]);
  expect(await page.evaluate(() => window.MundoView.posOf('bem-vindo')).then((p) => p.room)).toBe('jardim');

  // ── RELOAD → a nota nasce dentro da nova pasta (parent persistido no `.md`) ──
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.locator('#view-mundo').click();
  await expect(page.locator('#mundo-ui')).toBeVisible();
  const reparented = await page.evaluate(() => window.MundoView.posOf('bem-vindo'));
  expect(reparented.room, 'nota na nova pasta após reload').toBe('jardim');

  expect(erros, 'sem exceção de página').toEqual([]);
});
