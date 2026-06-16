// YG-159 passo 3: wikilinks resolvem por SLUG (id estável) e relabelam pro título
// do locale; link inexistente não quebra (marcado missing). Tradução nunca quebra link.
const { test, expect } = require('@playwright/test');
const isLocal = (u) => /localhost|127\.0\.0\.1/.test(u || '');

test('co-mundo: wikilink por slug relabela no locale, nunca quebra (YG-159)', async ({ page, baseURL }) => {
  test.skip(!isLocal(baseURL), 'mock do CO — local/CI');
  await page.route((u) => u.hostname === 'co.artelonga.com.br', (r) => r.fulfill({ json: { entries: [
    { path: 'a.md', title: 'A', body: 'veja [[b.md]] e [[fantasma.md]]', frontmatter: {} },
    { path: 'b.md', title: 'Bê', body: 'x', frontmatter: { title_en: 'Bee' } },
  ] } }));
  await page.goto('/co-mundo?u=tk', { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(400);
  let links = await page.evaluate(() => window.CoMundo.bodyLinks('veja [[b.md]] e [[fantasma.md]]'));
  expect(links[0]).toMatchObject({ slug: 'b.md', label: 'Bê' });   // resolve por slug, rótulo = título
  expect(links[1].slug).toBeFalsy();                                // inexistente → não quebra
  await page.evaluate(() => window.CoMundo.setLocale('en'));
  links = await page.evaluate(() => window.CoMundo.bodyLinks('veja [[b.md]]'));
  expect(links[0]).toMatchObject({ slug: 'b.md', label: 'Bee' });  // MESMO slug, rótulo localizado
});
