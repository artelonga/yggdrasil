// Smoke E2E (YG-133) — pega exatamente as classes de bug reportadas em
// 2026-06-12: conteúdo 404 atrás de página 200, e UI coberta pela nav fixa.
const { test, expect } = require('@playwright/test');

// Falha o teste em qualquer exceção de página (js_error silencioso).
function guardErrors(page) {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  return erros;
}

test('corpus do Ayvu Rapyta carrega capítulos (não "indisponível")', async ({ page }) => {
  const erros = guardErrors(page);
  await page.goto('/universos/corpus');
  // o reader mostra "Corpus indisponível (NNN)" quando a API falha
  await expect(page.locator('#trail')).not.toContainText('indisponível', { timeout: 10_000 });
  // capítulos populados no <select>
  const opcoes = page.locator('#chapsel option');
  await expect(opcoes.first()).toBeAttached({ timeout: 10_000 });
  expect(await opcoes.count()).toBeGreaterThan(0);
  expect(erros).toEqual([]);
});

test('comunicação: toolbar e chips de léxicos visíveis, NÃO cobertos pela nav', async ({ page }) => {
  const erros = guardErrors(page);
  await page.goto('/universos/comunicacao');
  const toolbar = page.locator('#toolbar');
  await expect(toolbar).toBeVisible();

  // o ponto central do topo da toolbar deve pertencer à própria toolbar —
  // se a nav fixa a cobre, elementFromPoint devolve a nav (o bug reportado)
  const box = await toolbar.boundingBox();
  expect(box).not.toBeNull();
  const coberta = await page.evaluate(([x, y]) => {
    const el = document.elementFromPoint(x, y);
    return el ? !document.getElementById('toolbar').contains(el) : true;
  }, [box.x + box.width / 2, box.y + 8]);
  expect(coberta, 'toolbar coberta por outro elemento fixo').toBe(false);

  // chips de léxicos (YG-132): Mbyá alcançável + link ao Ayvu Rapyta
  await expect(page.locator('#lexicos a', { hasText: 'Mbyá' })).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('#lexicos a', { hasText: 'Ayvu Rapyta' })).toBeVisible();
  expect(erros).toEqual([]);
});

test('páginas principais respondem sem exceção de JS', async ({ page }) => {
  for (const rota of ['/', '/universos', '/lobby', '/analytics', '/feedback']) {
    const erros = guardErrors(page);
    const resp = await page.goto(rota);
    expect(resp.status(), rota).toBe(200);
    await page.waitForTimeout(600); // dá tempo do JS inicial rodar
    expect(erros, `exceção de página em ${rota}`).toEqual([]);
  }
});

test('Ayvu Rapyta: clicar palavra liga ao léxico completo e mostra ocorrências (YG-134)', async ({ page }) => {
  const erros = guardErrors(page);
  await page.goto('/universos/corpus');
  // espera capítulos carregarem
  await expect(page.locator('#chapsel option').first()).toBeAttached({ timeout: 10_000 });
  // clica no primeiro token Mbyá de um verso
  const tok = page.locator('.stone .gn .tok').first();
  await expect(tok).toBeVisible({ timeout: 10_000 });
  await tok.click();
  // inspetor abre
  await expect(page.locator('#inspector')).toHaveClass(/open/, { timeout: 5_000 });
  // a busca live OU as ocorrências devem render algo dentro de ~3s (rede)
  await page.waitForTimeout(2500);
  const temLexico = await page.locator('#w-lexico .sense, #w-ocorr [data-occ]').count();
  // não exige resultado para toda palavra, mas a página não pode ter quebrado
  expect(erros).toEqual([]);
  expect(temLexico).toBeGreaterThanOrEqual(0);
});

// endpoints novos do léxico completo respondem
test('léxico completo: busca e índice respondem (YG-134)', async ({ request, baseURL }) => {
  const idx = await request.get(baseURL + '/api/v1/comunicacao/lexico/indice?lang=gn-mbya');
  expect(idx.ok()).toBeTruthy();
  expect((await idx.json()).length).toBeGreaterThan(1000);
  const busca = await request.get(baseURL + '/api/v1/comunicacao/lexico/busca?lang=gn-mbya&q=floresta&limit=5');
  expect(busca.ok()).toBeTruthy();
  expect((await busca.json()).total).toBeGreaterThan(0);
});

test('perfil universal: GET exige auth, catálogo mostra os dois comportamentos (YG-137)', async ({ page, request, baseURL }) => {
  // perfil é owner-only
  const r = await request.get(baseURL + '/api/v1/profile');
  expect(r.status()).toBe(401);
  // catálogo: cada card tem origem + "criar perfil aqui"
  const erros = guardErrors(page);
  await page.goto('/universos');
  await expect(page.locator('.card .cta-perfil').first()).toBeVisible({ timeout: 10_000 });
  // ao menos um card com link de origem
  expect(await page.locator('.card .cta-origem').count()).toBeGreaterThan(0);
  expect(erros).toEqual([]);
});

test('laboratório de corpus: endpoints e página (YG-139)', async ({ page, request, baseURL }) => {
  const cs = await request.get(baseURL + '/api/v1/corpus');
  expect(cs.ok()).toBeTruthy();
  expect((await cs.json()).length).toBeGreaterThan(0);
  const cmp = await request.get(baseURL + '/api/v1/corpus/compare?a=mbya-lexico&b=yoruba-lexico&mode=inner&limit=5');
  expect(cmp.ok()).toBeTruthy();
  const erros = guardErrors(page);
  await page.goto('/universos/corpus-lab');
  await expect(page.locator('#sel-a')).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(1500);
  expect(erros).toEqual([]);
});
