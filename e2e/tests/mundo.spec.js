// E2E do Mundo (YG-149) — manipulação direta vira estado durável: arrastar um
// objeto persiste a posição (reabrir mantém o layout) e arrastar pra outra pasta
// reparenta a nota. Dirige o MESMO caminho de drag-drop do mouse via window.__mundo
// (evita a matemática frágil de pixels do canvas).
const { test, expect } = require('@playwright/test');

function guardErrors(page) {
  const erros = [];
  page.on('pageerror', (e) => erros.push(String(e)));
  return erros;
}

async function bootMundo(page) {
  await page.goto('/mundo');
  await page.waitForFunction(() => window.__mundo && window.__mundo.ready, { timeout: 10_000 });
}

test('Mundo: arrastar objeto → recarregar → posição mantida (YG-149)', async ({ page }) => {
  const erros = guardErrors(page);
  await bootMundo(page);

  // posição inicial da nota "bem-vindo" (sala raiz)
  const antes = await page.evaluate(() => window.__mundo.posOf('bem-vindo'));
  expect(antes).not.toBeNull();

  // arrasta pra um tile livre (5,4) na raiz — commit em lote ao soltar
  const ok = await page.evaluate(() => window.__mundo.drag('bem-vindo', 5, 4));
  expect(ok).toBe(true);
  const depois = await page.evaluate(() => window.__mundo.posOf('bem-vindo'));
  expect(depois).toMatchObject({ room: 'raiz', x: 5, y: 4 });
  expect(depois.x === antes.x && depois.y === antes.y).toBe(false); // moveu mesmo

  // recarrega → o layout persistido é reaplicado (posição = estado durável)
  await bootMundo(page);
  const recarregado = await page.evaluate(() => window.__mundo.posOf('bem-vindo'));
  expect(recarregado).toMatchObject({ room: 'raiz', x: 5, y: 4 });

  expect(erros).toEqual([]);
});

test('Mundo: reparent (arrastar pra outra pasta) → nota na nova pasta após reload (YG-149)', async ({ page }) => {
  const erros = guardErrors(page);
  await bootMundo(page);

  // "sobre" começa na raiz; arrasta pra pasta "jardim"
  const inicio = await page.evaluate(() => window.__mundo.posOf('sobre'));
  expect(inicio.room).toBe('raiz');
  const ok = await page.evaluate(() => window.__mundo.reparent('sobre', 'jardim'));
  expect(ok).toBe(true);
  const movida = await page.evaluate(() => window.__mundo.posOf('sobre'));
  expect(movida.room).toBe('jardim');

  // recarrega → a nota continua na nova pasta (árvore reparentada persistiu)
  await bootMundo(page);
  const recarregada = await page.evaluate(() => window.__mundo.posOf('sobre'));
  expect(recarregada.room).toBe('jardim');

  expect(erros).toEqual([]);
});
