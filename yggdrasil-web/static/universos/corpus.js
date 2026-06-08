// Ayvu Rapyta — superfície de exploração + Caderno (protótipo, local-first).
//
// Trilha de versos (Mbyá ⟷ Español), leitura interlinear: cada palavra se
// decompõe — animada — em partículas (morfemas), cada uma apontando para todos os
// sentidos potenciais no léxico. Camada de jogo (Phase 1, localStorage, sem auth):
//   ★ favoritar  ·  ✎ notas pessoais  ·  salvar/retomar a jornada  ·
//   ✦ sugestões (CRUD) de correção/glosa — rascunhos rumo à fila de revisão.
'use strict';

const SLUG = 'ayvu-rapyta';
const state = { work: null, chapters: [], lex: {}, ci: 0, vi: 0, stack: [], tab: 'fav' };

const $ = (id) => document.getElementById(id);
const esc = (s) => (s || '').replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const norm = (s) => (s || '').toLowerCase().replace(/^[^0-9a-zñ'’]+|[^0-9a-zñ'’]+$/gi, '');

// ── local-first store ───────────────────────────────────────────────────────
const db = {
  read(k, d) { try { return JSON.parse(localStorage.getItem('ayvu.' + k)) ?? d; } catch { return d; } },
  write(k, v) { localStorage.setItem('ayvu.' + k, JSON.stringify(v)); },
  fav: {}, notes: {}, sugg: [], progress: null,
  load() { this.fav = this.read('fav', {}); this.notes = this.read('notes', {}); this.sugg = this.read('sugg', []); this.progress = this.read('progress', null); },
  saveFav() { this.write('fav', this.fav); }, saveNotes() { this.write('notes', this.notes); },
  saveSugg() { this.write('sugg', this.sugg); }, saveProgress() { this.write('progress', this.progress); },
};
const uid = () => 's' + Math.random().toString(36).slice(2, 9);

function isFav(key) { return !!db.fav[key]; }
function toggleFav(key, item) {
  if (db.fav[key]) delete db.fav[key]; else db.fav[key] = { ...item, ts: Date.now() };
  db.saveFav(); refreshCadCount();
}
function getNote(key) { return db.notes[key] || ''; }
function setNote(key, text, meta) {
  if (text.trim()) db.notes[key] = text; else delete db.notes[key];
  db.notes['__meta_' + key] = text.trim() ? meta : undefined;
  if (!text.trim()) delete db.notes['__meta_' + key];
  db.saveNotes(); refreshCadCount();
}
function noteMeta(key) { return db.notes['__meta_' + key]; }

function refreshCadCount() {
  const n = Object.keys(db.fav).length + Object.keys(db.notes).filter((k) => !k.startsWith('__meta_')).length + db.sugg.length;
  $('cad-count').textContent = n;
}

// ── boot ─────────────────────────────────────────────────────────────────────
async function boot() {
  db.load();
  let data;
  try {
    const r = await fetch(`/api/v1/comunicacao/corpus/${SLUG}`);
    if (!r.ok) throw new Error(r.status);
    data = await r.json();
  } catch (e) {
    $('trail').innerHTML = `<p style="color:#c66">Corpus indisponível (${e.message}).</p>`;
    return;
  }
  state.work = data.work; state.chapters = data.chapters || []; state.lex = data.lex || {};
  $('work-title').textContent = data.work.title || 'Ayvu Rapyta';
  $('work-sub').textContent = `${data.work.author || ''} · ${data.work.year || ''}`.replace(/^ · | · $/g, '');

  const sel = $('chapsel');
  sel.innerHTML = state.chapters.map((c, i) =>
    `<option value="${i}">Cap. ${esc(c.roman)}${c.title_es ? ' — ' + esc(c.title_es) : ''}</option>`).join('');
  const def = state.chapters.findIndex((c) => c.n === 2);
  state.ci = def >= 0 ? def : 0;

  sel.onchange = () => { state.ci = +sel.value; state.vi = 0; renderChapter(); };
  $('prev').onclick = () => step(-1);
  $('next').onclick = () => step(1);
  $('insp-x').onclick = closeInspector;
  $('cad-x').onclick = () => closeCaderno();
  $('scrim').onclick = () => { closeInspector(); closeCaderno(); };
  $('insp-back').onclick = () => { state.stack.pop(); renderInspector(); };
  $('caderno-btn').onclick = openCaderno;
  document.querySelectorAll('.tabs button').forEach((b) => b.onclick = () => { state.tab = b.dataset.tab; renderCaderno(); });
  $('resume-x').onclick = () => $('resume').classList.remove('show');
  $('resume-go').onclick = () => {
    const p = db.progress; state.ci = p.ci; state.vi = p.vi; $('chapsel').value = String(p.ci);
    $('resume').classList.remove('show'); renderChapter(true);
  };
  document.addEventListener('keydown', onKey);
  document.addEventListener('click', onTokClick, true);

  // resume banner
  if (db.progress && (db.progress.ci !== state.ci || db.progress.vi !== 0)) {
    const c = state.chapters[db.progress.ci];
    if (c) { $('resume-txt').textContent = `Você parou em Cap. ${c.roman} · verso ${db.progress.vi + 1}.`; $('resume').classList.add('show'); }
  }
  $('chapsel').value = String(state.ci);
  refreshCadCount();
  renderChapter();
}

function step(d) { const n = state.ci + d; if (n < 0 || n >= state.chapters.length) return; state.ci = n; state.vi = 0; $('chapsel').value = String(n); renderChapter(); }
function onKey(e) {
  if (e.key === 'Escape') { closeInspector(); closeCaderno(); return; }
  if (e.key === 'ArrowDown' || e.key === 'ArrowRight') { focusVerse(state.vi + 1); e.preventDefault(); }
  else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') { focusVerse(state.vi - 1); e.preventDefault(); }
  else if (e.key === '[') step(-1); else if (e.key === ']') step(1);
}
function saveProgress() { db.progress = { ci: state.ci, vi: state.vi, ts: Date.now() }; db.saveProgress(); }
function focusVerse(i) {
  const ch = state.chapters[state.ci]; if (!ch || i < 0 || i >= ch.verses.length) return;
  state.vi = i; saveProgress();
  const stones = document.querySelectorAll('.stone');
  stones.forEach((el, k) => el.classList.toggle('on', k === i));
  stones[i]?.scrollIntoView({ behavior: 'smooth', block: 'center' });
}

// ── trail / interlinear ──────────────────────────────────────────────────────
function lensSet() { return new Set(Object.values(db.fav).filter((f) => f.t === 'part').map((f) => f.id)); }

function gnTokens(words) {
  const lens = lensSet();
  return words.map((w, i) => {
    const linked = (w.lemma && state.lex[w.lemma]) || (w.seg || []).some((s) => state.lex[s]);
    const fav = isFav('word:' + w.n);
    const isLens = (w.seg || []).some((s) => lens.has(s));
    const cls = ['tok', linked ? 'linked' : '', fav ? 'fav' : '', isLens ? 'lens' : ''].filter(Boolean).join(' ');
    return `<span class="${cls}" data-wi="${i}">${esc(w.w)}</span>`;
  }).join(' ');
}

function verseKey(ch, v) { return `verse:${ch.n}.${v.v}`; }

function renderChapter(keepVi) {
  const ch = state.chapters[state.ci]; if (!ch) return;
  if (!keepVi) state.vi = state.vi || 0;
  $('prev').disabled = state.ci === 0; $('next').disabled = state.ci === state.chapters.length - 1;
  closeInspector(); saveProgress();

  $('chtitle').innerHTML =
    `<div class="roman">CAPÍTULO ${esc(ch.roman)}</div><h2>${esc(ch.title_es || '—')}</h2>` +
    (ch.title_gn ? `<div class="gn">${esc(ch.title_gn)}</div>` : '') +
    (ch.pages ? `<div class="pg">pp. ${esc(ch.pages)}</div>` : '');

  $('trail').innerHTML = ch.verses.map((v, i) => {
    const vk = verseKey(ch, v);
    const fav = isFav(vk), note = !!getNote(vk);
    return `<div class="stone${i === state.vi ? ' on' : ''}" data-i="${i}">
      <div class="dot">${v.v ?? i + 1}</div>
      <div class="card">
        <div class="card-actions">
          <button class="iconbtn fav ${fav ? 'act' : ''}" data-fav="${i}" title="favoritar verso">★</button>
          <button class="iconbtn note ${note ? 'act' : ''}" data-note="${i}" title="nota / sugestão">✎</button>
        </div>
        <div class="gn">${gnTokens(v.words || [])}</div>
        <div class="es">${esc(v.es)}</div>
        <div class="vnote" data-vnote="${i}" style="display:none"></div>
      </div></div>`;
  }).join('');

  $('trail').querySelectorAll('[data-fav]').forEach((b) => b.onclick = (e) => {
    e.stopPropagation();
    const v = ch.verses[+b.dataset.fav];
    toggleFav(verseKey(ch, v), { t: 'verse', id: `${ch.n}.${v.v}`, label: `Cap ${ch.roman} · v${v.v}`, ci: state.ci, vi: +b.dataset.fav, gn: (v.gn || '').slice(0, 60) });
    b.classList.toggle('act');
  });
  $('trail').querySelectorAll('[data-note]').forEach((b) => b.onclick = (e) => { e.stopPropagation(); toggleVerseNote(+b.dataset.note); });

  const notes = $('notes');
  if ((ch.notes || []).length) {
    notes.innerHTML = `<h3 id="notes-h">NOTAS de Cadogan (${ch.notes.length}) ▾</h3>` +
      `<ul id="notes-ul" class="hidden">${ch.notes.map((n) => `<li>${esc(n)}</li>`).join('')}</ul>`;
    $('notes-h').onclick = () => $('notes-ul').classList.toggle('hidden');
  } else notes.innerHTML = '';
  if (!keepVi) window.scrollTo({ top: 0, behavior: 'smooth' });
}

function toggleVerseNote(i) {
  const ch = state.chapters[state.ci], v = ch.verses[i], vk = verseKey(ch, v);
  const box = document.querySelector(`[data-vnote="${i}"]`);
  if (box.style.display === 'block') { box.style.display = 'none'; return; }
  box.style.display = 'block';
  box.innerHTML =
    `<label style="color:var(--dim);font-size:12px">minha nota neste verso</label>
     <textarea>${esc(getNote(vk))}</textarea>
     <div class="rowbtns">
       <button class="iconbtn" data-savenote>salvar nota</button>
       <button class="iconbtn" data-suggest>✦ propor correção do verso</button>
     </div>`;
  const ta = box.querySelector('textarea');
  box.querySelector('[data-savenote]').onclick = () => {
    setNote(vk, ta.value, { label: `Cap ${ch.roman} · v${v.v}`, ci: state.ci, vi: i });
    document.querySelector(`[data-note="${i}"]`).classList.toggle('act', !!ta.value.trim());
    box.style.display = 'none';
  };
  box.querySelector('[data-suggest]').onclick = () => {
    addSuggestion({ kind: 'verso', label: `Cap ${ch.roman} · v${v.v}`, ci: state.ci, vi: i,
      before: `MBYÁ: ${v.gn}\nESP: ${v.es}`, after: `MBYÁ: ${v.gn}\nESP: ${v.es}` });
    openCaderno('sugg');
  };
}

// ── inspector (lexicon drill-down + word actions) ────────────────────────────
function onTokClick(e) {
  const tok = e.target.closest('.tok'); if (!tok) return;
  const stone = tok.closest('.stone'); if (!stone) return;
  const verse = state.chapters[state.ci].verses[+stone.dataset.i];
  openWord(verse.words[+tok.dataset.wi], tok);
}
function openWord(word, tokEl) {
  document.querySelectorAll('.tok.active').forEach((t) => t.classList.remove('active'));
  if (tokEl) tokEl.classList.add('active');
  state.stack = [{ kind: 'word', word }]; openInspector(); renderInspector();
}
function openInspector() { closeCaderno(); $('inspector').classList.add('open'); $('scrim').classList.add('show'); }
function closeInspector() { $('inspector').classList.remove('open'); if (!$('caderno').classList.contains('open')) $('scrim').classList.remove('show'); document.querySelectorAll('.tok.active').forEach((t) => t.classList.remove('active')); state.stack = []; }

function renderInspector() {
  const view = state.stack[state.stack.length - 1]; if (!view) return;
  $('insp-back').style.display = state.stack.length > 1 ? '' : 'none';
  if (view.kind === 'word') renderWordView(view.word); else renderFormView(view.form);
}
function chip(seg, i, curated) {
  const senses = state.lex[seg];
  const cls = senses ? (curated ? 'pchip cur linked' : 'pchip linked') : 'pchip plain';
  const cnt = senses ? `<span class="cnt">${senses.length}</span>` : '';
  return `<span class="${cls}" data-seg="${esc(seg)}" style="animation-delay:${i * 70}ms">${esc(seg)}${cnt}</span>`;
}
function renderWordView(word) {
  $('insp-title').textContent = 'palavra'; $('insp-lex').href = '/universos/comunicacao';
  const lemma = (word.lemma && state.lex[word.lemma]) || [];
  const fk = 'word:' + word.n, nk = fk;
  $('insp-body').innerHTML =
    `<div class="insp-word">${esc(word.w)}</div>` +
    (lemma.length ? `<div class="insp-sub">${esc(lemma[0].g || '')}</div>` : '<div class="insp-sub">forma não encontrada como lema — veja as partículas</div>') +
    `<div class="rowbtns">
       <button class="iconbtn ${isFav(fk) ? 'act' : ''}" id="w-fav">★ favoritar</button>
       <button class="iconbtn" id="w-note">✎ nota</button>
       <button class="iconbtn" id="w-sugg">✦ sugerir glosa</button>
     </div>
     <div id="w-notebox" style="display:none"></div>` +
    `<div class="seglabel">${word.cur ? 'partículas (Cadogan)' : 'partículas potenciais'}</div>` +
    `<div class="chips">${(word.seg || []).map((s, i) => chip(s, i, !!word.cur)).join('') || '<span class="insp-sub">—</span>'}</div>` +
    (lemma.length > 1 ? `<div class="seglabel">sentidos do lema</div>` + lemma.map(senseRow).join('') : '');
  $('w-fav').onclick = () => { toggleFav(fk, { t: 'word', id: word.n, label: word.w }); $('w-fav').classList.toggle('act'); refreshTokens(); };
  $('w-note').onclick = () => {
    const box = $('w-notebox'); if (box.style.display === 'block') { box.style.display = 'none'; return; }
    box.style.display = 'block';
    box.innerHTML = `<textarea>${esc(getNote(nk))}</textarea><div class="rowbtns"><button class="iconbtn" id="w-notesave">salvar</button></div>`;
    $('w-notesave').onclick = () => { setNote(nk, box.querySelector('textarea').value, { label: word.w }); box.style.display = 'none'; };
  };
  $('w-sugg').onclick = () => { addSuggestion({ kind: 'glosa', label: word.w, before: (lemma[0] && lemma[0].g) || '', after: (lemma[0] && lemma[0].g) || '' }); openCaderno('sugg'); };
  bindChips();
}
function renderFormView(form) {
  $('insp-title').textContent = 'partícula';
  const senses = state.lex[form] || [], fk = 'part:' + form;
  $('insp-body').innerHTML =
    `<div class="insp-word">${esc(form)}</div>` +
    `<div class="insp-sub">${senses.length} sentido(s) potencial(is) no léxico</div>` +
    `<div class="rowbtns"><button class="iconbtn ${isFav(fk) ? 'act' : ''}" id="p-fav">★ favoritar (lente)</button></div>` +
    `<div class="seglabel">ocorrências no léxico</div>` +
    (senses.map(senseRow).join('') || '<div class="insp-sub">sem entrada direta</div>');
  $('p-fav').onclick = () => { toggleFav(fk, { t: 'part', id: form, label: form }); $('p-fav').classList.toggle('act'); refreshTokens(); };
}
function senseRow(s, i) {
  const src = s.src === 'cadogan' ? '<span class="src">Cadogan</span>' : '';
  return `<div class="sense" style="animation-delay:${i * 60}ms"><div class="hw">${esc(s.hw)}${src}</div><div class="g">${esc(s.g || '')}</div></div>`;
}
function bindChips() {
  $('insp-body').querySelectorAll('.pchip.linked, .pchip.cur').forEach((c) =>
    c.onclick = () => { state.stack.push({ kind: 'form', form: c.dataset.seg }); renderInspector(); });
}
function refreshTokens() {
  // re-render the active chapter's tokens to reflect fav/lens without losing scroll
  const ch = state.chapters[state.ci];
  document.querySelectorAll('.stone').forEach((stone) => {
    const v = ch.verses[+stone.dataset.i];
    stone.querySelector('.gn').innerHTML = gnTokens(v.words || []);
  });
}

// ── suggestions (CRUD) ───────────────────────────────────────────────────────
function addSuggestion(s) { db.sugg.unshift({ id: uid(), ts: Date.now(), status: 'rascunho', ...s }); db.saveSugg(); refreshCadCount(); }
function updateSuggestion(id, patch) { const s = db.sugg.find((x) => x.id === id); if (s) Object.assign(s, patch); db.saveSugg(); }
function delSuggestion(id) { db.sugg = db.sugg.filter((x) => x.id !== id); db.saveSugg(); refreshCadCount(); renderCaderno(); }

// ── caderno (favorites / notes / suggestions) ────────────────────────────────
function openCaderno(tab) { closeInspector(); if (typeof tab === 'string') state.tab = tab; document.querySelectorAll('.tabs button').forEach((b) => b.classList.toggle('on', b.dataset.tab === state.tab)); $('caderno').classList.add('open'); $('scrim').classList.add('show'); renderCaderno(); }
function closeCaderno() { $('caderno').classList.remove('open'); if (!$('inspector').classList.contains('open')) $('scrim').classList.remove('show'); }

function jumpTo(item) {
  if (item.ci != null) { state.ci = item.ci; $('chapsel').value = String(item.ci); renderChapter(true); setTimeout(() => focusVerse(item.vi || 0), 60); closeCaderno(); }
}
function renderCaderno() {
  document.querySelectorAll('.tabs button').forEach((b) => b.classList.toggle('on', b.dataset.tab === state.tab));
  const body = $('cad-body');
  if (state.tab === 'fav') {
    const items = Object.entries(db.fav).sort((a, b) => b[1].ts - a[1].ts);
    body.innerHTML = items.length ? items.map(([k, it]) => `
      <div class="item"><div class="h">
        <span class="badge">${it.t}</span><b>${esc(it.label)}</b>
        ${it.ci != null ? `<span class="ref" data-jump='${esc(JSON.stringify(it))}'>ir →</span>` : ''}
        <button class="del" data-del="${esc(k)}">remover</button>
      </div>${it.gn ? `<div class="body">${esc(it.gn)}…</div>` : ''}</div>`).join('')
      : '<div class="empty">Nenhum favorito ainda. Toque ★ num verso, palavra ou partícula.</div>';
    body.querySelectorAll('[data-del]').forEach((b) => b.onclick = () => { delete db.fav[b.dataset.del]; db.saveFav(); refreshCadCount(); renderCaderno(); refreshTokens(); });
  } else if (state.tab === 'notes') {
    const keys = Object.keys(db.notes).filter((k) => !k.startsWith('__meta_'));
    body.innerHTML = keys.length ? keys.map((k) => { const m = noteMeta(k) || {}; return `
      <div class="item"><div class="h"><b>${esc(m.label || k)}</b>
        ${m.ci != null ? `<span class="ref" data-jump='${esc(JSON.stringify(m))}'>ir →</span>` : ''}
        <button class="del" data-del="${esc(k)}">apagar</button></div>
        <div class="body">${esc(db.notes[k])}</div></div>`; }).join('')
      : '<div class="empty">Sem notas. Toque ✎ num verso ou palavra para anotar.</div>';
    body.querySelectorAll('[data-del]').forEach((b) => b.onclick = () => { setNote(b.dataset.del, '', null); renderCaderno(); renderChapter(true); });
  } else {
    body.innerHTML = db.sugg.length ? db.sugg.map((s) => `
      <div class="item"><div class="h"><span class="badge">${esc(s.kind)}</span><b>${esc(s.label)}</b>
        <button class="del" data-del="${s.id}">descartar</button></div>
        <div class="sugg-form">
          <label>proposta (${esc(s.kind)})</label>
          <textarea data-edit="${s.id}">${esc(s.after)}</textarea>
        </div>
        <div class="rowbtns"><button class="iconbtn" data-submit="${s.id}">enviar p/ revisão (Fase 2)</button>
        <span class="insp-sub" style="align-self:center">status: ${esc(s.status)}</span></div></div>`).join('')
      : '<div class="empty">Sem sugestões. Use ✦ num verso (correção) ou palavra (glosa) para propor melhorias ao corpus.</div>';
    body.querySelectorAll('[data-edit]').forEach((t) => t.oninput = () => updateSuggestion(t.dataset.edit, { after: t.value }));
    body.querySelectorAll('[data-del]').forEach((b) => b.onclick = () => delSuggestion(b.dataset.del));
    body.querySelectorAll('[data-submit]').forEach((b) => b.onclick = () => { updateSuggestion(b.dataset.submit, { status: 'enviado (local)' }); alert('Fase 2: isto irá para a fila de revisão do léxico (Writeback). Por enquanto fica salvo no Caderno.'); renderCaderno(); });
  }
  body.querySelectorAll('[data-jump]').forEach((el) => el.onclick = () => jumpTo(JSON.parse(el.dataset.jump)));
}

boot();
