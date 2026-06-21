// Regressão YG-167: um JWT velho/expirado no localStorage NÃO pode prender o
// usuário num loop /login ↔ /universos/comunicacao numa sala PÚBLICA.
// Antes: o GET /revisao no boot (token inválido) → 401 → redirect a /login →
// volta → 401 → ... Agora: 401 limpa o token e segue anônimo (só escrita re-loga).
const { test, expect } = require('@playwright/test');

const JWT_KEY = 'yggdrasil-jwt';
// JWT sintaticamente válido mas com assinatura/segredo errados → backend dá 401.
const STALE = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.' +
  'eyJzdWIiOiJzdGFsZSIsImV4cCI6OTk5OTk5OTk5OX0.' + 'assinatura-invalida-aqui';

test('sala pública com token velho não entra em loop de /login (YG-167)', async ({ page }) => {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));

  // injeta um token inválido antes de qualquer script da página rodar
  await page.addInitScript(([k, t]) => localStorage.setItem(k, t), [JWT_KEY, STALE]);

  await page.goto('/universos/comunicacao?id=public-mbya', { waitUntil: 'domcontentloaded' });

  // dá tempo do boot rodar (loadRoom + refreshReviewCount, que dispara o 401)
  await page.waitForTimeout(1500);

  // NÃO redirecionou para /login — continua na página da comunicação
  expect(page.url()).not.toContain('/login');
  expect(new URL(page.url()).pathname).toBe('/universos/comunicacao');

  // a toolbar (página viva) renderizou
  await expect(page.locator('#toolbar')).toBeVisible();

  // o token inválido foi limpo (não vai 401 de novo na próxima chamada)
  const tok = await page.evaluate((k) => localStorage.getItem(k), JWT_KEY);
  expect(tok, 'token inválido deve ser removido').toBeNull();

  expect(erros, 'sem exceção de página').toEqual([]);
});
