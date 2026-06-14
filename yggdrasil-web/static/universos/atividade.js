/* atividade.js — painel "lugar vivo" por universo (YG-145, fatia 3).
 *
 * Mostra ATIVOS AGORA + ÚLTIMAS ATIVIDADES de um universo. Lê o snapshot de
 * /api/v1/universos/{id}/atividade e atualiza ao vivo pelo pulso WS
 * (/api/v1/analytics/stream). Universo vem de `data-universo` no <script>.
 * Uso: <script defer src="/static/universos/atividade.js" data-universo="snake"></script>
 */
(function () {
  'use strict';
  var sc = document.currentScript;
  var U = (sc && sc.dataset && sc.dataset.universo) || '';
  if (!U) return;

  var esc = function (s) {
    return String(s == null ? '' : s).replace(/[&<>]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c];
    });
  };

  // ── tempo relativo curto (pt-BR) ──
  function haQuanto(ts) {
    var d = Math.max(0, Date.now() - (ts || 0));
    var s = Math.floor(d / 1000);
    if (s < 10) return 'agora';
    if (s < 60) return s + 's';
    var m = Math.floor(s / 60);
    if (m < 60) return m + 'min';
    var h = Math.floor(m / 60);
    if (h < 24) return h + 'h';
    return Math.floor(h / 24) + 'd';
  }
  function dur(ms) {
    var s = Math.floor((ms || 0) / 1000);
    if (s < 60) return s + 's';
    var m = Math.floor(s / 60);
    return m + 'min';
  }

  function linhaAtividade(a) {
    var ts = a.ts || 0;
    if (a.ev === 'partida') {
      var r = a.resultado === 'vitoria' ? '🏆 vitória'
        : a.resultado === 'levantou' ? '🪑 levantou da mesa'
        : '🏁 partida';
      var pts = (a.score != null) ? ' · ' + a.score + ' pts' : '';
      return '<li><span class="ev">' + r + pts + '</span><span class="ha">' + haQuanto(ts) + '</span></li>';
    }
    if (a.ev === 'saida') {
      var d = (a.ativo_ms != null) ? ' · ' + dur(a.ativo_ms) : '';
      return '<li class="dim"><span class="ev">← saiu' + d + '</span><span class="ha">' + haQuanto(ts) + '</span></li>';
    }
    return '<li class="dim"><span class="ev">→ entrou</span><span class="ha">' + haQuanto(ts) + '</span></li>';
  }

  // ── painel flutuante ──
  var box = document.createElement('aside');
  box.id = 'ygg-atividade';
  box.innerHTML =
    '<style>' +
    '#ygg-atividade{position:fixed;right:12px;bottom:12px;width:230px;max-width:78vw;' +
    'background:rgba(13,13,18,.92);border:1px solid #2b2620;border-radius:10px;' +
    'font:12px/1.4 system-ui,sans-serif;color:#e8e3d3;z-index:50;backdrop-filter:blur(6px);' +
    'box-shadow:0 6px 24px rgba(0,0,0,.4)}' +
    '#ygg-atividade .hd{display:flex;align-items:center;gap:.4rem;padding:.5rem .7rem;' +
    'border-bottom:1px solid #2b2620;cursor:pointer;user-select:none}' +
    '#ygg-atividade .dot{width:8px;height:8px;border-radius:50%;background:#34d399;' +
    'box-shadow:0 0 8px #34d399;flex:none}' +
    '#ygg-atividade .dot.off{background:#555;box-shadow:none}' +
    '#ygg-atividade .n{font-weight:700;color:#d4af37}' +
    '#ygg-atividade .tt{flex:1;color:#a9a99a;font-size:11px}' +
    '#ygg-atividade .tg{color:#a9a99a;font-size:14px;line-height:1}' +
    '#ygg-atividade ul{list-style:none;margin:0;padding:.3rem .5rem;max-height:180px;overflow-y:auto}' +
    '#ygg-atividade li{display:flex;justify-content:space-between;gap:.5rem;padding:.18rem .2rem}' +
    '#ygg-atividade li.dim{color:#a9a99a}' +
    '#ygg-atividade .ha{color:#6f6f63;flex:none;font-size:11px}' +
    '#ygg-atividade .vazio{color:#6f6f63;padding:.4rem .2rem;font-style:italic}' +
    '#ygg-atividade.col ul{display:none}' +
    '@media(max-width:600px){#ygg-atividade{width:190px}}' +
    '</style>' +
    '<div class="hd"><span class="dot off" id="ygg-dot"></span>' +
    '<span class="n" id="ygg-n">0</span><span class="tt">ativos agora</span>' +
    '<span class="tg" id="ygg-tg">▾</span></div>' +
    '<ul id="ygg-ul"><li class="vazio">carregando…</li></ul>';

  var ativos = 0, ultimas = [];
  function setAtivos(n) {
    ativos = (n == null ? 0 : n) | 0;
    var el = document.getElementById('ygg-n'); if (el) el.textContent = ativos;
    var dot = document.getElementById('ygg-dot'); if (dot) dot.classList.toggle('off', ativos <= 0);
  }
  function setUltimas(list) {
    ultimas = list || [];
    var ul = document.getElementById('ygg-ul'); if (!ul) return;
    ul.innerHTML = ultimas.length
      ? ultimas.map(linhaAtividade).join('')
      : '<li class="vazio">nada ainda — seja o primeiro</li>';
  }

  function carregar() {
    fetch('/api/v1/universos/' + encodeURIComponent(U) + '/atividade')
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (d) {
        if (!d) return;
        setAtivos(d.ativos_agora);
        setUltimas(d.ultimas);
      })
      .catch(function () {});
  }

  // refetch das "últimas" com debounce quando o pulso sinaliza mudança
  var refTimer = null;
  function refetchSoon() {
    if (refTimer) return;
    refTimer = setTimeout(function () { refTimer = null; carregar(); }, 1200);
  }

  function ligarWs() {
    try {
      var proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      var ws = new WebSocket(proto + '//' + location.host + '/api/v1/analytics/stream');
      ws.onmessage = function (e) {
        var m; try { m = JSON.parse(e.data); } catch (_) { return; }
        var frames = m && m.ev === 'snapshot' ? (m.eventos || []) : [m];
        frames.forEach(function (f) {
          if (!f) return;
          if (f.ev === 'presenca' && f.universo === U) { setAtivos(f.ativos); refetchSoon(); }
          else if (f.ev === 'stats' && f.presenca) { setAtivos(f.presenca[U] || 0); }
        });
      };
      // reconnect simples
      ws.onclose = function () { setTimeout(ligarWs, 5000); };
    } catch (_) { /* sem WS → fica no polling */ }
  }

  function montar() {
    document.body.appendChild(box);
    document.querySelector('#ygg-atividade .hd').addEventListener('click', function () {
      box.classList.toggle('col');
      var tg = document.getElementById('ygg-tg'); if (tg) tg.textContent = box.classList.contains('col') ? '▸' : '▾';
    });
    carregar();
    ligarWs();
    // mantém o "há quanto" fresco
    setInterval(function () { if (!box.classList.contains('col')) setUltimas(ultimas); }, 30000);
  }

  if (document.body) montar();
  else window.addEventListener('DOMContentLoaded', montar);
})();
