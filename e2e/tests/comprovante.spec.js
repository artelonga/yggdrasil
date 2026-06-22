// E2E do YG-172: comprovante de PIX. O apoiador anexa nota/arquivo ao seu apoio
// (público, por id); o operador vê "aguardando" vs "📎 enviado" e baixa o arquivo
// (admin-gated). Só roda local/CI.
const { test, expect } = require('@playwright/test');

function isLocal(baseURL) {
  return /localhost|127\.0\.0\.1/.test(baseURL || '');
}
const ADMIN = process.env.YGGDRASIL_ADMIN_TOKEN || 'admintest';

async function novoApoio(request) {
  const r = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'raiz', nome: 'Comprovante Teste', mostrar_creditos: false },
  });
  expect(r.ok()).toBeTruthy();
  return (await r.json()).id;
}

test('comprovante: nota marca enviado; arquivo só admin baixa; id inválido 404 (YG-172)', async ({ request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados — só local/CI');

  // id inexistente → 404
  const bad = await request.post('/api/v1/campanha/pledges/nada/comprovante', {
    multipart: { nota: 'x' },
  });
  expect(bad.status()).toBe(404);

  // apoio + nota
  const id = await novoApoio(request);
  const note = await request.post(`/api/v1/campanha/pledges/${id}/comprovante`, {
    multipart: { nota: 'E2E-REF-999 pago' },
  });
  expect(note.ok()).toBeTruthy();

  // outro apoio + arquivo (PNG fake)
  const id2 = await novoApoio(request);
  const png = Buffer.from('\x89PNG\r\n\x1a\nfake', 'binary');
  const up = await request.post(`/api/v1/campanha/pledges/${id2}/comprovante`, {
    multipart: { arquivo: { name: 'c.png', mimeType: 'image/png', buffer: png } },
  });
  expect(up.ok()).toBeTruthy();

  // admin vê o comprovante na lista
  const adm = await request.get('/api/v1/campanha/pledges', { headers: { Authorization: `Bearer ${ADMIN}` } });
  test.skip(adm.status() === 401, 'admin token não confere neste servidor');
  const pledges = await adm.json();
  const p1 = pledges.find((p) => p.id === id);
  const p2 = pledges.find((p) => p.id === id2);
  expect(p1.comprovante_em).toBeTruthy();
  expect(p1.comprovante_nota).toContain('E2E-REF-999');
  expect(p2.comprovante_arquivo).toBe(true);

  // baixar arquivo: 401 sem token, 200 com token (bytes corretos)
  const anon = await request.get(`/api/v1/campanha/pledges/${id2}/comprovante/arquivo`);
  expect(anon.status()).toBe(401);
  const dl = await request.get(`/api/v1/campanha/pledges/${id2}/comprovante/arquivo`, { headers: { Authorization: `Bearer ${ADMIN}` } });
  expect(dl.ok()).toBeTruthy();
  expect(dl.headers()['content-type']).toContain('image/png');
});

test('landing: form de comprovante aparece após apoiar e envia a nota (YG-172)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'só local/CI');
  // o form de comprovante só aparece quando há PIX (data.pix) — pré-checa
  const probe = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'semente', mostrar_creditos: false },
  });
  test.skip(!(await probe.json()).pix, 'servidor sem YGGDRASIL_PIX_KEY');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.goto('/campanha', { waitUntil: 'domcontentloaded' });
  await page.locator('#tiers button[data-tier="raiz"]').click();
  await page.locator('#ap-send').click();
  // PIX aparece e o form de comprovante também
  await expect(page.locator('#ap-pix')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('#ap-comp')).toBeVisible();
  // envia uma nota como comprovante
  await page.fill('#ap-comp-nota', 'paguei — E2E página');
  await page.locator('#ap-comp-send').click();
  await expect(page.locator('#ap-comp-ok')).toBeVisible({ timeout: 10_000 });
  expect(erros).toEqual([]);
});
