// E2E do YG-165: admin de apoios. GET /api/v1/campanha/pledges é admin-gated
// (única rota que expõe e-mail); a página /campanha/admin lista e confirma.
// Só roda local/CI (precisa do admin token).
const { test, expect } = require('@playwright/test');

function isLocal(baseURL) {
  return /localhost|127\.0\.0\.1/.test(baseURL || '');
}
const ADMIN = process.env.YGGDRASIL_ADMIN_TOKEN || 'admintest';

test('GET /pledges exige admin token (401 sem, 200 com — e só aqui vaza e-mail) (YG-165)', async ({ request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'admin-gated — só local/CI');

  // cria um apoio com e-mail
  const ap = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'galho', nome: 'Operador Teste', email: 'op@x.com', mostrar_creditos: false },
  });
  expect(ap.ok()).toBeTruthy();

  // sem token → 401
  const anon = await request.get('/api/v1/campanha/pledges');
  expect(anon.status()).toBe(401);

  // créditos público NUNCA traz e-mail (mesmo deste apoio opt-out)
  const cred = await (await request.get('/api/v1/creditos')).text();
  expect(cred).not.toContain('op@x.com');

  // com admin → 200 e traz e-mail (visão do operador)
  const adm = await request.get('/api/v1/campanha/pledges', { headers: { Authorization: `Bearer ${ADMIN}` } });
  test.skip(adm.status() === 401, 'admin token não confere neste servidor');
  expect(adm.ok()).toBeTruthy();
  const body = await adm.text();
  expect(body).toContain('op@x.com');
  expect(body).toContain('Operador Teste');
});

test('página /campanha/admin lista e confirma um apoio (YG-165)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'admin-gated — só local/CI');

  // garante que há um apoio pendente
  await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'raiz', nome: 'Para Confirmar', mostrar_creditos: true },
  });
  // confirma que o admin token funciona neste servidor
  const probe = await request.get('/api/v1/campanha/pledges', { headers: { Authorization: `Bearer ${ADMIN}` } });
  test.skip(probe.status() === 401, 'admin token não confere neste servidor');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.goto('/campanha/admin', { waitUntil: 'domcontentloaded' });

  await page.fill('#tok', ADMIN);
  await page.click('#load');

  // a tabela carrega com pelo menos um apoio
  await expect(page.locator('#tbl')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('#rows tr')).not.toHaveCount(0);
  const total = await page.evaluate(() => window.AdminApoios.count);
  expect(total).toBeGreaterThanOrEqual(1);

  // confirma o primeiro pendente, se houver botão. Asserto no estado DURÁVEL
  // (contagem de confirmados sobe + badge confirmado aparece), não na mensagem
  // transitória — o reload pós-confirmação limpa o #msg.
  const confBtn = page.locator('#rows button.conf').first();
  if (await confBtn.count()) {
    const confAntes = await page.evaluate(() => window.AdminApoios.confirmados);
    await confBtn.click();
    await expect.poll(() => page.evaluate(() => window.AdminApoios.confirmados), { timeout: 10_000 })
      .toBeGreaterThan(confAntes);
    await expect(page.locator('#rows .st.confirmado').first()).toBeVisible();
  }

  expect(erros).toEqual([]);
});
