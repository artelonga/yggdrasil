// E2e do Mundo do CO (YG-153) — federação inbound. Mocka a API do CO via
// page.route (auth real do CO é inviável headless), provando o WIRING do cliente:
// (a) lê entries do CO → vault; (b) write-back faz PUT ao CO com path+body certos.
const { test, expect } = require('@playwright/test');

const isLocal = (u) => /localhost|127\.0\.0\.1/.test(u || '');

test('Mundo do CO: lê entries (mock) + write-back PUT com path/body corretos (YG-153)', async ({ page, baseURL }) => {
  test.skip(!isLocal(baseURL), 'mocka o CO via route — roda contra servidor local/CI');

  let putPath = null;
  let putBody = null;
  // só intercepta o CO (não o servidor YG local, que também tem /api/v1/universes)
  await page.route((url) => url.hostname === 'co.artelonga.com.br', async (route) => {
    const req = route.request();
    const m = new URL(req.url()).pathname.match(/\/universes\/[^/]+\/entries(?:\/(.*))?$/);
    const path = m && m[1] ? decodeURIComponent(m[1]) : null;
    if (req.method() === 'GET' && !path) {
      return route.fulfill({ json: { entries: [
        { path: 'bem-vindo.md', title: 'Bem-vindo', body: 'corpo bem-vindo', frontmatter: {} },
        { path: 'jardim/plantas.md', title: 'Plantas', body: 'corpo plantas', frontmatter: {} },
      ] } });
    }
    if (req.method() === 'GET' && path) {
      return route.fulfill({ json: { path, title: 'Bem-vindo', body: 'corpo bem-vindo', frontmatter: {} } });
    }
    if (req.method() === 'PUT') {
      putPath = path;
      try { putBody = JSON.parse(req.postData() || '{}').body; } catch { putBody = null; }
      return route.fulfill({ json: { ok: true } });
    }
    return route.fulfill({ status: 404, json: {} });
  });

  await page.goto('/co-mundo?u=testkey', { waitUntil: 'domcontentloaded' });

  // (a) lê o vault do CO (mock) → 2 entradas, hierarquia pelo path
  const entries = await page.evaluate(() => window.CoMundo.loadVault());
  expect(entries.length).toBe(2);
  expect(entries.map((e) => e.path)).toContain('jardim/plantas.md');
  const e = await page.evaluate(() => window.CoMundo.read('bem-vindo.md'));
  expect(e.body).toBe('corpo bem-vindo');

  // (b) write-back: editar → PUT ao CO com path + body corretos
  const res = await page.evaluate(() => window.CoMundo.saveNote('bem-vindo.md', 'corpo editado'));
  expect(res.ok).toBeTruthy();
  await expect.poll(() => putPath).toBe('bem-vindo.md');
  expect(putBody).toBe('corpo editado');
});
