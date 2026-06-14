// E2E da travessia entre universos no "🌍 Mundo" (YG-157): do universo A → pisar
// num portal → carrega o universo B (lazy) → andar lá → voltar pro A pela trilha
// de universos. Espelha o `universe-as-node` do CO (instância YG ↔ universe_key).
// Construído SOBRE o MundoView unificado (YG-156): o loader lazy
// {rootId,ids,get,roomOf} + drag-drop durável continuam — os portais são uma
// camada nova, não uma reescrita do loader.
//
// Auto-contido: assina JWTs (mesmo segredo do servidor de teste) e monta as
// instâncias via API real. Só roda local — pula em alvos remotos (prod).
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

test('Mundo: A → portal → B → andar → voltar pro A (YG-157)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados via API — só roda contra servidor local/CI');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  const ts = Date.now();
  const dono = 'e2e-portais-' + ts;
  const outro = 'e2e-outro-' + ts;
  const token = signJwt(dono, 'portais@e2e.test');
  const tokenOutro = signJwt(outro, 'outro@e2e.test');

  async function novoUniverso(titulo, tok) {
    const r = await request.post(`/api/v1/instances?title=${encodeURIComponent(titulo)}`, {
      headers: { Authorization: `Bearer ${tok}` },
    });
    expect(r.ok(), `criar ${titulo}`).toBeTruthy();
    return r.json();
  }
  async function place(id, block, tok) {
    const r = await request.patch(`/api/v1/instances/${id}`, {
      headers: { Authorization: `Bearer ${tok}`, 'Content-Type': 'application/json' },
      data: { op: 'place_block', layer: 'base', block },
    });
    expect(r.ok(), `place_block ${block.id}`).toBeTruthy();
  }
  async function putNote(id, slug, title, markdown, tok) {
    const r = await request.put(`/api/v1/instances/${id}/notes/${slug}`, {
      headers: { Authorization: `Bearer ${tok}`, 'Content-Type': 'application/json' },
      data: { title, markdown },
    });
    expect(r.ok(), `put nota ${slug}`).toBeTruthy();
  }
  // ── universo A (com 1 nota) e universo B (com 1 nota), mesmos donos ──
  const A = await novoUniverso('Universo A', token);
  const B = await novoUniverso('Universo B', token);
  await place(A.id, { id: 'sol', block_type: 'note', pos: { x: 2, y: 2 }, label: 'Sol', props: { note_slug: 'sol' } }, token);
  await putNote(A.id, 'sol', 'Sol', 'O sol do universo A.', token);
  await place(B.id, { id: 'estrela', block_type: 'note', pos: { x: 2, y: 2 }, label: 'Estrela', props: { note_slug: 'estrela' } }, token);
  await putNote(B.id, 'estrela', 'Estrela', 'Uma estrela do universo B.', token);

  // universo de OUTRO dono, não publicado → NÃO deve virar portal (visibilidade)
  await novoUniverso('Universo Secreto', tokenOutro);

  // ── abre o Mundo de A como dono ──
  await page.addInitScript(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, token]);
  await page.goto(`/universos/instance/${A.id}`, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#title')).toHaveText('Universo A', { timeout: 10_000 });
  await page.locator('#view-mundo').click();
  await expect(page.locator('#mundo-ui')).toBeVisible();

  // o portal pro universo B aparece na fronteira (sala raiz); o secreto não
  await expect(page.locator('#mundo-tree .mt-portal', { hasText: 'Universo B' })).toBeVisible();
  await expect(page.locator('#mundo-tree')).not.toContainText('Universo Secreto');
  expect(await page.evaluate(() => window.MundoView.universe)).toBe(A.id);

  // ── ATRAVESSAR: clica no portal pro universo B → carrega o mundo de B (lazy) ──
  await page.locator('#mundo-tree .mt-portal', { hasText: 'Universo B' }).click();
  await expect.poll(() => page.evaluate(() => window.MundoView.universe)).toBe(B.id);

  // breadcrumb de universos: A › B (B atual); conteúdo é o de B
  await expect(page.locator('#mundo-uni')).toBeVisible();
  await expect(page.locator('#mundo-uni')).toContainText('Universo A');
  await expect(page.locator('#mundo-uni')).toContainText('Universo B');
  await expect(page.locator('#mundo-tree')).toContainText('Estrela');
  expect(await page.evaluate(() => window.MundoView.universeDepth)).toBe(1);

  // ── ANDAR em B: avatar/câmera preservados, move pra direita ──
  const x0 = await page.evaluate(() => window.MundoView.pos.x);
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(350);
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(350);
  const x1 = await page.evaluate(() => window.MundoView.pos.x);
  expect(x1, 'avatar andou em B').toBeGreaterThan(x0);

  // ── VOLTAR pro A pela trilha de universos (pop da pilha) ──
  await page.locator('#mundo-uni .mu-crumb', { hasText: 'Universo A' }).click();
  await expect.poll(() => page.evaluate(() => window.MundoView.universe)).toBe(A.id);
  await expect(page.locator('#mundo-tree')).toContainText('Sol');
  await expect(page.locator('#mundo-tree .mt-portal', { hasText: 'Universo B' })).toBeVisible();
  expect(await page.evaluate(() => window.MundoView.universeDepth)).toBe(0);

  expect(erros, 'sem exceção de página').toEqual([]);
});
