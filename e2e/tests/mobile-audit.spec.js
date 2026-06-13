const { test, expect } = require('@playwright/test');
test.use({ viewport: { width: 390, height: 844 } }); // iPhone 13 logical px, chromium
const ROTAS = ['/', '/lobby', '/analytics', '/universos', '/universos/comunicacao', '/universos/corpus', '/universos/instance/new', '/universos/instance/demo-x'];
for (const rota of ROTAS) {
  test(`mobile: ${rota} sem overflow horizontal`, async ({ page }) => {
    await page.goto(rota);
    await page.waitForTimeout(1200);
    const o = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, cw: document.documentElement.clientWidth }));
    expect(o.sw, `scrollWidth ${o.sw} > clientWidth ${o.cw} (overflow lateral)`).toBeLessThanOrEqual(o.cw + 2);
  });
}
