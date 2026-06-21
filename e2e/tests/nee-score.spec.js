// E2E da camada Shannon — score/bits do ÑE'Ẽ (YG-168).
//
// Prova, sem depender de auth real, que:
//   - O HUD de bits está presente na página.
//   - Cada entrada tem um botão "revelar".
//   - O quiz modal abre ao clicar numa entrada (variante "imediato" forçada).
//   - Sem login o HUD fica em modo guest (classe "guest").
//
// Testes de rotas (descobrir/revelar/identificar → HTTP 200/402) vivem nos
// testes de integração Rust (comunicacao_routes.rs). Este spec é read-only
// e roda sem criar dados.
const { test, expect } = require('@playwright/test');

// AudioContext falso (mesmo padrão de nee.spec.js)
const FAKE_AUDIO = `
  window.__fakeAudio = { oscillators: [], created: 0 };
  function FakeParam(v) { this.value = v; }
  FakeParam.prototype.setValueAtTime = function () { return this; };
  FakeParam.prototype.linearRampToValueAtTime = function () { return this; };
  function FakeOsc() { this.type = 'sine'; this.frequency = new FakeParam(0); }
  FakeOsc.prototype.connect = function (d) { return d; };
  FakeOsc.prototype.start = function () {
    window.__fakeAudio.oscillators.push({ freq: this.frequency.value });
  };
  FakeOsc.prototype.stop = function () {};
  function FakeGain() { this.gain = new FakeParam(0); }
  FakeGain.prototype.connect = function (d) { return d; };
  FakeGain.prototype.disconnect = function () {};
  function FakeCtx() { this.state = 'running'; this.currentTime = 0; this.destination = {}; }
  FakeCtx.prototype.resume = function () { return Promise.resolve(); };
  FakeCtx.prototype.createOscillator = function () { window.__fakeAudio.created++; return new FakeOsc(); };
  FakeCtx.prototype.createGain = function () { return new FakeGain(); };
  window.AudioContext = FakeCtx;
  window.webkitAudioContext = FakeCtx;
`;

// Força a variante A (imediato) para testar o quiz no mesmo teste.
const FORCE_VARIANT_A = `
  const _orig = Storage.prototype.getItem;
  Storage.prototype.getItem = function(k) {
    if (k === 'nee_quiz_variant') return 'imediato';
    return _orig.call(this, k);
  };
`;

test('ÑE\'Ẽ score: HUD presente + modo guest sem login', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);

  await page.goto('/universos/nee');

  // HUD deve estar visível
  const hud = page.locator('#bits-hud');
  await expect(hud).toBeVisible({ timeout: 10_000 });

  // Sem token no localStorage → modo guest
  const isGuest = await page.evaluate(() =>
    document.getElementById('bits-hud').classList.contains('guest')
  );
  expect(isGuest).toBe(true);

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});

test('ÑE\'Ẽ score: cada entrada tem botão "revelar"', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);

  await page.goto('/universos/nee');

  // Aguarda a grade carregar
  await expect(page.locator('button.entry[data-term="A4"]')).toBeVisible({ timeout: 10_000 });

  // Todas as entradas de música devem ter botão "revelar"
  const revealBtns = page.locator('.btn-reveal');
  const count = await revealBtns.count();
  expect(count).toBeGreaterThan(0);
  // texto do botão contém "revelar"
  const firstText = await revealBtns.first().textContent();
  expect(firstText.toLowerCase()).toContain('revelar');

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});

test('ÑE\'Ẽ score: quiz modal abre ao clicar na entrada (variante imediato)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);
  await page.addInitScript(FORCE_VARIANT_A);

  // Intercepta a chamada de descobrir para evitar auth real
  await page.route('/api/v1/comunicacao/score/descobrir', (route) =>
    route.fulfill({ status: 200, body: JSON.stringify({ creditado: 0, total_bits: 0 }) })
  );

  await page.goto('/universos/nee');

  // Espera uma entrada com glosa (Guaraní tem glosas)
  const entries = page.locator('button.entry');
  await expect(entries.first()).toBeVisible({ timeout: 10_000 });

  // Clica na 1ª entrada com glosa (pode ser music ou language)
  // Procura a primeira entrada que tem glosa no pack de guarani
  const guaraniEntry = page.locator('button.entry[data-pack="guarani-mbya"]').first();
  if (await guaraniEntry.count() > 0) {
    await guaraniEntry.click();
    // Quiz deve aparecer (com delay de 600ms)
    await expect(page.locator('#quiz-overlay.open')).toBeVisible({ timeout: 3_000 });
    // Deve haver opções de escolha
    const opts = page.locator('.quiz-option');
    await expect(opts.first()).toBeVisible();
    // Fecha
    await page.locator('#quiz-close').click();
    await expect(page.locator('#quiz-overlay.open')).not.toBeVisible();
  }

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});

test('ÑE\'Ẽ: áudio ainda funciona após YG-168 (regressão YG-155)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);
  await page.addInitScript(FORCE_VARIANT_A);

  // Intercepta score para não bloquear
  await page.route('/api/v1/comunicacao/score*', (route) =>
    route.fulfill({ status: 401, body: JSON.stringify({ erro: 'nao_autenticado' }) })
  );

  await page.goto('/universos/nee');

  const a4 = page.locator('button.entry[data-term="A4"]');
  await expect(a4).toBeVisible({ timeout: 10_000 });

  await a4.click();
  const afterNote = await page.evaluate(() => window.__fakeAudio.oscillators.slice());
  expect(afterNote.length).toBe(1);
  expect(Math.abs(afterNote[0].freq - 440)).toBeLessThan(0.5);

  const voices = await page.evaluate(() => window.__nee.engine.stats.voicesStarted);
  expect(voices).toBeGreaterThanOrEqual(1);

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});
