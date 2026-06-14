/* telemetria.js — tracker privacy-first do Yggdrasil (YG-127 Fase 1).
 *
 * Mesmo contrato do tracker da ArteLonga (docs/analytics-api.md do site):
 * batches `{schema:1, batch:[...]}` para o hub de analytics do CO, que mapeia
 * `site` → universe_key ("yggdrasil") e aplica as garantias server-side
 * (ip_hash com salt diário, filtro de bots, retenção 90d). Aqui:
 *   - zero PII: vid/sid são tokens opacos aleatórios; nada de e-mail/sub;
 *   - respeita Do-Not-Track e opt-out (localStorage ygg_optout=1);
 *   - best-effort: fila local, sendBeacon no pagehide, falha é silêncio.
 * Eventos: page_view, page_end (duração), js_error. Apps podem emitir os seus
 * via window.yggTelemetria.track(nome, props) — ex.: view trocada no player.
 */
(function () {
  'use strict';
  if (window.yggTelemetria) return;

  var ENDPOINT = 'https://co.artelonga.com.br/api/v1/telemetry/events';
  var SITE = 'yggdrasil';
  var QKEY = 'ygg_tq';
  var BATCH_MS = 5000;
  var MAX_Q = 200;

  function optedOut() {
    try {
      if (localStorage.getItem('ygg_optout') === '1') return true;
    } catch (_) { /* sem LS → segue */ }
    var dnt = navigator.doNotTrack || window.doNotTrack || navigator.msDoNotTrack;
    if (dnt === '1' || dnt === 'yes') return true;
    if (navigator.webdriver) return true; // browsers automatizados
    return false;
  }
  if (optedOut()) {
    var noop = function () {};
    window.yggTelemetria = {
      track: noop,
      atividade: function () { return { partida: noop, sair: noop }; },
      optedOut: true,
    };
    return;
  }

  function rnd(n) {
    var out = '';
    var abc = 'abcdefghijklmnopqrstuvwxyz0123456789';
    for (var i = 0; i < n; i++) out += abc[Math.floor(Math.random() * abc.length)];
    return out;
  }
  function getVid() {
    try {
      var v = localStorage.getItem('ygg_vid');
      if (!v) { v = rnd(24); localStorage.setItem('ygg_vid', v); }
      return v;
    } catch (_) { return rnd(24); }
  }
  function getSid() {
    try {
      var s = sessionStorage.getItem('ygg_sid');
      if (!s) { s = rnd(16); sessionStorage.setItem('ygg_sid', s); }
      return s;
    } catch (_) { return rnd(16); }
  }
  var VID = getVid(), SID = getSid();

  function loadQ() {
    try { return JSON.parse(localStorage.getItem(QKEY)) || []; } catch (_) { return []; }
  }
  function saveQ(q) {
    try { localStorage.setItem(QKEY, JSON.stringify(q.slice(-MAX_Q))); } catch (_) { /* cheio */ }
  }

  var flushTimer = null;
  function scheduleFlush(ms) {
    if (flushTimer) return;
    flushTimer = setTimeout(flush, ms || BATCH_MS);
  }
  function payload(batch) {
    return JSON.stringify({ schema: 1, batch: batch });
  }
  function flush() {
    flushTimer = null;
    var q = loadQ();
    if (!q.length) return;
    saveQ([]);
    fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: payload(q),
      keepalive: true,
    }).catch(function () {
      // offline/CO fora: devolve à fila (cap MAX_Q) e tenta no próximo evento
      saveQ(loadQ().concat(q));
    });
  }
  function flushBeacon() {
    var q = loadQ();
    if (!q.length) return;
    if (navigator.sendBeacon &&
        navigator.sendBeacon(ENDPOINT, new Blob([payload(q)], { type: 'application/json' }))) {
      saveQ([]);
    }
  }

  function track(name, props) {
    try {
      var ev = {
        s: 1,
        site: SITE,
        name: String(name || 'unknown').slice(0, 64),
        sid: SID,
        vid: VID,
        ts: Date.now(),
        tz: (typeof Intl !== 'undefined' && Intl.DateTimeFormat)
          ? (Intl.DateTimeFormat().resolvedOptions().timeZone || null) : null,
        path: location.pathname,
        query: null, // querystrings podem carregar estado — fora por padrão
        ref: document.referrer || null,
        vw: window.innerWidth,
        vh: window.innerHeight,
        lang: navigator.language || null,
        ua_brand: (navigator.userAgentData && navigator.userAgentData.brands)
          ? navigator.userAgentData.brands.map(function (b) { return b.brand; }).join(',') : null,
        utm: null,
        experiments: null,
        props: props || {},
      };
      var q = loadQ();
      q.push(ev);
      saveQ(q);
      scheduleFlush();
    } catch (_) { /* telemetria nunca quebra a página */ }
  }

  // ── atividade/presença (YG-145) ─────────────────────────────────────────────
  // O primitivo é PRESENÇA (entrar/sair), não partida/fim. Jogos sem fim (RPG,
  // poker colaborativo) só têm entrada/pulso/saída; jogos finitos emitem `partida`
  // por cima quando há um resultado (morte no snake, levantar da mesa). Mesma
  // taxonomia para todos, no contrato anônimo da ArteLonga. Uso na página do jogo:
  //   var atv = window.yggTelemetria.atividade('snake');
  //   ... atv.partida({ score: 42, resultado: 'fim' });  // opcional
  var PULSO_MS = 45000;
  // Ping de presença LOCAL (intra-app, para o painel/lobby ao vivo). Separado do
  // track() acima, que vai ao hub do CO (agregado anônimo). Mesmo vid opaco.
  function pingPresenca(universo, ev, extra) {
    try {
      var body = JSON.stringify(Object.assign(
        { universo: universo, vid: VID, ev: ev }, extra || {}));
      if (ev === 'saida' && navigator.sendBeacon) {
        navigator.sendBeacon('/api/v1/presenca',
          new Blob([body], { type: 'application/json' }));
        return;
      }
      fetch('/api/v1/presenca', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: body,
        keepalive: true,
      }).catch(function () {});
    } catch (_) { /* presença nunca quebra a página */ }
  }
  function atividade(universo) {
    universo = String(universo || '').slice(0, 64);
    if (!universo) return { partida: function () {}, sair: function () {} };
    var entrou = Date.now();
    var saiu = false;
    track('entrada', { universo: universo });
    pingPresenca(universo, 'entrada');
    var hb = setInterval(function () {
      // pulso só enquanto a aba está visível — mantém viva a presença de jogos
      // sem fim sem inflar tempo de aba em background.
      if (document.visibilityState !== 'hidden') {
        track('pulso', { universo: universo });
        pingPresenca(universo, 'pulso');
      }
    }, PULSO_MS);
    function sair() {
      if (saiu) return;
      saiu = true;
      clearInterval(hb);
      var ativo = Date.now() - entrou;
      track('saida', { universo: universo, ativo_ms: ativo });
      pingPresenca(universo, 'saida', { ativo_ms: ativo });
      flushBeacon();
    }
    window.addEventListener('pagehide', sair);
    return {
      partida: function (props) {
        props = props || {};
        track('partida', Object.assign({ universo: universo }, props));
        pingPresenca(universo, 'partida', props);
      },
      sair: sair,
    };
  }

  // ── eventos automáticos ────────────────────────────────────────────────────
  var t0 = Date.now();
  track('page_view', { title: (document.title || '').slice(0, 128) });

  window.addEventListener('pagehide', function () {
    track('page_end', { dur_ms: Date.now() - t0 });
    flushBeacon();
  });
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'hidden') flushBeacon();
  });
  window.addEventListener('error', function (e) {
    track('js_error', {
      msg: String((e && e.message) || '').slice(0, 256),
      src: String((e && e.filename) || '').slice(0, 128),
      line: (e && e.lineno) || 0,
    });
  });

  window.yggTelemetria = { track: track, atividade: atividade, optedOut: false };
})();
