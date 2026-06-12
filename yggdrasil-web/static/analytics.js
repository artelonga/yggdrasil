/* /analytics — seção pública de analytics ao vivo (YG-128, padrão do dashboard
 * da ArteLonga). Duas fontes, ambas anônimas:
 *   - hub do CO: summary?universe=yggdrasil (views/visitantes/timeseries/top/geo,
 *     PII-stripped, cache 5min server-side);
 *   - local: /api/v1/stats (jogando agora, sessões 24h) + /api/v1/scores/recent.
 * "Ao vivo" via polling (stats 10s, placares 20s, summary 60s); o stream WS é o
 * resto da YG-128. Falha de qualquer fonte degrada em silêncio. */
(function () {
  'use strict';

  var CO = 'https://co.artelonga.com.br/api/v1/analytics/public';
  var $ = function (id) { return document.getElementById(id); };
  var fmt = function (n) { return n == null ? '—' : Number(n).toLocaleString('pt-BR'); };
  var esc = function (s) {
    return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  };
  function getJSON(url) {
    return fetch(url).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; });
  }

  // ── stats locais de jogo (vivos) ───────────────────────────────────────────
  function loadLive() {
    getJSON('/api/v1/stats').then(function (s) {
      if (!s) return;
      $('k-agora').textContent = fmt(s.jogando_agora);
      $('k-ses24').textContent = fmt(s.sessoes_24h);
    });
  }

  function loadScores() {
    getJSON('/api/v1/scores/top?limit=8').then(function (d) {
      var scores = (d && d.scores) || [];
      var ul = $('recent-scores');
      $('scores-empty').hidden = scores.length > 0;
      ul.innerHTML = scores.map(function (s) {
        return '<li><span class="t">' + esc(s.game) + '</span>' +
          '<span class="v">' + fmt(s.score) + '</span>' +
          '<span>' + esc(String(s.user_id || 'anônimo').split('@')[0]) + '</span></li>';
      }).join('');
    });
  }

  // ── summary do CO (site) ───────────────────────────────────────────────────
  function loadSummary() {
    var days = $('days').value;
    getJSON(CO + '/summary?universe=yggdrasil&days=' + encodeURIComponent(days)).then(function (d) {
      if (!d) return;
      $('k-views').textContent = fmt(d.views);
      $('k-visitors').textContent = fmt(d.visitors);
      $('k-sessions').textContent = fmt(d.sessions);
      $('k-countries').textContent = fmt(d.countries);
      drawChart(d.timeseries || []);
      renderPages(d.top_pages || []);
      renderGeo(d.geo || []);
    });
  }

  function drawChart(ts) {
    var canvas = $('chart');
    $('chart-empty').hidden = ts.length > 0;
    var ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (!ts.length) return;
    var max = Math.max.apply(null, ts.map(function (p) { return p.count || 0; })) || 1;
    var w = canvas.width / ts.length;
    ctx.fillStyle = '#d4af37';
    ts.forEach(function (p, i) {
      var h = Math.max(2, ((p.count || 0) / max) * (canvas.height - 26));
      ctx.globalAlpha = 0.85;
      ctx.fillRect(i * w + 2, canvas.height - 18 - h, Math.max(2, w - 4), h);
    });
    ctx.globalAlpha = 0.5;
    ctx.fillStyle = '#e8e3d3';
    ctx.font = '10px system-ui';
    ctx.textAlign = 'left';
    ctx.fillText((ts[0].bucket || '').slice(5), 2, canvas.height - 5);
    ctx.textAlign = 'right';
    ctx.fillText((ts[ts.length - 1].bucket || '').slice(5), canvas.width - 2, canvas.height - 5);
    ctx.globalAlpha = 1;
  }

  function renderPages(pages) {
    var tb = $('top-pages');
    $('pages-empty').hidden = pages.length > 0;
    var max = Math.max.apply(null, pages.map(function (p) { return p.views || 0; })) || 1;
    tb.innerHTML = pages.slice(0, 12).map(function (p) {
      var pct = (((p.views || 0) / max) * 100).toFixed(0);
      return '<tr><td class="path">' + esc(p.path) + '</td>' +
        '<td><span class="bar" style="width:' + pct + 'px"></span></td>' +
        '<td class="num">' + fmt(p.views) + '</td>' +
        '<td class="num">' + fmt(p.visitors) + '</td></tr>';
    }).join('');
  }

  function renderGeo(geo) {
    var tb = $('geo');
    $('geo-empty').hidden = geo.length > 0;
    tb.innerHTML = geo.slice(0, 8).map(function (g) {
      var lugar = [g.city, g.country].filter(Boolean).join(', ') || '—';
      return '<tr><td>' + esc(lugar) + '</td><td class="num">' + fmt(g.visitors) + '</td></tr>';
    }).join('');
  }

  $('days').addEventListener('change', loadSummary);

  // ── stream ao vivo (YG-128): stats em <1s via WS; polling fica de fallback ──
  function ligarStream() {
    try {
      var proto = location.protocol === 'https:' ? 'wss://' : 'ws://';
      var ws = new WebSocket(proto + location.host + '/api/v1/analytics/stream');
      ws.onmessage = function (m) {
        try {
          var d = JSON.parse(m.data);
          var um = function (f) { if (f.ev === 'stats') {
            $('k-agora').textContent = fmt(f.jogando_agora);
            $('k-ses24').textContent = fmt(f.sessoes_24h);
          } };
          if (d.ev === 'snapshot') (d.eventos || []).forEach(um); else um(d);
        } catch (_) { /* frame estranho → ignora */ }
      };
      ws.onclose = function () { setTimeout(ligarStream, 15_000); }; // reconecta
    } catch (_) { /* sem WS → polling cobre */ }
  }
  ligarStream();

  loadLive(); loadScores(); loadSummary();
  setInterval(loadLive, 10_000);
  setInterval(loadScores, 20_000);
  setInterval(loadSummary, 60_000);
})();
