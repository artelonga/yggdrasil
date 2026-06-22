// E2E do YG-174: passkeys (WebAuthn) reais, com autenticador virtual (CDP).
// Fluxo completo: (logado) registrar passkey → (deslogado) entrar com passkey →
// Yggdrasil emite o JWT local. Só roda local/CI (cria dados; precisa do segredo).
const { test, expect } = require('@playwright/test');
const crypto = require('crypto');

const SECRET = process.env.YGGDRASIL_JWT_SECRET || 'ci-test';
const JWT_KEY = 'yggdrasil-jwt';
const EMAIL = 'passkey@e2e.test';
const SUB = 'e2e-passkey';

function b64url(buf) {
  return Buffer.from(buf).toString('base64').replace(/=/g, '').replace(/\+/g, '-').replace(/\//g, '_');
}
function signJwt(sub, email) {
  const now = Math.floor(Date.now() / 1000);
  const head = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const body = b64url(JSON.stringify({ sub, email, exp: now + 3600, iat: now }));
  const data = `${head}.${body}`;
  const sig = b64url(crypto.createHmac('sha256', SECRET).update(data).digest());
  return `${data}.${sig}`;
}
function isLocal(u) { return /localhost/.test(u || ''); }

test('passkey: registrar (logado) e depois entrar com biometria (YG-174)', async ({ page, baseURL }) => {
  // rp_id=localhost exige origin localhost; este spec usa BASE_URL=http://localhost:3030
  test.skip(!isLocal(baseURL), 'WebAuthn rp_id=localhost — rode com BASE_URL=http://localhost:3030');

  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  // autenticador virtual (plataforma, user-verified — simula Face ID/digital)
  const client = await page.context().newCDPSession(page);
  await client.send('WebAuthn.enable');
  await client.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2', transport: 'internal',
      hasResidentKey: true, hasUserVerification: true,
      isUserVerified: true, automaticPresenceSimulation: true,
    },
  });

  const token = signJwt(SUB, EMAIL);

  // ── 1) REGISTRAR: logado (?force=1 não redireciona) ──
  // injeta o JWT via evaluate + reload (NÃO addInitScript — ele re-injetaria em
  // toda navegação e o passo 2 nunca ficaria deslogado).
  await page.goto('/login?force=1', { waitUntil: 'domcontentloaded' });
  await page.evaluate(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, token]);
  await page.goto('/login?force=1', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#passkey-reg')).toBeVisible({ timeout: 10_000 });
  await page.locator('#btn-passkey-reg').click();
  await expect(page.locator('#passkey-reg-msg')).toContainText('registrado', { timeout: 15_000 });

  // ── 2) LOGIN: desloga, entra só com o passkey ──
  await page.evaluate((k) => localStorage.removeItem(k), JWT_KEY);
  await page.goto('/login', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#btn-passkey')).toBeVisible();
  await page.fill('#email', EMAIL);
  await page.locator('#btn-passkey').click();

  // login bem-sucedido → JWT no localStorage + saiu da /login (foi pro lobby)
  await expect.poll(() => page.evaluate((k) => localStorage.getItem(k), JWT_KEY), { timeout: 15_000 })
    .toBeTruthy();
  await expect.poll(() => new URL(page.url()).pathname).not.toBe('/login');

  // o JWT emitido é válido p/ a sessão (tem 3 segmentos HS256)
  const jwt = await page.evaluate((k) => localStorage.getItem(k), JWT_KEY);
  expect(jwt.split('.').length).toBe(3);

  expect(erros, 'sem exceção de página').toEqual([]);
});

test('passkey login sem credencial → erro amigável (YG-174)', async ({ page, baseURL }) => {
  test.skip(!isLocal(baseURL), 'BASE_URL=http://localhost:3030');
  await page.goto('/login', { waitUntil: 'domcontentloaded' });
  // se o dispositivo de teste suporta WebAuthn, o botão aparece
  if (await page.locator('#btn-passkey').isVisible()) {
    await page.fill('#email', 'ninguem-' + Date.now() + '@x.com');
    await page.locator('#btn-passkey').click();
    await expect(page.locator('#erro-email')).toContainText('Nenhum passkey', { timeout: 10_000 });
  }
});
