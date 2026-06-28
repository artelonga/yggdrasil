/* topologia.js — visualização do universo centralizado (YG-175, slice 2).
 *
 * Grafo de sentido cross-linguístico: cada termo de qualquer LexiconPack é um
 * nó; arestas ligam linguagens. Força-dirigido em <canvas> (vanilla, sem libs).
 * Caminhar = focar um nó: expande o grafo em volta (/grafo?around=) e, se você
 * estiver logado, registra a co-visitação a partir do nó anterior (POST
 * /explorar → weight++). É a "linkagem por exploração" da visão: a telemetria
 * de andar entre símbolos materializa as arestas. Inspector mostra glosa,
 * vizinhos (clique = caminhar) e referências (links). Logado pode nomear a
 * relação (/aresta) e anexar referência (/no/{id}/ref). */
(function () {
  'use strict';
  var API = '/api/v1/topologia';
  var $ = function (s) { return document.querySelector(s); };
  var esc = function (s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };
  function token() { try { return localStorage.getItem('yggdrasil-jwt'); } catch (e) { return null; } }
  function authed() { return !!token(); }
  function headers(json) {
    var h = json ? { 'Content-Type': 'application/json' } : {};
    var t = token(); if (t) h.Authorization = 'Bearer ' + t;
    return h;
  }
  function J(u, opt) { return fetch(u, opt).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }); }

  var canvas = $('#graph'), ctx = canvas.getContext('2d');
  var DPR = Math.max(1, window.devicePixelRatio || 1);
  var W = 0, H = 0;
  function resize() {
    // viewport direto: o canvas é fixed/inset:0, e clientWidth pode ser 0 antes
    // do primeiro layout (gravidade iria pro canto (0,0)).
    W = window.innerWidth || canvas.clientWidth;
    H = window.innerHeight || canvas.clientHeight;
    canvas.width = W * DPR; canvas.height = H * DPR;
    ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
  }
  window.addEventListener('resize', resize);

  // ── estado do grafo ──────────────────────────────────────────────────────
  var nodes = {};         // id → {id,x,y,vx,vy,pack,kind,term,gloss,role,pinned}
  var edges = [];         // {a,b,weight,relation,source}
  var focusId = null, prevFocus = null;
  var overlays = { corpus: false, lexico: false, neural: false }; // overlays de sentido (cosseno)
  var myWords = {};      // YG-178: id → {status, seen_count} (camada pessoal)
  var myNodeList = [];   // nós próprios (p/ persistência local + migração)
  var myEdgeList = [];   // arestas próprias (idem)
  var onlyMine = false;  // filtro "só minhas"

  // ── tier grátis: memória LOCAL (cache) p/ anônimos crescerem antes do signup ──
  var LOCAL_KEY = 'ygg-topo-local';
  function loadLocal() {
    try { return JSON.parse(localStorage.getItem(LOCAL_KEY)); } catch (e) { return null; }
  }
  function persistLocal() {
    if (authed()) return; // logado → servidor é a fonte
    try { localStorage.setItem(LOCAL_KEY, JSON.stringify({ words: myWords, nodes: myNodeList, edges: myEdgeList, texts: myTexts })); } catch (e) {}
  }
  var nextStatus = { visited: 'learning', learning: 'known', known: 'known' };
  // índice termo(lower) → id, p/ casar palavras de textos sem slugify no cliente.
  function termIndex() {
    if (termIndex._c) return termIndex._c;
    var idx = {}; for (var id in byId) { idx[(byId[id].term || '').toLowerCase()] = id; }
    termIndex._c = idx; return idx;
  }
  function matchClient(text) {
    var idx = termIndex(), seen = {}, out = [];
    text.split(/[^\p{L}\p{N}'’]+/u).forEach(function (w) {
      var id = idx[w.toLowerCase()];
      if (id && !seen[id]) { seen[id] = 1; out.push(id); }
    });
    return out;
  }
  // CTA: depois de cultivar algumas palavras, convida a salvar/criar conta.
  function maybeCTA() {
    if (authed() || localStorage.getItem('ygg-cta-dismissed')) return;
    if (Object.keys(myWords).length < 3) return;
    var c = $('#cta'); if (!c || c.classList.contains('show')) return;
    c.innerHTML = '🌱 Você já cultivou <b>' + Object.keys(myWords).length +
      ' palavras</b> — crie uma conta para salvá-las e sincronizar. ' +
      '<a class="r-cta" href="/login">criar conta / entrar</a> ' +
      '<span id="cta-x" title="agora não">✕</span>';
    c.classList.add('show');
    $('#cta-x').addEventListener('click', function () { c.classList.remove('show'); localStorage.setItem('ygg-cta-dismissed', '1'); });
  }
  // Migra o cache local para a conta após login (o tier grátis não se perde).
  function migrateLocal() {
    var l = loadLocal();
    if (!l || !(Object.keys(l.words || {}).length || (l.nodes || []).length || (l.texts || []).length)) return Promise.resolve();
    var payload = {
      words: Object.keys(l.words || {}),
      nodes: (l.nodes || []).map(function (n) { return { term: n.term, gloss: n.gloss || null }; }),
      edges: (l.edges || []).map(function (e) { return { a: e.a, b: e.b, relation: e.relation || null }; }),
      texts: (l.texts || []).map(function (t) { return { title: t.loc, text: t.text }; }),
    };
    return J('/api/v1/me/topologia/importar', { method: 'POST', headers: headers(true), body: JSON.stringify(payload) })
      .then(function (r) { localStorage.removeItem(LOCAL_KEY); if (r) toast('progresso salvo na sua conta ✦'); });
  }
  function activeOverlays() { return Object.keys(overlays).filter(function (k) { return overlays[k]; }); }
  function anyOverlay() { return overlays.corpus || overlays.lexico; }
  var view = { x: 0, y: 0, k: 1 };   // pan (x,y) + zoom (k)

  // cor por língua (dados reais: gn-mbya, yo). Verde-floresta p/ Mbyá, âmbar p/
  // Iorubá; outras línguas por hash de matiz. (Sem 'música' — era conceito.)
  function packColor(pack) {
    if (pack === 'gn-mbya') return '#86c98e';
    if (pack === 'yo') return '#e9c349';
    if (pack === 'meu') return '#ffb3b5'; // YG-178: meus nós próprios (rosa)
    var h = 0; for (var i = 0; i < pack.length; i++) h = (h * 31 + pack.charCodeAt(i)) % 360;
    return 'hsl(' + h + ',55%,68%)';
  }

  // Layout fixo: posições vêm do servidor (espiral de phyllotaxis por rank de
  // popularidade). Sem força global — 4837 nós exigem posições determinísticas.
  function ensureNode(rn) {
    var n = nodes[rn.id];
    if (!n) {
      n = nodes[rn.id] = { id: rn.id, x: rn.x || 0, y: rn.y || 0, vx: 0, vy: 0, pinned: true };
    }
    if (typeof rn.x === 'number') { n.x = rn.x; n.y = rn.y; }
    n.pack = rn.pack; n.kind = rn.kind; n.term = rn.term; n.gloss = rn.gloss;
    n.role = rn.role; n.pop = rn.pop || 0;
    return n;
  }

  // mescla um subgrafo no estado (preserva posições dos nós já presentes)
  function mergeGraph(g) {
    if (!g) return;
    (g.nodes || []).forEach(ensureNode);
    (g.edges || []).forEach(function (e) {
      // chaveia por par + source: uma aresta semântica e uma de exploração entre
      // o mesmo par coexistem (overlay vs grafo humano).
      var found = edges.find(function (x) { return x.a === e.a && x.b === e.b && x.source === e.source && x.method === e.method; });
      if (found) { found.weight = e.weight; found.relation = e.relation; found.score = e.score; }
      else edges.push({ a: e.a, b: e.b, weight: e.weight, relation: e.relation, source: e.source, score: e.score, method: e.method });
    });
  }

  // ── física força-dirigida ──────────────────────────────────────────────────
  function step() {
    var ids = Object.keys(nodes);
    // repulsão entre todos os pares
    for (var i = 0; i < ids.length; i++) {
      var a = nodes[ids[i]];
      for (var j = i + 1; j < ids.length; j++) {
        var b = nodes[ids[j]];
        var dx = a.x - b.x, dy = a.y - b.y;
        var d2 = Math.max(dx * dx + dy * dy, 600); // piso evita "explosão" no seed
        var f = Math.min(3.0, 4200 / d2);          // repulsão capada
        var d = Math.sqrt(d2);
        var fx = (dx / d) * f, fy = (dy / d) * f;
        a.vx += fx; a.vy += fy; b.vx -= fx; b.vy -= fy;
      }
    }
    // molas nas arestas (mais peso → mais curta/firme)
    edges.forEach(function (e) {
      var a = nodes[e.a], b = nodes[e.b]; if (!a || !b) return;
      var dx = b.x - a.x, dy = b.y - a.y;
      var d = Math.sqrt(dx * dx + dy * dy) || 0.01;
      var rest = 120 - Math.min(40, e.weight * 8);
      var f = (d - rest) * 0.012;
      var fx = (dx / d) * f, fy = (dy / d) * f;
      a.vx += fx; a.vy += fy; b.vx -= fx; b.vy -= fy;
    });
    // gravidade ao centro + integração + damping
    ids.forEach(function (id) {
      var n = nodes[id];
      if (n.pinned) { n.vx = 0; n.vy = 0; return; }
      n.vx += (W / 2 - n.x) * 0.004;
      n.vy += (H / 2 - n.y) * 0.004;
      n.vx *= 0.85; n.vy *= 0.85;
      n.x += n.vx; n.y += n.vy;
    });
  }

  // raio por popularidade (nº de exemplos no corpus); foco maior.
  function nodeRadius(n) {
    if (n.id === focusId) return 16;
    return 4 + Math.min(9, Math.log2((n.pop || 0) + 1) * 1.6);
  }

  // ── render ─────────────────────────────────────────────────────────────────
  function toScreen(p) { return { x: p.x * view.k + view.x, y: p.y * view.k + view.y }; }
  // ids vizinhos do nó focado (p/ realce + LOD de rótulo)
  function focusNeighborIds() {
    var s = {};
    if (!focusId) return s;
    edges.forEach(function (e) {
      if (e.a === focusId) s[e.b] = true; else if (e.b === focusId) s[e.a] = true;
    });
    return s;
  }
  function draw() {
    ctx.clearRect(0, 0, W, H);
    // arestas
    edges.forEach(function (e) {
      var a = nodes[e.a], b = nodes[e.b]; if (!a || !b) return;
      var pa = toScreen(a), pb = toScreen(b);
      ctx.beginPath();
      ctx.moveTo(pa.x, pa.y); ctx.lineTo(pb.x, pb.y);
      if (e.source === 'user') {
        ctx.strokeStyle = 'rgba(233,195,73,.8)'; ctx.setLineDash([]);
        ctx.lineWidth = Math.min(5, 1 + e.weight * 0.7);
      } else if (e.source === 'semantic') {
        ctx.strokeStyle = e.method === 'lexico' ? 'rgba(111,208,192,.5)' : e.method === 'neural' ? 'rgba(255,155,210,.55)' : 'rgba(170,150,255,.5)';
        ctx.setLineDash([2, 4]);
        ctx.lineWidth = 1 + (e.score || 0) * 2.2;
      } else {
        ctx.strokeStyle = 'rgba(197,198,204,.32)'; ctx.setLineDash([5, 5]);
        ctx.lineWidth = Math.min(5, 1 + e.weight * 0.7);
      }
      ctx.stroke(); ctx.setLineDash([]);
      var lbl = e.relation || (e.source === 'semantic' && view.k > 0.8 ? (e.score || 0).toFixed(2) : '');
      if (lbl) {
        ctx.fillStyle = e.source !== 'semantic' ? 'rgba(233,195,73,.85)' : (e.method === 'lexico' ? 'rgba(111,208,192,.85)' : e.method === 'neural' ? 'rgba(255,155,210,.9)' : 'rgba(170,150,255,.85)');
        ctx.font = '11px Manrope, sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(lbl, (pa.x + pb.x) / 2, (pa.y + pb.y) / 2 - 4);
      }
    });
    // nós — com culling de viewport + LOD nos rótulos (4837 nós).
    var neigh = focusNeighborIds();
    var fewNodes = Object.keys(nodes).length <= 60; // modo sentença: rotula tudo
    ctx.textAlign = 'center'; ctx.textBaseline = 'top';
    Object.keys(nodes).forEach(function (id) {
      if (onlyMine && !myWords[id]) return; // filtro "só minhas"
      var n = nodes[id], p = toScreen(n);
      if (p.x < -40 || p.x > W + 40 || p.y < -40 || p.y > H + 40) return; // cull
      var r = nodeRadius(n) * Math.min(1.6, Math.max(0.5, view.k));
      var focused = id === focusId;
      if (focused || neigh[id]) {
        ctx.beginPath(); ctx.arc(p.x, p.y, r + 4, 0, 7);
        ctx.strokeStyle = focused ? 'rgba(233,195,73,.95)' : 'rgba(170,150,255,.6)';
        ctx.lineWidth = 2; ctx.stroke();
      }
      // YG-178: léxico-não-meu = oco/apagado; MINHA = preenchida por status + anel.
      var mine = myWords[id];
      ctx.beginPath(); ctx.arc(p.x, p.y, r, 0, 7);
      if (mine) {
        if (mine.status === 'known') { ctx.fillStyle = '#e9c349'; ctx.fill(); }
        else { ctx.globalAlpha = mine.status === 'learning' ? 0.85 : 0.5; ctx.fillStyle = packColor(n.pack); ctx.fill(); ctx.globalAlpha = 1; }
        ctx.beginPath(); ctx.arc(p.x, p.y, r + 2.5, 0, 7); ctx.strokeStyle = 'rgba(233,195,73,.85)'; ctx.lineWidth = 1.5; ctx.stroke();
      } else {
        ctx.globalAlpha = 0.4; ctx.strokeStyle = packColor(n.pack); ctx.lineWidth = 1.5; ctx.stroke(); ctx.globalAlpha = 1;
      }
      // LOD: poucos nós (modo sentença) → sempre rotula; senão, só ampliado/populares.
      if (fewNodes || focused || neigh[id] || view.k > 1.5 || (view.k > 0.7 && (n.pop || 0) >= 8)) {
        ctx.fillStyle = focused ? '#fff' : '#d8d6d4';
        ctx.font = 'italic ' + (focused ? 17 : 13) + 'px Newsreader, Georgia, serif';
        ctx.fillText(n.term, p.x, p.y + r + 2);
      }
    });
  }

  // Layout fixo (posições do servidor) → sem física global; só desenha.
  function loop() { draw(); requestAnimationFrame(loop); }

  // ── interação: pan / zoom / drag / clique ──────────────────────────────────
  function nodeAt(sx, sy) {
    var hit = null, best = 1e9;
    Object.keys(nodes).forEach(function (id) {
      var n = nodes[id], p = toScreen(n), r = nodeRadius(n) * view.k + 6;
      var dx = sx - p.x, dy = sy - p.y, d = dx * dx + dy * dy;
      if (d < r * r && d < best) { best = d; hit = n; }
    });
    return hit;
  }
  var drag = null, panning = null, downAt = null;
  canvas.addEventListener('mousedown', function (e) {
    downAt = { x: e.clientX, y: e.clientY };
    var n = nodeAt(e.offsetX, e.offsetY);
    if (n) { drag = n; n.pinned = true; }
    else panning = { x: e.offsetX, y: e.offsetY, vx: view.x, vy: view.y };
  });
  window.addEventListener('mousemove', function (e) {
    if (drag) {
      var rect = canvas.getBoundingClientRect();
      drag.x = (e.clientX - rect.left - view.x) / view.k;
      drag.y = (e.clientY - rect.top - view.y) / view.k;
    } else if (panning) {
      view.x = panning.vx + (e.offsetX - panning.x);
      view.y = panning.vy + (e.offsetY - panning.y);
    }
  });
  window.addEventListener('mouseup', function (e) {
    // clique = movimento < 5px (tolera o micro-drag do clique sintético)
    var click = downAt && Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) < 5;
    if (drag && click) { claim(drag.id); focus(drag.id); }
    if (drag) drag.pinned = (drag.id === focusId); // mantém só o focado fixo
    drag = null; panning = null; downAt = null;
  });
  canvas.addEventListener('wheel', function (e) {
    e.preventDefault();
    var f = e.deltaY < 0 ? 1.1 : 0.9;
    var nk = Math.max(0.3, Math.min(3, view.k * f));
    // zoom centrado no cursor
    view.x = e.offsetX - (e.offsetX - view.x) * (nk / view.k);
    view.y = e.offsetY - (e.offsetY - view.y) * (nk / view.k);
    view.k = nk;
  }, { passive: false });

  // ── caminhar / focar ───────────────────────────────────────────────────────
  function focus(id) {
    if (!id) return;
    // exploração: caminhar de prevFocus → id registra co-visitação (logado)
    if (authed() && focusId && focusId !== id) {
      var from = focusId;
      J(API + '/explorar', { method: 'POST', headers: headers(true), body: JSON.stringify({ from: from, to: id }) })
        .then(function (edge) {
          if (edge) {
            mergeGraph({ nodes: [], edges: [{ a: edge.a, b: edge.b, weight: edge.weight, relation: edge.relation, source: edge.source }] });
            toast('explorado: ' + short(from) + ' ⇄ ' + short(id) + ' (peso ' + edge.weight + ')');
          }
        });
    }
    prevFocus = focusId; focusId = id;
    Object.keys(nodes).forEach(function (k) { if (nodes[k].pinned && k !== id) nodes[k].pinned = false; });
    if (nodes[id]) nodes[id].pinned = false;
    // grafo primeiro (popula arestas, incl. semânticas) → depois o inspector,
    // que lê as arestas semânticas incidentes do estado em memória.
    var sem = anyOverlay() ? '&semantica=true&overlay=' + activeOverlays().join(',') : '';
    J(API + '/grafo?around=' + encodeURIComponent(id) + '&depth=2' + sem)
      .then(mergeGraph)
      .then(function () { return J(API + '/no/' + encodeURIComponent(id)); })
      .then(renderInspector);
  }
  function short(id) { var n = nodes[id]; return n ? n.term : id.split(':')[1] || id; }

  // YG-178: reivindicar/aprender — clicar uma palavra a torna MINHA (e avança status).
  function claim(id) {
    if (!id || !nodes[id]) return;
    if (authed()) {
      J('/api/v1/me/topologia/visitar', { method: 'POST', headers: headers(true), body: JSON.stringify({ node: id }) })
        .then(function (w) {
          if (w && w.status) { myWords[id] = { status: w.status, seen_count: w.seen_count }; updateMineCount(); toast(short(id) + ' → ' + w.status + (w.status === 'known' ? ' ✦' : '')); }
        });
    } else {
      // tier grátis: cresce no cache local; avança o status localmente.
      var cur = myWords[id];
      myWords[id] = cur ? { status: nextStatus[cur.status] || 'known', seen_count: cur.seen_count + 1 } : { status: 'visited', seen_count: 1 };
      persistLocal(); updateMineCount(); maybeCTA();
      toast(short(id) + ' → ' + myWords[id].status + ' (local)');
    }
  }
  function updateMineCount() {
    var n = Object.keys(myWords).length;
    var who = $('#whoami'); if (who) { who.style.display = n ? '' : 'none'; who.textContent = '● ' + n + ' minhas'; }
    // "só minhas" só aparece quando há ao menos 1 (senão confunde quem chega novo)
    var sm = $('#only-mine'); if (sm) sm.style.display = n ? '' : 'none';
  }

  // ── inspector ───────────────────────────────────────────────────────────────
  function renderInspector(data) {
    if (!data) return;
    var n = data.node;
    var chips = '<span class="r-chip">' + esc(n.pack) + '</span>' +
      '<span class="r-chip">' + esc(n.kind) + '</span>' +
      (n.role ? '<span class="r-chip">' + esc(n.role) + '</span>' : '');
    var refs = (data.refs || []).map(function (r) {
      return '<a class="ref" href="' + esc(r.href) + '" target="_blank" rel="noopener">↗ ' +
        esc(r.label || r.kind) + ' <span style="color:var(--on-var)">(' + esc(r.kind) + ')</span></a>';
    }).join('') || '<div class="empty">Nenhuma referência. ' + (authed() ? 'Anexe uma por link.' : 'Entre para anexar.') + '</div>';

    // Próximos por SENTIDO (cosseno) — uma seção por overlay ativo, lidas do
    // estado em memória; cor por método (corpus violeta / léxico teal).
    function semSection(method, label, color) {
      if (!overlays[method]) return '';
      var sem = edges.filter(function (x) { return x.source === 'semantic' && x.method === method && (x.a === n.id || x.b === n.id); })
        .map(function (x) { return { node: x.a === n.id ? x.b : x.a, score: x.score || 0 }; })
        .sort(function (p, q) { return q.score - p.score; });
      return '<h4>' + label + '</h4>' + (sem.map(function (s) {
        return '<div class="nb" data-go="' + esc(s.node) + '">' +
          '<span class="t">' + esc(short(s.node)) + '</span>' +
          '<span class="w" style="color:' + color + '">' + s.score.toFixed(2) +
          (authed() ? ' <span class="prom" data-prom="' + esc(s.node) + '" title="promover a relação">✦</span>' : '') +
          '</span></div>';
      }).join('') || '<div class="empty">Sem vizinhos acima do limiar.</div>');
    }
    var semList = semSection('corpus', 'Por sentido · corpus (Ayvu Rapytã)', '#c9bcff') +
                  semSection('lexico', 'Por sentido · léxico (definição)', '#9fe3d8') +
                  semSection('neural', 'Por sentido · neural (embedding local)', '#ffc4e3');

    // Instâncias REAIS em sentenças (YG-177): versos do Ayvu Rapytã + exemplos do
    // dicionário. Cada verso traz toggles de TRADUÇÃO (lentes): ES de Cadogan
    // disponível; demais = "propor tradução" (YG-179). Mídia (áudio…) virá depois.
    var versos = (data.versos || []).map(function (v) {
      var tr = v.tr || {};
      var chips = Object.keys(tr).map(function (l) {
        return '<button class="tr-chip" data-tr="' + esc(tr[l]) + '" data-l="' + esc(l) + '">' + l.toUpperCase() + '</button>';
      }).join('');
      return '<div class="inst"><span class="loc">' + esc(v.chapter) + ' · v' + v.verse + '</span>' +
        '<div class="vt">' + esc(v.text) + '</div>' +
        '<div class="tr-row">' + chips + '<span class="tr-propose" title="propor uma tradução / anexar mídia (em breve)">+ tradução</span></div>' +
        '<div class="vp tr-text" hidden></div></div>';
    }).join('');
    var exs = (data.exemplos || []).map(function (x) {
      return '<div class="inst"><div class="vt">' + esc(x.gn) + '</div>' +
        '<div class="vp">' + esc(x.pt) + '</div></div>';
    }).join('');
    var instBlock =
      (versos ? '<h4>Instâncias no Ayvu Rapytã <span class="hint-i">(traduções: clique ES)</span></h4>' + versos : '') +
      (exs ? '<h4>Exemplos (dicionário)</h4>' + exs : '');

    // Card: termo → glosa → INSTÂNCIAS (com tradução) → sentido (só se overlay on)
    // → referências → ações. Conexões NÃO em lista — aparecem no grafo (caminhe).
    $('#insp-body').innerHTML =
      '<div class="r-display term">' + esc(n.term) + '</div>' +
      '<div class="meta">' + chips + '</div>' +
      (n.gloss ? '<div class="gloss">' + esc(n.gloss) + '</div>' : '') +
      instBlock +
      semList +
      '<h4>Referências (links)</h4>' + refs +
      '<div class="row-actions">' +
        '<button class="r-ghost" id="act-vocab" title="adiciona ao seu vocabulário — revise depois">＋ vocabulário</button>' +
        '<button class="r-ghost" id="act-conn" title="conecte esta palavra a outra que é sua (conexão pessoal)">＋ conexão</button>' +
        '<button class="r-ghost gated" id="act-ref">＋ referência</button>' +
      '</div>';
    $('#inspector').classList.add('open');
    // toggle de tradução por verso (lente): clicar ES mostra o texto de Cadogan.
    $('#insp-body').querySelectorAll('.tr-chip').forEach(function (el) {
      el.addEventListener('click', function () {
        var box = el.closest('.inst').querySelector('.tr-text');
        if (box.dataset.l === el.dataset.l && !box.hidden) { box.hidden = true; box.dataset.l = ''; return; }
        box.textContent = '[' + el.dataset.l + '] ' + el.dataset.tr; box.dataset.l = el.dataset.l; box.hidden = false;
      });
    });
    $('#insp-body').querySelectorAll('.nb[data-go]').forEach(function (el) {
      el.addEventListener('click', function () { claim(el.dataset.go); focus(el.dataset.go); });
    });
    // promover sugestão semântica → aresta user (✦ dentro do item; não navega)
    $('#insp-body').querySelectorAll('.prom[data-prom]').forEach(function (el) {
      el.addEventListener('click', function (ev) {
        ev.stopPropagation();
        var alvo = el.dataset.prom;
        var rel = prompt('Confirmar relação entre "' + short(n.id) + '" e "' + short(alvo) + '" — nome (ex.: mesmo conceito):', 'mesmo conceito');
        if (rel == null) return;
        J(API + '/aresta', { method: 'POST', headers: headers(true), body: JSON.stringify({ a: n.id, b: alvo, relation: rel || null }) })
          .then(function (edge) { if (edge) { mergeGraph({ nodes: [], edges: [edge] }); toast('promovido a relação ✦'); focus(n.id); } else toast('falhou'); });
      });
    });
    var av = $('#act-vocab'); if (av) av.addEventListener('click', function () { claim(n.id); });
    var ac = $('#act-conn'); if (ac) ac.addEventListener('click', function () { minhaConexao(n.id); });
    var af = $('#act-ref'); if (af) af.addEventListener('click', function () { anexarRef(n.id); });
  }
  $('#insp-close').addEventListener('click', function () { $('#inspector').classList.remove('open'); });

  function nomearRelacao(id, neighbors) {
    if (!neighbors.length) { toast('caminhe até um vizinho primeiro'); return; }
    var alvo = prompt('Qual termo (id) ligar a "' + short(id) + '"?\n' +
      neighbors.map(function (nb) { return '• ' + nb.node; }).join('\n'), neighbors[0].node);
    if (!alvo) return;
    var rel = prompt('Nome da relação (ex.: cognato, mesmo conceito, rima sonora):', '');
    if (rel == null) return;
    J(API + '/aresta', { method: 'POST', headers: headers(true), body: JSON.stringify({ a: id, b: alvo, relation: rel || null }) })
      .then(function (edge) {
        if (edge) { mergeGraph({ nodes: [], edges: [edge] }); toast('relação nomeada ✦'); focus(id); }
        else toast('falhou (nó válido? logado?)');
      });
  }
  // YG-178 slice 2: conexão PESSOAL (privada) entre duas palavras minhas/léxico.
  function minhaConexao(id) {
    var q = prompt('Conectar "' + short(id) + '" a qual palavra? (termo exato)');
    if (!q) return;
    var tid = byId[q] ? q : null;
    if (!tid) { for (var k in byId) { if (byId[k].term === q) { tid = k; break; } } }
    if (!tid) { toast('palavra não encontrada no léxico/seus nós'); return; }
    var rel = prompt('Nome da conexão (opcional):') || null;
    if (authed()) {
      J('/api/v1/me/topologia/aresta', { method: 'POST', headers: headers(true), body: JSON.stringify({ a: id, b: tid, relation: rel }) })
        .then(function (e) { if (e && e.a) { if (byId[tid]) ensureNode(byId[tid]); edges.push(e); toast('conexão sua criada'); focus(id); } else toast('falhou'); });
    } else {
      var e = { a: id, b: tid, weight: 1, relation: rel, source: 'user' };
      edges.push(e); myEdgeList.push(e); if (byId[tid]) ensureNode(byId[tid]); persistLocal(); toast('conexão sua (local)'); focus(id);
    }
  }
  function anexarRef(id) {
    var kind = prompt('Tipo da referência: sentence | etymology | audio | link', 'link');
    if (!kind) return;
    var href = prompt('Link (href) — frase/etimologia/áudio/URL:', '');
    if (!href) return;
    var label = prompt('Rótulo (opcional):', '') || null;
    J(API + '/no/' + encodeURIComponent(id) + '/ref', { method: 'POST', headers: headers(true), body: JSON.stringify({ kind: kind, href: href, label: label }) })
      .then(function (r) { if (r) { toast('referência anexada'); focus(id); } else toast('falhou (tipo válido? logado?)'); });
  }

  // ── paleta (catálogo de nós) + busca ────────────────────────────────────────
  // catálogo = LOOKUP só (id→nó); NÃO se renderiza tudo (era a lag). O grafo
  // começa vazio e se filtra às palavras de UMA sentença ("ler primeiro").
  var byId = {};       // id → nó (term/gloss/x/y/pop)
  var sentList = [];   // sentenças do Ayvu Rapytã (/sentencas)
  var myTexts = [];    // meus textos (corpus pessoal, /me/topologia/textos)
  var PAL_MAX = 120;

  function renderSentences(filter) {
    var f = (filter || '').toLowerCase();
    var items = myTexts.concat(sentList).filter(function (s) {
      return !f || s.text.toLowerCase().indexOf(f) >= 0 || s.loc.toLowerCase().indexOf(f) >= 0;
    }).slice(0, PAL_MAX);
    var html = items.map(function (s) {
      return '<div class="pal-item sent" data-sent="' + esc(s.id) + '">' +
        '<div><span class="loc" style="font-size:.62rem;letter-spacing:.1em;color:var(--secondary)">' + esc(s.loc) +
        '</span> <span class="gl">' + s.terms.length + ' palavras</span></div>' +
        '<div class="vt" style="font-family:var(--head);font-style:italic">' + esc(s.text) + '</div></div>';
    }).join('') || '<div class="hint">nenhuma sentença.</div>';
    $('#pal-list').innerHTML = html;
    $('#pal-list').querySelectorAll('.pal-item[data-sent]').forEach(function (el) {
      el.addEventListener('click', function () {
        var s = sentList.find(function (x) { return x.id === el.dataset.sent; });
        if (s) loadSentence(s);
        if (window.innerWidth < 720) $('#palette').classList.remove('open');
      });
    });
  }

  // Renderiza SÓ as palavras desta sentença (nas posições reais do léxico) +
  // suas conexões internas. Nada de 8 mil nós. O texto fica no banner (ler).
  function loadSentence(s) {
    nodes = {}; edges = []; focusId = null; prevFocus = null;
    s.terms.forEach(function (id) { if (byId[id]) ensureNode(byId[id]); });
    var tr = s.tr || {};
    var trChips = Object.keys(tr).map(function (l) {
      return '<button class="tr-chip" data-tr="' + esc(tr[l]) + '" data-l="' + esc(l) + '">' + l.toUpperCase() + '</button>';
    }).join('');
    $('#sent-banner').innerHTML = '<span class="loc">' + esc(s.loc) + '</span> ' + esc(s.text) +
      (trChips || s.lang === 'gn-mbya' ? '<div class="b-tr">' + trChips + '<span class="tr-propose" title="propor tradução / mídia (em breve)">+ idioma</span></div>' : '') +
      '<div class="b-trtext tr-text" hidden></div>';
    $('#sent-banner').classList.add('show');
    $('#sent-banner').querySelectorAll('.tr-chip').forEach(function (el) {
      el.addEventListener('click', function () {
        var box = $('#sent-banner').querySelector('.b-trtext');
        if (box.dataset.l === el.dataset.l && !box.hidden) { box.hidden = true; box.dataset.l = ''; return; }
        box.textContent = '[' + el.dataset.l + '] ' + el.dataset.tr; box.dataset.l = el.dataset.l; box.hidden = false;
      });
    });
    fitView();
    loadSentenceEdges();
    toast(s.terms.length + ' palavras desta sentença — clique uma para explorar');
  }

  // Arestas INTRA-sentença (entre as palavras carregadas), p/ os overlays ativos.
  function loadSentenceEdges() {
    if (!anyOverlay()) return;
    var set = {}; Object.keys(nodes).forEach(function (id) { set[id] = true; });
    var ov = activeOverlays().join(',');
    Object.keys(nodes).forEach(function (id) {
      J(API + '/grafo?around=' + encodeURIComponent(id) + '&depth=1&semantica=true&overlay=' + ov)
        .then(function (g) {
          if (!g) return;
          (g.edges || []).forEach(function (e) {
            if (set[e.a] && set[e.b] &&
                !edges.find(function (x) { return x.a === e.a && x.b === e.b && x.source === e.source && x.method === e.method; }))
              edges.push(e);
          });
        });
    });
  }
  $('#pal-toggle').addEventListener('click', function () { $('#palette').classList.toggle('open'); });
  // guia / onboarding (aprender explorando): abre no 1º acesso, reabre pelo "? guia".
  function openHelp() { $('#help-ov').classList.add('show'); }
  function closeHelp() { $('#help-ov').classList.remove('show'); localStorage.setItem('ygg-help-seen', '1'); }
  $('#help-btn').addEventListener('click', openHelp);
  $('#help-x').addEventListener('click', closeHelp);
  $('#help-go').addEventListener('click', closeHelp);
  $('#help-ov').addEventListener('click', function (e) { if (e.target === $('#help-ov')) closeHelp(); });
  function toggleOverlay(which, btn) {
    overlays[which] = !overlays[which];
    $(btn).classList.toggle('on', overlays[which]);
    // purga arestas semânticas do overlay desligado; mantém o outro
    edges = edges.filter(function (e) { return e.source !== 'semantic' || overlays[e.method]; });
    if (focusId) focus(focusId);
    else if (Object.keys(nodes).length) loadSentenceEdges();
    else toast(anyOverlay() ? 'overlay ligado — escolha uma sentença' : 'overlays desligados');
  }
  $('#sem-corpus').addEventListener('click', function () { toggleOverlay('corpus', '#sem-corpus'); });
  $('#sem-lexico').addEventListener('click', function () { toggleOverlay('lexico', '#sem-lexico'); });
  $('#sem-neural').addEventListener('click', function () { toggleOverlay('neural', '#sem-neural'); });
  $('#only-mine').addEventListener('click', function () {
    onlyMine = !onlyMine;
    $('#only-mine').classList.toggle('on', onlyMine);
    toast(onlyMine ? 'só minhas palavras' : 'léxico completo');
  });
  // YG-178 slice 2: EXPRESSÃO — adicionar palavra própria / escrever texto.
  function addLocalNode(term, gloss) {
    var slug = term.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '') || 'x';
    var n = { id: 'local:' + slug, pack: 'meu', kind: 'user', term: term, gloss: gloss || undefined, x: -3000 - (myNodeList.length * 60), y: -400 + (myNodeList.length * 50) % 900, pop: 0 };
    byId[n.id] = n; myNodeList.push(n); ensureNode(n); myWords[n.id] = { status: 'visited', seen_count: 1 };
    termIndex._c = null; updateMineCount(); persistLocal(); maybeCTA(); focus(n.id); toast('palavra sua adicionada (local)');
  }
  $('#add-word').addEventListener('click', function () {
    var term = prompt('Nova palavra sua (fora do léxico):'); if (!term) return;
    var gloss = prompt('Significado (opcional):') || null;
    if (authed()) {
      J('/api/v1/me/topologia/no', { method: 'POST', headers: headers(true), body: JSON.stringify({ term: term, gloss: gloss }) })
        .then(function (n) { if (n && n.id) { byId[n.id] = n; ensureNode(n); myWords[n.id] = { status: 'visited', seen_count: 1 }; termIndex._c = null; updateMineCount(); focus(n.id); toast('palavra sua adicionada'); } else toast('falhou'); });
    } else addLocalNode(term, gloss);
  });
  $('#add-text').addEventListener('click', function () {
    var text = prompt('Escreva seu texto (vira seu corpus pessoal):'); if (!text) return;
    var title = prompt('Título (opcional):') || null;
    if (authed()) {
      J('/api/v1/me/topologia/texto', { method: 'POST', headers: headers(true), body: JSON.stringify({ title: title, text: text }) })
        .then(function (r) { if (r) { J('/api/v1/me/topologia/textos', { headers: headers() }).then(function (ts) { myTexts = ts || []; renderSentences(''); $('#palette').classList.add('open'); toast('texto adicionado ao seu corpus'); }); } else toast('falhou'); });
    } else {
      myTexts.unshift({ id: 'localtext:' + myTexts.length, lang: 'meu', loc: title || 'meu texto', text: text, terms: matchClient(text) });
      renderSentences(''); $('#palette').classList.add('open'); persistLocal(); maybeCTA(); toast('texto no seu corpus (local)');
    }
  });
  $('#search').addEventListener('input', function (e) {
    renderSentences(e.target.value);
    if (!$('#palette').classList.contains('open')) $('#palette').classList.add('open');
  });

  // ── toast ────────────────────────────────────────────────────────────────────
  var toTimer = null;
  function toast(msg) {
    var t = $('#toast'); t.textContent = msg; t.classList.add('show');
    clearTimeout(toTimer); toTimer = setTimeout(function () { t.classList.remove('show'); }, 2600);
  }

  // ── boot ───────────────────────────────────────────────────────────────────
  function boot() {
    resize();
    function loadServerPersonal() {
      J('/api/v1/me/topologia', { headers: headers() }).then(function (m) {
        if (!m) return;
        (m.words || []).forEach(function (w) { myWords[w.node_id] = { status: w.status, seen_count: w.seen_count }; });
        (m.nodes || []).forEach(function (n) { byId[n.id] = n; ensureNode(n); termIndex._c = null; });
        (m.edges || []).forEach(function (e) { edges.push(e); });
        updateMineCount();
      });
      J('/api/v1/me/topologia/textos', { headers: headers() }).then(function (ts) {
        myTexts = ts || []; renderSentences($('#search').value || '');
      });
    }
    if (authed()) {
      document.body.classList.add('authed');
      $('#loginlink').hidden = true;
      // migra o cache do tier grátis (se houver) → conta, depois carrega do servidor.
      migrateLocal().then(loadServerPersonal);
    } else {
      // tier grátis: cresce SEM conta, no cache local do navegador.
      var l = loadLocal();
      if (l) {
        myWords = l.words || {};
        (l.nodes || []).forEach(function (n) { byId[n.id] = n; myNodeList.push(n); ensureNode(n); });
        (l.edges || []).forEach(function (e) { myEdgeList.push(e); edges.push(e); });
        myTexts = l.texts || [];
        termIndex._c = null; updateMineCount(); maybeCTA();
      }
    }
    // LOOKUP só (não renderiza): id→nó, p/ resolver as palavras de uma sentença.
    J(API + '/nos').then(function (cat) {
      (cat || []).forEach(function (n) { byId[n.id] = n; });
    });
    // entrada "ler primeiro": lista de sentenças (versos do Ayvu Rapytã).
    J(API + '/sentencas?lang=gn-mbya&limit=300').then(function (ss) {
      sentList = ss || [];
      renderSentences('');
      $('#palette').classList.add('open');
      toast(sentList.length + ' sentenças (Ayvu Rapytã) — escolha uma para ler e ver suas palavras');
    });
    updateMineCount(); // estado vazio: esconde "minhas"/"só minhas" até a 1ª reivindicação
    if (!localStorage.getItem('ygg-help-seen')) openHelp(); // guia no 1º acesso
    loop();
  }

  // Enquadra todos os nós no viewport (margem 12%).
  function fitView() {
    var ids = Object.keys(nodes);
    if (!ids.length) return;
    var minx = Infinity, miny = Infinity, maxx = -Infinity, maxy = -Infinity;
    ids.forEach(function (id) {
      var n = nodes[id];
      if (n.x < minx) minx = n.x; if (n.x > maxx) maxx = n.x;
      if (n.y < miny) miny = n.y; if (n.y > maxy) maxy = n.y;
    });
    var w = Math.max(1, maxx - minx), h = Math.max(1, maxy - miny);
    view.k = Math.min(W / w, H / h) * 0.88;
    view.x = W / 2 - ((minx + maxx) / 2) * view.k;
    view.y = H / 2 - ((miny + maxy) / 2) * view.k;
  }
  boot();
})();
