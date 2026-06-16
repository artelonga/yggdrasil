// E2E do YG-161: apoio independente (pledge) + página de créditos.
// Crowdfunding próprio (não-Catarse): POST /api/v1/campanha/apoiar registra um
// apoio; /creditos mostra só quem optou por aparecer (nunca e-mail). Só roda
// contra servidor local/CI (cria dados via API).
const { test, expect } = require('@playwright/test');

function isLocal(baseURL) {
  return /localhost|127\.0\.0\.1/.test(baseURL || '');
}

test('apoio independente aparece nos créditos (opt-in); opt-out fica fora (YG-161)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria dados via API — só roda contra servidor local/CI');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  const visivel = 'Apoiadora ' + Date.now();
  const oculto = 'Oculto ' + Date.now();

  // tiers canônicos vêm do backend
  const tiers = await request.get('/api/v1/campanha/tiers');
  expect(tiers.ok()).toBeTruthy();
  const tj = await tiers.json();
  expect(tj.find((t) => t.slug === 'raiz').preco).toBe(60);

  // opt-in: aparece nos créditos, com recado
  const a = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'raiz', nome: visivel, mensagem: 'da semente ao topo', mostrar_creditos: true, email: 'vis@x.com' },
  });
  expect(a.ok(), 'apoiar opt-in').toBeTruthy();
  const aj = await a.json();
  expect(aj.status).toBe('pendente'); // nada cobrado ainda
  expect(aj.valor).toBe(60);

  // opt-out: NÃO aparece
  const b = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'semente', nome: oculto, mostrar_creditos: false },
  });
  expect(b.ok(), 'apoiar opt-out').toBeTruthy();

  // a API de créditos nunca vaza e-mail
  const cred = await request.get('/api/v1/creditos');
  const credText = await cred.text();
  expect(credText).toContain(visivel);
  expect(credText).not.toContain(oculto);
  expect(credText).not.toContain('vis@x.com');

  // a página /creditos renderiza o apoiador opt-in sob o tier Raiz
  await page.goto('/creditos', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#roll')).toContainText(visivel, { timeout: 10_000 });
  await expect(page.locator('#roll')).toContainText('Raiz');
  await expect(page.locator('#roll')).not.toContainText(oculto);
  await expect(page.locator('#roll')).toContainText('da semente ao topo'); // recado público

  const count = await page.evaluate(() => window.Creditos.count);
  expect(count).toBeGreaterThanOrEqual(1);

  expect(erros, 'sem exceção de página').toEqual([]);
});

test('tier inválido é rejeitado (400) (YG-161)', async ({ request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'só local/CI');
  const r = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'ouro' },
  });
  expect(r.status()).toBe(400);
});
