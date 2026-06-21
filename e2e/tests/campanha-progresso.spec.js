// E2E do YG-164: barra de progresso da campanha. Conta SÓ apoios confirmados;
// mostra meta/percentual/nº de apoiadores. Cria dados via API + confirma com o
// admin token. Só roda local/CI (precisa do admin token e da meta configurada).
const { test, expect } = require('@playwright/test');

function isLocal(baseURL) {
  return /localhost|127\.0\.0\.1/.test(baseURL || '');
}
const ADMIN = process.env.YGGDRASIL_ADMIN_TOKEN || 'admintest';

test('progresso conta só confirmados e a landing mostra a barra (YG-164)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'cria/confirma dados via API — só local/CI');

  // baseline
  const before = await (await request.get('/api/v1/campanha/progresso')).json();

  // registra um apoio (raiz = R$60) e confirma via admin
  const ap = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'raiz', nome: 'Barra Teste', mostrar_creditos: true },
  });
  expect(ap.ok()).toBeTruthy();
  const id = (await ap.json()).id;

  // pendente ainda NÃO conta no arrecadado
  const mid = await (await request.get('/api/v1/campanha/progresso')).json();
  expect(mid.arrecadado).toBe(before.arrecadado);
  expect(mid.pendentes).toBeGreaterThanOrEqual(1);

  const conf = await request.post(`/api/v1/campanha/pledges/${id}/confirmar`, {
    headers: { Authorization: `Bearer ${ADMIN}` },
  });
  test.skip(conf.status() === 401, 'admin token não confere neste servidor');
  expect(conf.ok()).toBeTruthy();

  // agora conta
  const after = await (await request.get('/api/v1/campanha/progresso')).json();
  expect(after.arrecadado).toBe(before.arrecadado + 60);
  expect(after.apoiadores).toBe(before.apoiadores + 1);

  // a landing mostra a barra (oculta só quando não há meta nem apoiadores)
  await page.goto('/campanha', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#progresso')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('#prog-arrecadado')).toContainText('R$');
  await expect(page.locator('#prog-foot')).toContainText('apoiador');
});
