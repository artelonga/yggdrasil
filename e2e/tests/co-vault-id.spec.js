// YG-159: co-vault honra frontmatter.parent (id estável) sobre o path.
// Prova que tradução (que muda path/título) não reformata a árvore.
const { test, expect } = require('@playwright/test');
const isLocal = (u) => /localhost|127\.0\.0\.1/.test(u || '');

test('co-vault: hierarquia por frontmatter.parent (id), com fallback path (YG-159)', async ({ page, baseURL }) => {
  test.skip(!isLocal(baseURL), 'usa hook window.CoMundo — servidor local/CI');
  await page.route((u) => u.hostname === 'co.artelonga.com.br', (r) => r.fulfill({ json: { entries: [] } }));
  await page.goto('/co-mundo?u=tk', { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(300);
  const rooms = await page.evaluate(() => window.CoMundo.roomsFrom([
    { path: 'projetos.md', title: 'Projetos', parent: null },               // vira pasta (alvo de parent-id)
    { path: 'tarefa-x.md', title: 'Tarefa X', parent: 'projetos.md' },      // filho POR ID (não por path)
    { path: 'jardim/plantas.md', title: 'Plantas', parent: null },          // hierarquia por PATH (fallback)
    { path: 'solta.md', title: 'Solta', parent: null },                     // raiz
  ]));
  expect(rooms['tarefa-x.md']).toBe('projetos.md');     // id estável, não ROOT (que o path daria)
  expect(rooms['jardim/plantas.md']).toBe('jardim');    // fallback path segue funcionando
  expect(rooms['solta.md']).toBe('__root__');
});
