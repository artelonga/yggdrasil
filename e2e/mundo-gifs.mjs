// Gera GIFs das jornadas-chave do /mundo (YG-150) via Playwright + ffmpeg.
//
// O protótipo do Mundo é 100% client-side (dados mock em sample.js; chamadas de
// backend — NPC/feedback/telemetria — têm fallback), então gravamos as jornadas
// contra um servidor estático mínimo, sem subir o workspace Rust inteiro.
//
//   cd e2e && npm install && npm run gifs
//
// Cada jornada vira um <context> com recordVideo → .webm, convertido em GIF
// otimizado (palette 2-pass, 600px, 10fps) em docs/mundo/. Reaproveita o
// chromium e o ffmpeg que o Playwright já baixa (sem deps extras).
import { chromium } from '@playwright/test';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { readFile, mkdir, rm, readdir } from 'node:fs/promises';
import { existsSync, readdirSync, rmSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import gifenc from 'gifenc';
import pngjs from 'pngjs';
const { GIFEncoder, quantize, applyPalette } = gifenc;
const { PNG } = pngjs;
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join, extname } from 'node:path';
import { homedir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const STATIC = join(REPO, 'yggdrasil-web', 'static');
const OUT = join(REPO, 'docs', 'mundo');
const TMP = join(HERE, '.videos');

const TILE = 44; // tema padrão (medieval-castle) — geometria da sala raiz
const ROOM_W = 15, ROOM_H = 11; // sala raiz (sample.js) — menor que a viewport → câmera centraliza

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
};

// Servidor estático mínimo: /mundo → mundo-proto.html, /static/* → static/*.
function startServer() {
  const server = createServer(async (req, res) => {
    const pathname = decodeURIComponent((req.url || '/').split('?')[0]);
    let rel;
    if (pathname === '/mundo' || pathname === '/mundo/') rel = 'universos/mundo-proto.html';
    else if (pathname.startsWith('/static/')) rel = pathname.slice('/static/'.length);
    else if (pathname === '/') rel = 'universos/mundo-proto.html';
    else { res.statusCode = 404; res.end('nope'); return; }
    const file = join(STATIC, rel);
    if (!file.startsWith(STATIC)) { res.statusCode = 403; res.end('no'); return; }
    try {
      const buf = await readFile(file);
      res.setHeader('Content-Type', MIME[extname(file)] || 'application/octet-stream');
      res.end(buf);
    } catch {
      res.statusCode = 404; res.end('404');
    }
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok({ server, port: server.address().port })));
}

// Acha o ffmpeg: $FFMPEG → PATH → binário que o Playwright já baixou.
function resolveFfmpeg() {
  if (process.env.FFMPEG && existsSync(process.env.FFMPEG)) return process.env.FFMPEG;
  const sys = spawnSync('ffmpeg', ['-version']);
  if (sys.status === 0) return 'ffmpeg';
  const cache = join(homedir(), 'Library', 'Caches', 'ms-playwright');
  if (existsSync(cache)) {
    for (const d of readdirSync(cache)) {
      if (!d.startsWith('ffmpeg-')) continue;
      for (const bin of ['ffmpeg-mac', 'ffmpeg-linux', 'ffmpeg.exe']) {
        const p = join(cache, d, bin);
        if (existsSync(p)) return p;
      }
    }
  }
  throw new Error('ffmpeg não encontrado — instale ffmpeg ou rode `npx playwright install` antes.');
}

// O ffmpeg que vem com o Playwright é um build enxuto (--disable-everything):
// tem scale/png/image2, mas NÃO tem palettegen/fps nem muxer de gif. Então
// usamos ele só pra extrair frames PNG já reduzidos e montamos o GIF com gifenc
// (paleta global → cores estáveis + arquivo menor). Sem dep de ffmpeg do sistema.
function webmToGif(ffmpeg, webm, gif, { width = 480, fps = 8, colors = 64 } = {}) {
  const framesDir = webm + '-frames';
  mkdirSync(framesDir, { recursive: true });
  const r = spawnSync(ffmpeg, ['-y', '-i', webm, '-vf', `scale=${width}:-1`, '-r', String(fps),
    join(framesDir, 'f-%04d.png')]);
  if (r.status !== 0) throw new Error('extração de frames falhou:\n' + r.stderr);
  const files = readdirSync(framesDir).filter((f) => f.endsWith('.png')).sort();
  if (!files.length) throw new Error('nenhum frame extraído de ' + webm);
  const enc = GIFEncoder();
  const delay = Math.round(1000 / fps);
  let palette = null;
  for (const f of files) {
    const png = PNG.sync.read(readFileSync(join(framesDir, f)));
    if (!palette) palette = quantize(png.data, colors); // paleta global do 1º frame
    const index = applyPalette(png.data, palette);
    enc.writeFrame(index, png.width, png.height, { palette, delay });
  }
  enc.finish();
  writeFileSync(gif, enc.bytes());
  rmSync(framesDir, { recursive: true, force: true });
}

// Centro (px de página) do tile (tx,ty): a sala raiz é menor que a viewport,
// então a câmera centraliza a sala dentro do canvas.
function tileCenter(box, tx, ty) {
  const offX = (box.width - ROOM_W * TILE) / 2;
  const offY = (box.height - ROOM_H * TILE) / 2;
  return { x: box.x + offX + tx * TILE + TILE / 2, y: box.y + offY + ty * TILE + TILE / 2 };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function steps(page, key, n, gap = 300) {
  for (let i = 0; i < n; i++) { await page.keyboard.press(key); await sleep(gap); }
}

// ── jornadas ────────────────────────────────────────────────────────────────
// Cada jornada recebe a page já em /mundo (tema padrão) e executa as ações.
const JORNADAS = {
  // 1. Andar: caminho em L pela sala raiz (longe de entidades/bordas).
  async andar(page) {
    await page.locator('header h1').click(); // foca a página p/ o teclado
    await sleep(500);
    await steps(page, 'a', 4); // ← esquerda
    await steps(page, 'w', 4); // ↑ cima
    await steps(page, 'd', 3); // → direita
    await sleep(600);
  },

  // 2. Entrar numa sala: pisar na porta "Jardim" (pasta = sala) → entra.
  async entrar_sala(page) {
    await page.locator('header h1').click();
    await sleep(500);
    await steps(page, 'a', 3); // até a coluna da porta
    await steps(page, 'w', 6); // sobe e pisa na porta {4,2}
    await page.waitForFunction(
      () => document.querySelector('#trilha .crumb.on')?.textContent?.includes('Jardim'),
      { timeout: 4000 },
    ).catch(() => {});
    await sleep(1200); // mostra a sala nova + trilha atualizada
  },

  // 3. Abrir/editar nota: pelo menu lateral (árvore viva) — abre, edita, salva.
  async abrir_editar_nota(page) {
    await sleep(500);
    await page.locator('#arvore .t-note').first().click(); // abre o painel da nota
    await page.waitForSelector('#panel.open', { timeout: 4000 });
    await sleep(700);
    await page.locator('#nota-edit').click(); // ✏️ Editar → textarea
    await page.waitForSelector('#nota-ta', { timeout: 4000 });
    await sleep(400);
    await page.locator('#nota-ta').click();
    await page.keyboard.press('End');
    await page.keyboard.type('  — editado no tour ✍️', { delay: 45 });
    await sleep(400);
    await page.locator('#nota-edit').click(); // 💾 Salvar → toast
    await page.waitForSelector('#toast.show', { timeout: 4000 }).catch(() => {});
    await sleep(900);
  },

  // 4. Drag-drop reposiciona: arrasta a nota "Bem-vindo" {7,4} → tile livre {5,6}.
  async drag_drop(page) {
    await sleep(500);
    const box = await page.locator('#cv').boundingBox();
    const from = tileCenter(box, 7, 4);
    const to = tileCenter(box, 5, 6);
    await page.mouse.move(from.x, from.y);
    await sleep(250);
    await page.mouse.down();
    const N = 8;
    for (let i = 1; i <= N; i++) {
      await page.mouse.move(from.x + (to.x - from.x) * i / N, from.y + (to.y - from.y) * i / N);
      await sleep(70); // deixa o destaque tracejado renderizar quadro a quadro
    }
    await page.mouse.up();
    await page.waitForSelector('#toast.show', { timeout: 4000 }).catch(() => {});
    await sleep(900);
  },
};

async function main() {
  const ffmpeg = resolveFfmpeg();
  await rm(TMP, { recursive: true, force: true });
  await mkdir(TMP, { recursive: true });
  await mkdir(OUT, { recursive: true });
  const { server, port } = await startServer();
  const base = `http://127.0.0.1:${port}`;
  const browser = await chromium.launch();
  try {
    for (const [nome, jornada] of Object.entries(JORNADAS)) {
      const videoDir = join(TMP, nome);
      const ctx = await browser.newContext({
        viewport: { width: 1280, height: 800 },
        recordVideo: { dir: videoDir, size: { width: 1280, height: 800 } },
      });
      const page = await ctx.newPage();
      await page.goto(base + '/mundo', { waitUntil: 'networkidle' });
      await page.waitForSelector('#cv', { timeout: 8000 });
      await sleep(600); // primeira pintura do canvas
      await jornada(page);
      if (process.env.VERIFY) await page.screenshot({ path: join('/tmp', `mundo-verify-${nome}.png`) });
      const video = page.video();
      await ctx.close(); // finaliza o .webm
      const webm = await video.path();
      const gif = join(OUT, `${nome}.gif`);
      webmToGif(ffmpeg, webm, gif);
      const kb = Math.round((await readFile(gif)).length / 1024);
      console.log(`✓ ${nome}.gif (${kb} KB)`);
    }
  } finally {
    await browser.close();
    server.close();
    await rm(TMP, { recursive: true, force: true });
  }
  const gifs = (await readdir(OUT)).filter((f) => f.endsWith('.gif'));
  console.log(`\n${gifs.length} GIFs em docs/mundo/: ${gifs.join(', ')}`);
}

main().catch((e) => { console.error(e); process.exit(1); });
