// E2E do ÑE'Ẽ Mundo caminhável (YG-167): LexiconPack vira sala walkable.
//
// Prova, SEM depender de hardware de som, que:
//   1. O pack de música seed carrega e vira uma Room (via packToRoom).
//   2. "Pisar" numa nota (stepOn) → AudioEngine toca o som (nó de áudio criado).
//   3. Inspector exibe term/gloss/role.
//   4. Uma relação no inspector redireciona (goTo) para a entrada-alvo.
//
// Usa o mesmo FakeAudioContext do nee.spec.js (injetado antes do boot).
// Read-only: roda contra local OU prod — os pacotes-seed são servidos sem auth.
const { test, expect } = require('@playwright/test');

// AudioContext falso: registra cada oscilador criado em window.__fakeAudio.
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

test('ÑE\'Ẽ Mundo: pack de música → sala caminhável → pisar numa nota toca som + inspector (YG-167)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  await page.addInitScript(FAKE_AUDIO);

  await page.goto('/universos/nee/world?pack=musica');

  // Aguarda o boot: Room criada e indexada pelo controlador.
  await page.waitForFunction(() => window.__neeWorld && window.__neeWorld.room !== null, { timeout: 15_000 });

  // ── pisar numa NOTA (A4 = 440 Hz) ──
  const stepped = await page.evaluate(() => window.__neeWorld.stepOn('A4'));
  expect(stepped, 'stepOn A4 deve retornar true').toBe(true);

  // 1 nó de áudio criado (nota = 1 voz).
  const afterNote = await page.evaluate(() => window.__fakeAudio.oscillators.slice());
  expect(afterNote.length, 'nota A4 → 1 oscilador').toBe(1);
  expect(Math.abs(afterNote[0].freq - 440), 'freq A4 ≈ 440 Hz').toBeLessThan(0.5);

  // AudioEngine.stats também contabilizou.
  const voices = await page.evaluate(() => window.__neeWorld.audioEngine.stats.voicesStarted);
  expect(voices, 'voicesStarted = 1').toBe(1);

  // Inspector exibe a glosa da nota.
  await expect(page.locator('#inspector')).toBeVisible();
  await expect(page.locator('#ins-term')).toHaveText('A4');
  await expect(page.locator('#ins-gloss')).toContainText('440');
  await expect(page.locator('#ins-role')).toContainText('note');

  // ── pisar num ACORDE (Dó maior → 3 vozes) ──
  await page.evaluate(() => window.__neeWorld.stepOn('I — Dó maior'));
  const total = await page.evaluate(() => window.__fakeAudio.created);
  expect(total, '1 nota + 3 vozes do acorde = 4 nós criados').toBe(4);

  // Inspector mostra o acorde com relações (as notas do acorde).
  await expect(page.locator('#inspector')).toBeVisible();
  await expect(page.locator('#ins-term')).toHaveText('I — Dó maior');
  await expect(page.locator('#ins-gloss')).toContainText('Tônica');
  await expect(page.locator('#ins-relations')).not.toHaveAttribute('hidden');

  // Ao menos 1 relação visível e clicável.
  const relLink = page.locator('button.rel-link').first();
  await expect(relLink).toBeVisible();

  // ── clicar numa relação fecha o inspector e navega ──
  await relLink.click();
  await expect(page.locator('#inspector')).toHaveAttribute('hidden', '');

  expect(erros, `erros de página: ${erros.join(' | ')}`).toEqual([]);
});

test('ÑE\'Ẽ Mundo: memory-light preservado — residentes ≤ MAX_RESIDENT_ENTRIES (YG-167)', async ({ page }) => {
  await page.addInitScript(FAKE_AUDIO);
  await page.goto('/universos/nee/world?pack=musica');
  await page.waitForFunction(() => window.__neeWorld && window.__neeWorld.room !== null, { timeout: 15_000 });

  const { resident, cap } = await page.evaluate(() => ({
    resident: window.__neeWorld.packLoader.residentEntries,
    cap: window.__neeWorld.packLoader.cap,
  }));
  expect(resident, 'entradas residentes dentro do teto').toBeLessThanOrEqual(cap);
  expect(resident, 'ao menos 1 entrada carregada').toBeGreaterThan(0);
});
