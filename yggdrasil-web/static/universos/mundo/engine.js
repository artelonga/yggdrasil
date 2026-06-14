/* mundo/engine.js — motor de mundo 2D walkable (YG-146 protótipo).
 *
 * Theme-agnóstico e data-agnóstico: recebe uma Room (grade + entidades) e um
 * Theme (como desenhar cada coisa) e cuida de movimento (WASD + clique→BFS),
 * câmera que segue o avatar, colisão e o laço de render. Inspiração: Tibia
 * (salas/tiles, andar passo-a-passo) + Gather (zonas de interação ao pisar).
 *
 * Não conhece pastas/notas/NPC: dispara onInteract(entidade) e quem usa decide.
 * Trocar de tema = trocar como desenha; trocar de sala = swap de dados. */

export const TileKind = { FLOOR: 0, WALL: 1 };

export class World {
  constructor(canvas, opts = {}) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.theme = null;
    this.room = null;
    this.onInteract = opts.onInteract || function () {};
    this.onArrive = opts.onArrive || function () {};
    this.onDragDrop = opts.onDragDrop || function () {};
    this.onEdge = opts.onEdge || function () {}; // andar contra a borda → navega hierarquia
    this.drag = null; this.dragTile = null; this._down = null;
    this._edgeLock = false;
    // avatar: posição em grid (alvo) + posição em pixels (interpolada)
    this.ax = 1; this.ay = 1; this.px = 0; this.py = 0;
    this.dir = 'down';
    this.path = [];           // fila de passos {x,y}
    this.moveT = 0;           // progresso 0..1 do passo atual
    this.diag = false;        // passo atual é diagonal? (normaliza velocidade)
    this.held = new Set();    // direções seguradas → movimento contínuo + diagonais
    this.runT = 0;            // tempo correndo contínuo (para aceleração)
    this.fast = false;        // caminho por clique = velocidade alta (quase instantâneo)
    this.BASE = 9.5;          // tiles/s ao começar a segurar
    this.MAX = 16;            // tiles/s no pico da aceleração (segurando)
    this.CLICK = 20;          // tiles/s para caminhos por clique
    this.hover = null;        // {x,y} sob o mouse
    this.cam = { x: 0, y: 0 };
    this.t0 = null;
    this._raf = null;
    this._bind();
  }

  setTheme(theme) { this.theme = theme; this._resize(); }

  setRoom(room, spawn) {
    this.room = room;
    const s = spawn || room.spawn || { x: 1, y: 1 };
    this.ax = s.x; this.ay = s.y;
    const ts = this.theme ? this.theme.tile : 40;
    this.px = s.x * ts; this.py = s.y * ts;
    this.path = []; this.moveT = 0;
    this._resize();
  }

  // ── entidades em (x,y) ──
  entityAt(x, y) {
    const r = this.room; if (!r) return null;
    if (r.exit && r.exit.x === x && r.exit.y === y) return { type: 'exit', _ref: r.exit, ...r.exit };
    for (const d of r.doors || []) if (d.x === x && d.y === y) return { type: 'door', _ref: d, ...d };
    for (const n of r.notes || []) if (n.x === x && n.y === y) return { type: 'note', _ref: n, ...n };
    for (const p of r.npcs || []) if (p.x === x && p.y === y) return { type: 'npc', _ref: p, ...p };
    return null;
  }
  isWall(x, y) {
    const r = this.room; if (!r) return true;
    if (x < 0 || y < 0 || x >= r.w || y >= r.h) return true;
    if (r.tiles[y][x] === TileKind.WALL) return true;
    // NPCs são sólidos (fala-se de lado/clicando); portas/notas são pisáveis
    for (const p of r.npcs || []) if (p.x === x && p.y === y) return true;
    return false;
  }

  // ── BFS para clique→caminho ──
  _bfs(sx, sy, gx, gy) {
    if (sx === gx && sy === gy) return [];
    const r = this.room, key = (x, y) => x + ',' + y;
    const prev = new Map(); prev.set(key(sx, sy), null);
    const q = [[sx, sy]];
    while (q.length) {
      const [x, y] = q.shift();
      for (const [dx, dy] of [[0, -1], [0, 1], [-1, 0], [1, 0]]) {
        const nx = x + dx, ny = y + dy, k = key(nx, ny);
        if (prev.has(k)) continue;
        // permite pisar no alvo mesmo que seja entidade pisável (porta/nota/exit)
        const blocked = this.isWall(nx, ny) && !(nx === gx && ny === gy && !this._solidGoal(gx, gy));
        if (blocked) continue;
        prev.set(k, [x, y]);
        if (nx === gx && ny === gy) {
          const path = []; let cx = nx, cy = ny;
          while (cx !== sx || cy !== sy) { path.unshift({ x: cx, y: cy }); [cx, cy] = prev.get(key(cx, cy)); }
          return path;
        }
        q.push([nx, ny]);
      }
    }
    return null;
  }
  _solidGoal(x, y) { // NPC/wall não são alvos pisáveis
    const r = this.room;
    if (r.tiles[y] && r.tiles[y][x] === TileKind.WALL) return true;
    for (const p of r.npcs || []) if (p.x === x && p.y === y) return true;
    return false;
  }

  goTo(gx, gy) {
    if (!this.room) return;
    // alvo sólido (NPC) → anda até o tile adjacente mais próximo e interage
    if (this._solidGoal(gx, gy)) {
      const adj = [[gx, gy - 1], [gx, gy + 1], [gx - 1, gy], [gx + 1, gy]]
        .filter(([x, y]) => !this.isWall(x, y));
      let best = null, bestLen = 1e9;
      for (const [x, y] of adj) {
        const p = this._bfs(this.ax, this.ay, x, y);
        if (p && p.length < bestLen) { best = p; bestLen = p.length; }
      }
      if (best) { this.path = best; this.fast = true; this.diag = false; this._pendingInteract = { x: gx, y: gy }; }
      else { this.onInteract(this.entityAt(gx, gy)); } // já adjacente
      return;
    }
    const p = this._bfs(this.ax, this.ay, gx, gy);
    if (p && p.length) { this.path = p; this.fast = true; this.diag = false; }
  }

  // vetor a partir das teclas seguradas (suporta diagonais)
  _dirVec() {
    const h = this.held;
    return [
      (h.has('right') ? 1 : 0) - (h.has('left') ? 1 : 0),
      (h.has('down') ? 1 : 0) - (h.has('up') ? 1 : 0),
    ];
  }
  // um passo no sentido das teclas; diagonal cai p/ ortogonal se bloqueada
  _tryStep(dx, dy) {
    if (!dx && !dy) return false;
    this.dir = dy < 0 ? 'up' : dy > 0 ? 'down' : dx < 0 ? 'left' : 'right';
    let tx = this.ax + dx, ty = this.ay + dy, diag = dx !== 0 && dy !== 0;
    if (diag && this.isWall(tx, ty)) {
      if (!this.isWall(this.ax + dx, this.ay)) { dy = 0; ty = this.ay; diag = false; }
      else if (!this.isWall(this.ax, this.ay + dy)) { dx = 0; tx = this.ax; diag = false; }
      else return false;
    }
    if (this._solidGoal(tx, ty)) { this.onInteract(this.entityAt(tx, ty)); return false; }
    // andar contra a BORDA da sala navega a hierarquia (↑ entra, ↓ home, ←→ irmãs)
    if (!diag && this._isBorder(tx, ty)) { this._edge(this.dir); return false; }
    if (this.isWall(tx, ty)) return false;
    this.fast = false; this.diag = diag; this.path = [{ x: tx, y: ty }];
    return true;
  }
  _isBorder(x, y) { const r = this.room; return x <= 0 || y <= 0 || x >= r.w - 1 || y >= r.h - 1; }
  _edge(dir) {
    if (this._edgeLock) return;
    this._edgeLock = true; this.held.clear();
    setTimeout(() => { this._edgeLock = false; }, 350); // evita disparo repetido segurando
    this.onEdge(dir);
  }
  _feedHeld() { if (!this.path.length && this.moveT === 0 && this.held.size) { const [dx, dy] = this._dirVec(); this._tryStep(dx, dy); } }

  // ── laço ──
  _tick(ts) {
    if (this.t0 == null) this.t0 = ts;
    const dt = Math.min(0.05, (ts - this.t0) / 1000); this.t0 = ts;
    const tsz = this.theme.tile;

    this._feedHeld(); // teclado contínuo (segurando) → próximo passo + diagonais

    const moving = this.path.length > 0;
    this.runT = moving ? this.runT + dt : 0; // aceleração ao correr contínuo
    if (moving) {
      const nxt = this.path[0];
      if (!this.diag) this.dir = nxt.y < this.ay ? 'up' : nxt.y > this.ay ? 'down' : nxt.x < this.ax ? 'left' : 'right';
      const accel = Math.min(1, this.runT / 0.4);
      const sp = this.fast ? this.CLICK : this.BASE + (this.MAX - this.BASE) * accel;
      this.moveT += dt * sp * (this.diag ? 0.7071 : 1); // diagonal anda dist=√2 → normaliza
      const fromX = this.ax * tsz, fromY = this.ay * tsz;
      const e = Math.min(1, this.moveT);
      this.px = fromX + (nxt.x * tsz - fromX) * e;
      this.py = fromY + (nxt.y * tsz - fromY) * e;
      if (this.moveT >= 1) {
        this.ax = nxt.x; this.ay = nxt.y; this.moveT = 0; this.path.shift();
        this.px = this.ax * tsz; this.py = this.ay * tsz;
        if (!this.path.length) {
          this.diag = false;
          if (this._pendingInteract) { const pi = this._pendingInteract; this._pendingInteract = null; this.onInteract(this.entityAt(pi.x, pi.y)); }
          else { const e2 = this.entityAt(this.ax, this.ay); this.onArrive(this.ax, this.ay); if (e2) this.onInteract(e2); }
        }
      }
    } else {
      this.px = this.ax * tsz; this.py = this.ay * tsz;
    }
    this._render(ts / 1000);
    this._raf = requestAnimationFrame((t) => this._tick(t));
  }

  start() { if (!this._raf) this._raf = requestAnimationFrame((t) => this._tick(t)); }
  stop() { if (this._raf) cancelAnimationFrame(this._raf); this._raf = null; this.t0 = null; }

  _resize() {
    const dpr = window.devicePixelRatio || 1;
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    this.canvas.width = w * dpr; this.canvas.height = h * dpr;
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.vw = w; this.vh = h;
  }

  _render(time) {
    const { ctx, room, theme } = this; if (!room || !theme) return;
    const tsz = theme.tile;
    // câmera segue avatar, clampada à sala (ou centraliza se sala < viewport)
    const cx = this.px + tsz / 2, cy = this.py + tsz / 2;
    const roomW = room.w * tsz, roomH = room.h * tsz;
    let camX = cx - this.vw / 2, camY = cy - this.vh / 2;
    camX = roomW <= this.vw ? (roomW - this.vw) / 2 : Math.max(0, Math.min(camX, roomW - this.vw));
    camY = roomH <= this.vh ? (roomH - this.vh) / 2 : Math.max(0, Math.min(camY, roomH - this.vh));
    this.cam.x = camX; this.cam.y = camY;

    ctx.save();
    ctx.translate(-camX, -camY);
    if (theme.bg) theme.bg(ctx, { x: camX, y: camY, w: this.vw, h: this.vh, roomW, roomH }, time);

    // chão + paredes
    for (let y = 0; y < room.h; y++) {
      for (let x = 0; x < room.w; x++) {
        const px = x * tsz, py = y * tsz;
        if (room.tiles[y][x] === TileKind.WALL) theme.wall(ctx, px, py, tsz, x, y, room);
        else theme.floor(ctx, px, py, tsz, x, y);
      }
    }
    // exit, portas, notas, npcs, avatar
    const hov = (e) => this.hover && this.hover.x === e.x && this.hover.y === e.y;
    if (room.exit) theme.exit(ctx, room.exit.x * tsz, room.exit.y * tsz, tsz, hov(room.exit), time);
    for (const d of room.doors || []) theme.door(ctx, d.x * tsz, d.y * tsz, tsz, d, hov(d), time);
    for (const n of room.notes || []) theme.note(ctx, n.x * tsz, n.y * tsz, tsz, n, hov(n), time);
    for (const p of room.npcs || []) theme.npc(ctx, p.x * tsz, p.y * tsz, tsz, p, hov(p), time);
    theme.avatar(ctx, this.px, this.py, tsz, this.dir, !!this.path.length, time);

    // alvo de arrasto (arrastar nota/pasta p/ reposicionar ou soltar em pasta)
    if (this.drag && this.dragTile) {
      const dx = this.dragTile.x * tsz, dy = this.dragTile.y * tsz;
      const onDoor = (room.doors || []).some((d) => d.x === this.dragTile.x && d.y === this.dragTile.y);
      ctx.save();
      ctx.fillStyle = onDoor ? 'rgba(120,255,160,.22)' : 'rgba(100,200,255,.18)';
      ctx.fillRect(dx, dy, tsz, tsz);
      ctx.strokeStyle = onDoor ? '#7fe0a8' : '#6cf'; ctx.lineWidth = 2; ctx.setLineDash([5, 4]);
      ctx.strokeRect(dx + 2, dy + 2, tsz - 4, tsz - 4); ctx.setLineDash([]);
      ctx.restore();
    }

    if (theme.ambiance) theme.ambiance(ctx, { x: camX, y: camY, w: this.vw, h: this.vh }, time, room);
    ctx.restore();

    // rótulos das entidades sob o mouse (HUD simples, fora do translate)
    if (this.hover) {
      const e = this.entityAt(this.hover.x, this.hover.y);
      if (e && (e.label || e.title || e.name)) {
        const sx = this.hover.x * tsz - camX + tsz / 2, sy = this.hover.y * tsz - camY - 6;
        const txt = e.label || e.title || e.name;
        ctx.font = '600 12px system-ui, sans-serif'; ctx.textAlign = 'center';
        const w = ctx.measureText(txt).width + 14;
        ctx.fillStyle = 'rgba(0,0,0,.78)';
        roundRect(ctx, sx - w / 2, sy - 18, w, 18, 5); ctx.fill();
        ctx.fillStyle = '#fff'; ctx.fillText(txt, sx, sy - 5);
      }
    }
  }

  _screenToTile(clientX, clientY) {
    const rect = this.canvas.getBoundingClientRect();
    const x = clientX - rect.left + this.cam.x;
    const y = clientY - rect.top + this.cam.y;
    const tsz = this.theme.tile;
    return { x: Math.floor(x / tsz), y: Math.floor(y / tsz) };
  }

  _keyDir(k) {
    if (['ArrowUp', 'w', 'W'].includes(k)) return 'up';
    if (['ArrowDown', 's', 'S'].includes(k)) return 'down';
    if (['ArrowLeft', 'a', 'A'].includes(k)) return 'left';
    if (['ArrowRight', 'd', 'D'].includes(k)) return 'right';
    return null;
  }
  _bind() {
    window.addEventListener('keydown', (e) => {
      if (isTyping()) return; // não sequestra WASD/Enter quando digitando num campo
      const dir = this._keyDir(e.key);
      if (dir) { this.held.add(dir); e.preventDefault(); this._feedHeld(); /* resposta instantânea */ }
      else if (e.key === 'Enter') { const en = this.entityAt(this.ax, this.ay); if (en) this.onInteract(en); }
    });
    window.addEventListener('keyup', (e) => { const dir = this._keyDir(e.key); if (dir) this.held.delete(dir); });
    window.addEventListener('blur', () => this.held.clear());

    this.canvas.addEventListener('mousemove', (e) => {
      const t = this._screenToTile(e.clientX, e.clientY);
      this.hover = t;
      if (this._down && !this.drag) {
        const dist = Math.abs(e.clientX - this._down.sx) + Math.abs(e.clientY - this._down.sy);
        if (dist > 6) this.drag = { ent: this._down.ent }; // virou arrasto
      }
      if (this.drag) this.dragTile = t;
    });
    this.canvas.addEventListener('mouseleave', () => { this.hover = null; });
    this.canvas.addEventListener('mousedown', (e) => {
      const t = this._screenToTile(e.clientX, e.clientY);
      const ent = this.entityAt(t.x, t.y);
      // arrastável: nota ou porta (pasta). NPC/exit não arrastam.
      this._down = (ent && (ent.type === 'note' || ent.type === 'door'))
        ? { ent, sx: e.clientX, sy: e.clientY } : null;
    });
    window.addEventListener('mouseup', (e) => {
      if (this.drag && this.dragTile) {
        const to = this.dragTile, target = this.entityAt(to.x, to.y);
        this.onDragDrop(this.drag.ent, to.x, to.y, target);
        this.drag = null; this.dragTile = null; this._down = null; return;
      }
      const wasDown = this._down; this._down = null; this.drag = null;
      // clique simples (sem arrasto) → andar até o tile
      const t = this._screenToTile(e.clientX, e.clientY);
      const rect = this.canvas.getBoundingClientRect();
      if (e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom) {
        this.goTo(t.x, t.y);
      }
      void wasDown;
    });
    window.addEventListener('resize', () => this._resize());
  }
}

export function isTyping() {
  const a = document.activeElement;
  return !!a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.isContentEditable);
}

export function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}
