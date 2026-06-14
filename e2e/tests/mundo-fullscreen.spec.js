// E2E do "Mundo full experience I" (YG-151): tela cheia + navegar o vault inteiro.
// Monta uma instância REAL com ≥3 níveis de pasta aninhada + uma nota profunda,
// entra em FULLSCREEN, caminha do topo até a nota funda (pasta = sala, nota =
// objeto), abre a nota (lê o .md real do NoteStore) e SAI da tela cheia.
//
// Auto-contido: assina um JWT (mesmo segredo do servidor de teste) e monta a
// instância via API real. Só roda contra servidor local — pula em alvos remotos.
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

test('Mundo full experience: fullscreen → navegar ≥3 níveis → abrir nota profunda → sair (YG-151)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados via API — só roda contra servidor local/CI');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  const sub = 'e2e-mundo-fs-' + Date.now();
  const token = signJwt(sub, 'mundo-fs@e2e.test');
  const auth = { Authorization: `Bearer ${token}` };

  // ── monta uma instância real: raiz → Sala 1 → Sala 2 → Sala 3 → nota funda ──
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
  async function connect(from, to) {
    const r = await request.patch(`/api/v1/instances/${id}`, {
      headers: { ...auth, 'Content-Type': 'application/json' },
      data: { op: 'add_connection', connection: { id: `${from}-${to}`, from, to, props: { kind: 'parent' } } },
    });
    expect(r.ok(), `add_connection ${from}->${to}`).toBeTruthy();
  }

  await place({ id: 'sala1', block_type: 'pasta', pos: { x: 2, y: 2 }, label: 'Sala 1', props: { note_slug: 'sala-1' } });
  await place({ id: 'sala2', block_type: 'pasta', pos: { x: 4, y: 2 }, label: 'Sala 2', props: { note_slug: 'sala-2' } });
  await place({ id: 'sala3', block_type: 'pasta', pos: { x: 6, y: 2 }, label: 'Sala 3', props: { note_slug: 'sala-3' } });
  await place({ id: 'funda', block_type: 'note', pos: { x: 8, y: 2 }, label: 'Nota Funda', props: { note_slug: 'nota-funda' } });

  await putNote('sala-1', 'Sala 1', '');
  await putNote('sala-2', 'Sala 2', '');
  await putNote('sala-3', 'Sala 3', '');
  await putNote('nota-funda', 'Nota Funda', 'Conteúdo **profundo** no fundo do vault.');

  // aninhamento: sala2 ⊂ sala1 ⊂ raiz · sala3 ⊂ sala2 · nota-funda ⊂ sala3
  await connect('sala2', 'sala1');
  await connect('sala3', 'sala2');
  await connect('funda', 'sala3');

  // ── abre o instance view como dono e entra na view Mundo ──
  await page.addInitScript(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, token]);
  await page.goto(`/universos/instance/${id}`, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#title')).toHaveText('Novo universo', { timeout: 10_000 });

  await page.locator('#view-mundo').click();
  await expect(page.locator('#mundo-ui')).toBeVisible();
  await expect(page.locator('#mundo-tree')).toContainText('Sala 1'); // pasta = porta na raiz

  // o vault INTEIRO é navegável: toda pasta (recursiva) é uma sala conhecida
  const roomIds = await page.evaluate(() => window.MundoView.rooms);
  expect(roomIds).toEqual(expect.arrayContaining(['sala1', 'sala2', 'sala3']));

  // ── ENTRAR em tela cheia pelo botão da HUD ──
  await page.locator('#mundo-fs').click();
  await expect.poll(() => page.evaluate(() => !!document.fullscreenElement), {
    message: 'entrou em tela cheia', timeout: 5_000,
  }).toBe(true);

  // ── NAVEGAR ≥3 níveis de pasta: raiz → Sala 1 → Sala 2 → Sala 3 ──
  await page.locator('#mundo-tree .mt-folder', { hasText: 'Sala 1' }).click();
  expect(await page.evaluate(() => window.MundoView.cur)).toBe('sala1');
  await page.locator('#mundo-tree .mt-folder', { hasText: 'Sala 2' }).click();
  expect(await page.evaluate(() => window.MundoView.cur)).toBe('sala2');
  await page.locator('#mundo-tree .mt-folder', { hasText: 'Sala 3' }).click();
  expect(await page.evaluate(() => window.MundoView.cur)).toBe('sala3');
  // a trilha mostra o caminho completo no vault (raiz › Sala 1 › Sala 2 › Sala 3)
  await expect(page.locator('#mundo-crumb .mc-crumb.on')).toHaveText('Sala 3');
  await expect(page.locator('#mundo-crumb')).toContainText('Sala 1');

  // ── ABRIR a nota profunda → conteúdo .md REAL do NoteStore ──
  await page.locator('#mundo-tree .mt-note', { hasText: 'Nota Funda' }).click();
  await expect(page.locator('#mundo-panel')).toBeVisible();
  await expect(page.locator('#mundo-panel .mp-body')).toContainText('profundo no fundo do vault');

  // ── voltar à raiz pela trilha (sem subconjunto artificial) ──
  await page.locator('#mundo-crumb .mc-crumb').first().click();
  expect(await page.evaluate(() => window.MundoView.cur)).toBe('__root__');

  // ── SAIR da tela cheia pela tecla F ──
  await page.keyboard.press('f');
  await expect.poll(() => page.evaluate(() => !!document.fullscreenElement), {
    message: 'saiu da tela cheia', timeout: 5_000,
  }).toBe(false);

  expect(erros, 'sem exceção de página').toEqual([]);
});
