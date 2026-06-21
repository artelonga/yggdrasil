// E2E do sequencer espacial do ÑE'Ẽ (YG-170).
//
// Prova que: gravar uma caminhada de 3 notas produz um `EntryAudio::Sequence`
// com exatamente 3 passos, E que 3 eventos de áudio foram disparados via
// `AudioEngine` (via `window.__nee.sequencer.stats.audiosFired`).
//
// Usa o mesmo AudioContext falso do nee.spec.js (injetado via addInitScript),
// sem dependência de hardware de som.
const { test, expect } = require('@playwright/test');

const FAKE_AUDIO = `
  window.__fakeAudio = { oscillators: [], created: 0 };
  function FakeParam(v) { this.value = v; }
  FakeParam.prototype.setValueAtTime = function () { return this; };
  FakeParam.prototype.linearRampToValueAtTime = function () { return this; };
  function FakeOsc() {
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
  FakeCtx.prototype.createOscillator = function () { window.__fakeAudio.created++; return new FakeOsc(); };
  FakeCtx.prototype.createGain = function () { return new FakeGain(); };
  window.AudioContext = FakeCtx;
  window.webkitAudioContext = FakeCtx;
`;

test('ÑE\'Ẽ sequencer: gravar 3 notas → Sequence com 3 passos + 3 audiosFired (YG-170)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);

  await page.goto('/universos/nee');

  // Aguarda o pacote de música carregar (notas individuais ficam visíveis)
  const c4 = page.locator('button.entry[data-term="C4"]');
  await expect(c4).toBeVisible({ timeout: 10_000 });

  // Inicia gravação
  await page.locator('#btn-record').click();

  // "Caminha" por 3 notas (C4 → E4 → G4)
  await page.locator('button.entry[data-term="C4"]').click();
  await page.locator('button.entry[data-term="E4"]').click();
  await page.locator('button.entry[data-term="G4"]').click();

  // Para a gravação
  await page.locator('#btn-stop').click();

  // ── buffer de passos: exatamente 3 ──────────────────────────────────
  const steps = await page.evaluate(() => window.__nee.sequencer.steps.length);
  expect(steps).toBe(3);

  // ── eventos de áudio: 3 (1 por nota, cada uma = 1 voz) ──────────────
  const audiosFired = await page.evaluate(() => window.__nee.sequencer.stats.audiosFired);
  expect(audiosFired).toBe(3);

  // ── o objeto EntryAudio::Sequence resultante é válido ────────────────
  const seq = await page.evaluate(() => window.__nee.sequencer.toSequence());
  expect(seq).toBeTruthy();
  expect(seq.mode).toBe('sequence');
  expect(seq.steps.length).toBe(3);

  // cada passo tem freqs e duration_ms
  for (const step of seq.steps) {
    expect(step.freqs.length).toBeGreaterThanOrEqual(1);
    expect(typeof step.duration_ms).toBe('number');
    expect(step.duration_ms).toBeGreaterThan(0);
  }

  // ── o AudioEngine.stats também registrou as vozes ────────────────────
  const voices = await page.evaluate(() => window.__nee.engine.stats.voicesStarted);
  expect(voices).toBe(3); // 3 notas de 1 freq cada = 3 vozes

  // ── replay funciona (dispara mais vozes via engine) ──────────────────
  await page.locator('#btn-replay').click();
  const voicesAfterReplay = await page.evaluate(() => window.__nee.engine.stats.voicesStarted);
  expect(voicesAfterReplay).toBe(6); // +3 do replay

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});

test('ÑE\'Ẽ sequencer: BPM configura duration_ms dos passos (YG-170)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);

  await page.goto('/universos/nee');
  await expect(page.locator('button.entry[data-term="C4"]')).toBeVisible({ timeout: 10_000 });

  // Ajusta BPM para 60 → 1 passo = 1000 ms
  await page.locator('#bpm').fill('60');

  await page.locator('#btn-record').click();
  await page.locator('button.entry[data-term="C4"]').click();
  await page.locator('#btn-stop').click();

  const seq = await page.evaluate(() => window.__nee.sequencer.toSequence());
  expect(seq).toBeTruthy();
  expect(seq.steps[0].duration_ms).toBe(1000); // 60 000 / 60 = 1000 ms

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});
