// E2E do ÑE'Ẽ — LexiconPack + som via Web Audio (YG-155).
//
// Prova, SEM depender de hardware de som, que: o pacote de música carrega →
// clicar numa nota cria/parametriza um OscillatorNode (freq certa) → clicar num
// acorde cria várias vozes. Faz isso injetando um AudioContext FALSO que apenas
// registra os nós criados (page.addInitScript, antes do bundle rodar).
//
// Read-only (não cria dados): roda contra local OU prod — os pacotes-seed são
// sintetizados pelo servidor e servidos sem auth.
const { test, expect } = require('@playwright/test');

// AudioContext falso: registra cada oscilador (type + frequency) em
// window.__fakeAudio. connect() é encadeável (retorna o nó destino).
const FAKE_AUDIO = `
  window.__fakeAudio = { oscillators: [], created: 0 };
  function FakeParam(v) { this.value = v; }
  FakeParam.prototype.setValueAtTime = function () { return this; };
  FakeParam.prototype.linearRampToValueAtTime = function () { return this; };
  function FakeOsc(ctx) {
    this.type = 'sine';
    this.frequency = new FakeParam(0);
    this.onended = null;
  }
  FakeOsc.prototype.connect = function (dest) { return dest; };
  FakeOsc.prototype.start = function () {
    window.__fakeAudio.oscillators.push({ type: this.type, freq: this.frequency.value });
  };
  FakeOsc.prototype.stop = function () {};
  function FakeGain() { this.gain = new FakeParam(0); }
  FakeGain.prototype.connect = function (dest) { return dest; };
  FakeGain.prototype.disconnect = function () {};
  function FakeCtx() {
    this.state = 'running';
    this.currentTime = 0;
    this.destination = {};
  }
  FakeCtx.prototype.resume = function () { return Promise.resolve(); };
  FakeCtx.prototype.createOscillator = function () { window.__fakeAudio.created++; return new FakeOsc(this); };
  FakeCtx.prototype.createGain = function () { return new FakeGain(); };
  window.AudioContext = FakeCtx;
  window.webkitAudioContext = FakeCtx;
`;

test('ÑE\'Ẽ: pacote de música carrega → nota toca → acorde toca várias vozes (YG-155)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);

  await page.goto('/universos/nee');

  // pacote de música chegou (lazy-load) e renderizou notas/acordes.
  const a4 = page.locator('button.entry[data-term="A4"]');
  await expect(a4).toBeVisible({ timeout: 10_000 });

  // ── disparar uma NOTA → 1 oscilador com a freq de A4 (440 Hz) ──
  await a4.click();
  const afterNote = await page.evaluate(() => window.__fakeAudio.oscillators.slice());
  expect(afterNote.length).toBe(1);
  expect(Math.abs(afterNote[0].freq - 440)).toBeLessThan(0.5);

  // ── disparar um ACORDE (tríade de Dó maior) → 3 vozes juntas ──
  await page.locator('button.entry[data-term="I — Dó maior"]').click();
  const total = await page.evaluate(() => window.__fakeAudio.created);
  expect(total).toBe(4); // 1 da nota + 3 do acorde

  // a engine também contabilizou as vozes internamente.
  const voices = await page.evaluate(() => window.__nee.engine.stats.voicesStarted);
  expect(voices).toBe(4);

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});
