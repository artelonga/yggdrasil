// YG-159 i18n: toggle de locale relabela título/corpo a partir do frontmatter,
// mantendo identidade (slug) estável — base do i18n "tradução como camada".
const { test, expect } = require('@playwright/test');
const isLocal = (u) => /localhost|127\.0\.0\.1/.test(u || '');

test('co-mundo i18n: locale relabela (slug estável), fallback p/ fonte (YG-159)', async ({ page, baseURL }) => {
  test.skip(!isLocal(baseURL), 'mock do CO via route — servidor local/CI');
  await page.route((u) => u.hostname === 'co.artelonga.com.br', (r) => r.fulfill({ json: { entries: [
    { path: 'bem-vindo.md', title: 'Bem-vindo', body: 'olá', frontmatter: { title_en: 'Welcome', body_en: 'hello' } },
    { path: 'sobre.md', title: 'Sobre', body: 'x', frontmatter: {} },
  ] } }));
  await page.goto('/co-mundo?u=tk', { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(400);
  expect(await page.evaluate(() => window.CoMundo.locales())).toContain('en');
  const src = await page.evaluate(() => window.CoMundo.localized(''));
  expect(src['bem-vindo.md']).toBe('Bem-vindo');
  const en = await page.evaluate(() => window.CoMundo.localized('en'));
  expect(en['bem-vindo.md']).toBe('Welcome');   // traduzido
  expect(en['sobre.md']).toBe('Sobre');          // sem tradução → fallback fonte (id estável)
});
