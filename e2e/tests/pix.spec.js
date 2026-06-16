// E2E do YG-163: PIX independente (BR Code copia-e-cola + QR). Quando a campanha
// tem YGGDRASIL_PIX_KEY configurada, POST /apoiar devolve o bloco `pix` e a
// landing mostra o QR + copia-e-cola. Só roda local/CI com a chave PIX setada.
const { test, expect } = require('@playwright/test');

function isLocal(baseURL) {
  return /localhost|127\.0\.0\.1/.test(baseURL || '');
}

test('apoiar devolve PIX copia-e-cola + QR quando há chave configurada (YG-163)', async ({ request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'precisa do servidor local com YGGDRASIL_PIX_KEY');

  const r = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'raiz', nome: 'Apoiador PIX', mostrar_creditos: true },
  });
  expect(r.ok()).toBeTruthy();
  const j = await r.json();

  // se o servidor de teste tiver PIX ligado, o bloco vem completo
  test.skip(!j.pix, 'servidor sem YGGDRASIL_PIX_KEY — PIX desligado');
  expect(j.pix.copia_e_cola).toMatch(/^000201/); // BR Code
  expect(j.pix.copia_e_cola).toContain('br.gov.bcb.pix');
  expect(j.pix.copia_e_cola).toContain('540560.00'); // tier raiz = R$60
  expect(j.pix.qr_svg).toContain('<svg');
  expect(j.pix.valor).toBe(60);
});

test('landing mostra QR + copia-e-cola ao apoiar (YG-163)', async ({ page, request, baseURL }) => {
  test.skip(!isLocal(baseURL), 'precisa do servidor local com YGGDRASIL_PIX_KEY');
  // pré-checa se o PIX está ligado neste servidor; se não, pula o teste de página
  const probe = await request.post('/api/v1/campanha/apoiar', {
    headers: { 'Content-Type': 'application/json' },
    data: { tier: 'semente', mostrar_creditos: false },
  });
  test.skip(!(await probe.json()).pix, 'servidor sem YGGDRASIL_PIX_KEY');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.goto('/campanha', { waitUntil: 'domcontentloaded' });

  // abre o modal do tier Raiz e registra o apoio
  await page.locator('#tiers button[data-tier="raiz"]').click();
  await expect(page.locator('#ap')).toHaveClass(/open/);
  await page.fill('#ap-nome', 'Apoiador Página');
  await page.locator('#ap-send').click();

  // bloco PIX aparece com QR (svg) + copia-e-cola
  await expect(page.locator('#ap-pix')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('#ap-qr svg')).toBeVisible();
  await expect(page.locator('#ap-cec')).toContainText('br.gov.bcb.pix');
  await expect(page.locator('#ap-send')).toBeHidden(); // não fecha sozinho; deixa pagar

  expect(erros).toEqual([]);
});
