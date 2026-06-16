// E2E do YG-162: stubs de conteúdo do SRD de Shandara — os povos prometidos
// (Matéria/Tempo/Energia), combate, magia e a primeira criatura viram docs
// navegáveis. Verifica API (tree + doc) e o reader renderizando um stub via
// deep-link. Independe de auth; roda em qualquer alvo.
const { test, expect } = require('@playwright/test');

const STUBS = [
  'povos/petreos',
  'povos/perenes',
  'povos/fulgores',
  'regras/combate',
  'regras/magia',
  'bestiario/eco-da-cicatriz',
];

test('SRD: stubs YG-162 aparecem na árvore e são servidos pela API', async ({ request, baseURL }) => {
  const tree = await request.get(baseURL + '/api/v1/shandara/srd');
  expect(tree.ok()).toBeTruthy();
  const paths = (await tree.json()).map((d) => d.path);
  for (const p of STUBS) {
    expect(paths, `tree contém ${p}`).toContain(p);
    const doc = await request.get(baseURL + '/api/v1/shandara/srd/' + p);
    expect(doc.ok(), `doc ${p} servido`).toBeTruthy();
    expect(await doc.text()).toContain('stub'); // marcado honestamente como stub
  }
});

test('SRD: reader renderiza um stub via deep-link (YG-162)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.goto('/universos/shandara#povos/petreos', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#doc h1')).toContainText('Pétreos', { timeout: 10_000 });
  expect(erros).toEqual([]);
});
