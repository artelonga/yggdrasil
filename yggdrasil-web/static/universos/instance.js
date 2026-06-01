// Player + editor genérico de instâncias de universo (YG-78 + YG-81).
//
// Renderiza uma UniverseInstance client-side: camadas z-ordenadas com opacity,
// projeção 2d/iso, blocos e conexões. Sem WASM — é dado estático com toggles.
// Em modo edição (dono autenticado) aplica EditOps via PATCH.

'use strict';

const JWT_KEY = 'yggdrasil-jwt';
const API = '/api/v1';

const state = {
  id: location.pathname.split('/').pop(),
  token: localStorage.getItem(JWT_KEY),
  me: null,
  inst: null,
  template: null,
  edit: false,
  selectedType: null,   // block_type da paleta
  selectedBlock: null,  // id do bloco selecionado
  connectFrom: null,    // origem pendente de conexão
  deleteMode: false,
  images: {},           // hash -> HTMLImageElement (object URLs)
  drag: null,           // { blockId, layerId }
};

const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

function authHeaders() {
  return state.token ? { Authorization: `Bearer ${state.token}` } : {};
}

function decodeSub(token) {
  try { return JSON.parse(atob(token.split('.')[1])).sub; } catch { return null; }
}

function toast(msg) {
  const t = document.getElementById('toast');
  t.textContent = msg;
  t.classList.add('show');
  setTimeout(() => t.classList.remove('show'), 2200);
}

// ─── Carregamento ────────────────────────────────────────────────────────────

async function load() {
  if (state.token) state.me = decodeSub(state.token);
  const res = await fetch(`${API}/instances/${state.id}`, { headers: authHeaders() });
  if (!res.ok) {
    document.getElementById('title').textContent = res.status === 404
      ? 'Universo não encontrado'
      : 'Acesso negado';
    return;
  }
  state.inst = await res.json();
  document.getElementById('title').textContent = state.inst.title || 'Universo';
  document.getElementById('desc').textContent = state.inst.description || '';

  // dono autenticado vê o botão de edição
  if (state.me && state.me === state.inst.owner) {
    document.getElementById('modeBtn').hidden = false;
  }
  if (state.inst.template) {
    try {
      const tr = await fetch(`${API}/templates/${state.inst.template}`);
      if (tr.ok) state.template = await tr.json();
    } catch { /* paleta opcional */ }
  }

  await loadImages();
  sizeCanvas();
  renderLayers();
  renderPalette();
  render();
}

async function loadImages() {
  const hashes = new Set();
  for (const l of state.inst.layers) {
    if (l.background) hashes.add(l.background.hash);
  }
  await Promise.all([...hashes].map(async (hash) => {
    try {
      const r = await fetch(`${API}/instances/${state.id}/attachments/${hash}`, { headers: authHeaders() });
      if (!r.ok) return;
      const blob = await r.blob();
      const img = new Image();
      img.src = URL.createObjectURL(blob);
      await img.decode().catch(() => {});
      state.images[hash] = img;
    } catch { /* ignora imagem faltante */ }
  }));
}

// ─── Geometria ───────────────────────────────────────────────────────────────

function gridSpec() { return state.inst.grid; }

function isIso() { return state.inst.projection === 'isometric'; }

function cellToScreen(x, y) {
  const g = gridSpec();
  const c = g.cell_size;
  if (isIso()) {
    const originX = g.height * c / 2;
    return { sx: (x - y) * (c / 2) + originX, sy: (x + y) * (c / 4) + c };
  }
  return { sx: x * c, sy: y * c };
}

function cellCenter(x, y) {
  const c = gridSpec().cell_size;
  const p = cellToScreen(x, y);
  return isIso() ? { cx: p.sx, cy: p.sy + c / 4 } : { cx: p.sx + c / 2, cy: p.sy + c / 2 };
}

function screenToCell(mx, my) {
  const g = gridSpec();
  const c = g.cell_size;
  if (isIso()) {
    const originX = g.height * c / 2;
    const ix = (mx - originX) / (c / 2);
    const iy = (my - c) / (c / 4);
    const x = Math.round((ix + iy) / 2);
    const y = Math.round((iy - ix) / 2);
    return { x, y };
  }
  return { x: Math.floor(mx / c), y: Math.floor(my / c) };
}

function sizeCanvas() {
  const g = gridSpec();
  const c = g.cell_size;
  if (isIso()) {
    canvas.width = (g.width + g.height) * (c / 2);
    canvas.height = (g.width + g.height) * (c / 4) + 2 * c;
  } else {
    canvas.width = g.width * c;
    canvas.height = g.height * c;
  }
}

// ─── Render ──────────────────────────────────────────────────────────────────

function allBlocks() {
  const out = [];
  for (const l of state.inst.layers) {
    if (l.kind === 'background') continue;
    for (const b of l.blocks) out.push({ block: b, layer: l });
  }
  return out;
}

function findBlock(id) {
  for (const l of state.inst.layers) {
    const b = l.blocks.find((b) => b.id === id);
    if (b) return { block: b, layer: l };
  }
  return null;
}

function render() {
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const g = gridSpec();
  const c = g.cell_size;

  // camadas em z-order (índice 0 = base)
  for (const l of state.inst.layers) {
    if (!l.visible) continue;
    ctx.globalAlpha = l.opacity;
    if (l.kind === 'background' && l.background && state.images[l.background.hash]) {
      const img = state.images[l.background.hash];
      // cobre a área da grade preservando proporção
      drawCover(img, 0, 0, g.width * c, g.height * c);
    }
  }
  ctx.globalAlpha = 1;

  if (!isIso() && (!state.template || state.template.render_hints?.grid_lines)) drawGridLines();

  // conexões (sob os blocos)
  for (const conn of state.inst.connections) {
    const a = findBlock(conn.from), b = findBlock(conn.to);
    if (!a || !b) continue;
    const p = cellCenter(a.block.pos.x, a.block.pos.y);
    const q = cellCenter(b.block.pos.x, b.block.pos.y);
    drawEdge(p, q, conn.directed, conn.label);
  }

  // blocos
  for (const { block } of allBlocks()) drawBlock(block);
}

function drawCover(img, dx, dy, dw, dh) {
  const ir = img.width / img.height, dr = dw / dh;
  let w = dw, h = dh, ox = dx, oy = dy;
  if (ir > dr) { h = dh; w = dh * ir; ox = dx + (dw - w) / 2; }
  else { w = dw; h = dw / ir; oy = dy + (dh - h) / 2; }
  ctx.drawImage(img, ox, oy, w, h);
}

function drawGridLines() {
  const g = gridSpec(), c = g.cell_size;
  ctx.strokeStyle = '#15151f';
  ctx.lineWidth = 1;
  for (let x = 0; x <= g.width; x++) {
    ctx.beginPath(); ctx.moveTo(x * c, 0); ctx.lineTo(x * c, g.height * c); ctx.stroke();
  }
  for (let y = 0; y <= g.height; y++) {
    ctx.beginPath(); ctx.moveTo(0, y * c); ctx.lineTo(g.width * c, y * c); ctx.stroke();
  }
}

function drawBlock(b) {
  const c = gridSpec().cell_size;
  const { cx, cy } = cellCenter(b.pos.x, b.pos.y);
  const r = c * 0.45;
  const color = b.props?.color || '#d4af37';
  const icon = b.props?.icon || '■';
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.fillStyle = color + '33';
  ctx.fill();
  ctx.lineWidth = b.id === state.selectedBlock ? 3 : 1.5;
  ctx.strokeStyle = b.id === state.selectedBlock ? '#fff' : color;
  ctx.stroke();
  ctx.fillStyle = '#fff';
  ctx.font = `${Math.round(c * 0.7)}px system-ui`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(icon, cx, cy);
  if (b.label) {
    ctx.font = `${Math.round(c * 0.42)}px system-ui`;
    ctx.fillStyle = '#e8e3d3';
    ctx.fillText(b.label, cx, cy + r + c * 0.35);
  }
}

function drawEdge(p, q, directed, label) {
  ctx.strokeStyle = '#7ec8e3aa';
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(p.cx, p.cy);
  ctx.lineTo(q.cx, q.cy);
  ctx.stroke();
  if (directed) {
    const ang = Math.atan2(q.cy - p.cy, q.cx - p.cx);
    const ah = 8;
    ctx.beginPath();
    ctx.moveTo(q.cx, q.cy);
    ctx.lineTo(q.cx - ah * Math.cos(ang - 0.4), q.cy - ah * Math.sin(ang - 0.4));
    ctx.lineTo(q.cx - ah * Math.cos(ang + 0.4), q.cy - ah * Math.sin(ang + 0.4));
    ctx.closePath();
    ctx.fillStyle = '#7ec8e3aa';
    ctx.fill();
  }
  if (label) {
    ctx.fillStyle = '#7ec8e3';
    ctx.font = '10px system-ui';
    ctx.textAlign = 'center';
    ctx.fillText(label, (p.cx + q.cx) / 2, (p.cy + q.cy) / 2 - 4);
  }
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

function renderLayers() {
  const el = document.getElementById('layers');
  el.innerHTML = '';
  // mostra topo→base na UI
  [...state.inst.layers].reverse().forEach((l) => {
    const row = document.createElement('div');
    row.className = 'layer-row';
    const vis = document.createElement('input');
    vis.type = 'checkbox'; vis.checked = l.visible;
    vis.onchange = () => editLayer(l.id, { visible: vis.checked });
    const name = document.createElement('label');
    name.textContent = l.name;
    const op = document.createElement('input');
    op.type = 'range'; op.min = 0; op.max = 1; op.step = 0.05; op.value = l.opacity;
    op.title = 'Transparência';
    op.oninput = () => { l.opacity = parseFloat(op.value); render(); };
    op.onchange = () => editLayer(l.id, { opacity: parseFloat(op.value) });
    row.append(vis, name, op);
    el.append(row);
  });
}

function renderPalette() {
  const el = document.getElementById('palette');
  el.innerHTML = '';
  const items = state.template?.palette || [{ block_type: 'note', label: 'Nota', default_props: { icon: '📝' } }];
  items.forEach((it) => {
    const d = document.createElement('div');
    d.className = 'palette-item' + (state.selectedType === it.block_type ? ' sel' : '');
    d.innerHTML = `<span class="ico">${it.default_props?.icon || '■'}</span> ${it.label}`;
    d.onclick = () => {
      state.selectedType = state.selectedType === it.block_type ? null : it.block_type;
      state.deleteMode = false;
      renderPalette();
    };
    el.append(d);
  });
}

function showInspector(b) {
  const el = document.getElementById('inspector');
  if (!b) { el.innerHTML = '<p class="hint">Clique num bloco para ver seu conteúdo.</p>'; return; }
  el.innerHTML = `<strong>${b.label || b.block_type}</strong>`;
  for (const a of (b.attachments || [])) {
    const div = document.createElement('div');
    div.className = 'att';
    const url = `${API}/instances/${state.id}/attachments/${a.hash}`;
    if (a.kind === 'image') {
      div.innerHTML = `<img alt="${a.filename}" src="${url}">`;
    } else if (a.kind === 'pdf') {
      div.innerHTML = `<a href="${url}" target="_blank">📄 ${a.filename}</a>`;
    } else if (a.kind === 'sound') {
      div.innerHTML = `<audio controls src="${url}"></audio>`;
    } else {
      div.innerHTML = `<a href="${url}" target="_blank">${a.filename}</a>`;
    }
    if (a.attribution || b.props?.attribution) {
      const credit = document.createElement('div');
      credit.className = 'credit';
      credit.textContent = 'Fonte: ' + (a.attribution || b.props.attribution);
      div.append(credit);
    }
    el.append(div);
  }
  if (state.edit) {
    const up = document.createElement('button');
    up.textContent = '📎 Anexar arquivo';
    up.style.marginTop = '0.6rem';
    up.onclick = () => triggerUpload(b.id);
    el.append(up);
  }
}

// ─── Edição (EditOps via PATCH) ──────────────────────────────────────────────

async function patch(op) {
  const res = await fetch(`${API}/instances/${state.id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(op),
  });
  if (!res.ok) {
    let msg = 'Operação inválida';
    try { msg = (await res.json()).erro || msg; } catch {}
    toast('⚠ ' + msg);
    await load(); // reconcilia estado em erro
    return null;
  }
  state.inst = await res.json();
  renderLayers();
  render();
  return state.inst;
}

function editLayer(id, fields) {
  return patch({ op: 'edit_layer', layer: id, ...fields });
}

function targetBlocksLayer() {
  const l = state.inst.layers.find((l) => l.kind === 'blocks');
  return l ? l.id : (state.inst.layers[0] && state.inst.layers[0].id);
}

async function placeBlock(cell) {
  const layer = targetBlocksLayer();
  const item = (state.template?.palette || []).find((p) => p.block_type === state.selectedType);
  const id = `${state.selectedType}-${Date.now().toString(36)}`;
  await patch({
    op: 'place_block',
    layer,
    block: {
      id,
      block_type: state.selectedType,
      pos: { x: cell.x, y: cell.y },
      props: item?.default_props || {},
    },
  });
}

async function moveBlock(blockId, layerId, cell) {
  await patch({ op: 'move_block', layer: layerId, block_id: blockId, to: { x: cell.x, y: cell.y } });
}

async function deleteBlock(blockId, layerId) {
  await patch({ op: 'delete_block', layer: layerId, block_id: blockId });
  state.selectedBlock = null;
  showInspector(null);
}

async function addConnection(from, to) {
  await patch({
    op: 'add_connection',
    connection: { id: `c-${Date.now().toString(36)}`, from, to, directed: true },
  });
}

// ─── Upload de anexos ────────────────────────────────────────────────────────

let uploadTargetBlock = null;
function triggerUpload(blockId) {
  uploadTargetBlock = blockId;
  document.getElementById('upload').click();
}

document.getElementById('upload').addEventListener('change', async (e) => {
  const file = e.target.files[0];
  e.target.value = '';
  if (!file || !uploadTargetBlock) return;
  const fd = new FormData();
  fd.append('file', file);
  const res = await fetch(`${API}/instances/${state.id}/attachments`, {
    method: 'POST', headers: authHeaders(), body: fd,
  });
  if (!res.ok) {
    let msg = 'Upload falhou';
    try { msg = (await res.json()).erro || msg; } catch {}
    toast('⚠ ' + msg); return;
  }
  const cref = await res.json();
  const hit = findBlock(uploadTargetBlock);
  await patch({ op: 'attach_content', layer: hit.layer.id, block_id: uploadTargetBlock, content: cref });
  await loadImages();
  const again = findBlock(uploadTargetBlock);
  showInspector(again.block);
  render();
  toast('Anexo adicionado');
});

// ─── Interação no canvas ─────────────────────────────────────────────────────

function blockAt(mx, my) {
  const c = gridSpec().cell_size;
  for (const { block } of allBlocks()) {
    const { cx, cy } = cellCenter(block.pos.x, block.pos.y);
    if (Math.hypot(mx - cx, my - cy) <= c * 0.5) return block;
  }
  return null;
}

function mouseXY(e) {
  const r = canvas.getBoundingClientRect();
  return { mx: (e.clientX - r.left) * (canvas.width / r.width), my: (e.clientY - r.top) * (canvas.height / r.height) };
}

canvas.addEventListener('mousedown', (e) => {
  const { mx, my } = mouseXY(e);
  const hit = blockAt(mx, my);

  if (!state.edit) {
    state.selectedBlock = hit ? hit.id : null;
    showInspector(hit);
    render();
    return;
  }

  // modo edição
  if (state.deleteMode && hit) {
    const layer = findBlock(hit.id).layer.id;
    deleteBlock(hit.id, layer);
    return;
  }
  if (state.connectFrom) {
    if (hit && hit.id !== state.connectFrom) addConnection(state.connectFrom, hit.id);
    state.connectFrom = null;
    document.getElementById('connBtn').classList.remove('active');
    return;
  }
  if (hit) {
    state.selectedBlock = hit.id;
    state.drag = { blockId: hit.id, layerId: findBlock(hit.id).layer.id, moved: false };
    showInspector(hit);
    render();
    return;
  }
  // célula vazia + tipo selecionado → coloca
  if (state.selectedType) {
    const cell = screenToCell(mx, my);
    const g = gridSpec();
    if (cell.x >= 0 && cell.y >= 0 && cell.x < g.width && cell.y < g.height) placeBlock(cell);
  }
});

canvas.addEventListener('mousemove', (e) => {
  if (!state.drag) return;
  state.drag.moved = true;
});

canvas.addEventListener('mouseup', (e) => {
  if (state.drag && state.drag.moved) {
    const { mx, my } = mouseXY(e);
    const cell = screenToCell(mx, my);
    const g = gridSpec();
    if (cell.x >= 0 && cell.y >= 0 && cell.x < g.width && cell.y < g.height) {
      moveBlock(state.drag.blockId, state.drag.layerId, cell);
    }
  }
  state.drag = null;
});

// ─── Controles de modo ───────────────────────────────────────────────────────

document.getElementById('modeBtn').addEventListener('click', () => {
  state.edit = !state.edit;
  document.body.classList.toggle('edit', state.edit);
  document.getElementById('modeBtn').classList.toggle('active', state.edit);
  document.getElementById('modeBtn').textContent = state.edit ? '👁️ Visualizar' : '✏️ Editar';
});

document.getElementById('connBtn').addEventListener('click', () => {
  if (!state.selectedBlock) { toast('Selecione um bloco de origem'); return; }
  state.connectFrom = state.selectedBlock;
  state.deleteMode = false;
  document.getElementById('connBtn').classList.add('active');
  document.getElementById('delBtn').classList.remove('active');
  toast('Clique no bloco de destino');
});

document.getElementById('delBtn').addEventListener('click', () => {
  state.deleteMode = !state.deleteMode;
  state.connectFrom = null;
  state.selectedType = null;
  document.getElementById('delBtn').classList.toggle('active', state.deleteMode);
  document.getElementById('connBtn').classList.remove('active');
  renderPalette();
});

load();
