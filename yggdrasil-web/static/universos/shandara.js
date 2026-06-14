/* /universos/shandara — reader do SRD (YG-144). Árvore de /api/v1/shandara/srd;
 * cada doc é Markdown cru de /srd/{path}, renderizado client-side. Deep-link via
 * #path (ex.: #mundo/grande-guerra). */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var esc = function (s) { return String(s == null ? '' : s).replace(/[&<>]/g, function (c) {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]; }); };

  // renderizador Markdown compacto (headings, bold, listas, blockquote, hr, links, código inline)
  function md(src) {
    var lines = String(src || '').replace(/\r/g, '').split('\n');
    var html = '', inUl = false, inOl = false, para = [];
    function flushPara() {
      if (para.length) { html += '<p>' + inline(para.join(' ')) + '</p>'; para = []; }
    }
    function closeLists() {
      if (inUl) { html += '</ul>'; inUl = false; }
      if (inOl) { html += '</ol>'; inOl = false; }
    }
    function inline(t) {
      t = esc(t);
      t = t.replace(/`([^`]+)`/g, '<code>$1</code>');
      t = t.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
      t = t.replace(/\*([^*]+)\*/g, '<em>$1</em>');
      t = t.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
      return t;
    }
    lines.forEach(function (raw) {
      var l = raw.replace(/\s+$/, '');
      var h = /^(#{1,4})\s+(.*)$/.exec(l);
      if (h) { flushPara(); closeLists(); html += '<h' + h[1].length + '>' + inline(h[2]) + '</h' + h[1].length + '>'; return; }
      if (/^>\s?/.test(l)) { flushPara(); closeLists(); html += '<blockquote>' + inline(l.replace(/^>\s?/, '')) + '</blockquote>'; return; }
      if (/^(-{3,}|\*{3,})$/.test(l)) { flushPara(); closeLists(); html += '<hr>'; return; }
      var ul = /^[-*]\s+(.*)$/.exec(l);
      if (ul) { flushPara(); if (inOl) { html += '</ol>'; inOl = false; } if (!inUl) { html += '<ul>'; inUl = true; } html += '<li>' + inline(ul[1]) + '</li>'; return; }
      var ol = /^\d+\.\s+(.*)$/.exec(l);
      if (ol) { flushPara(); if (inUl) { html += '</ul>'; inUl = false; } if (!inOl) { html += '<ol>'; inOl = true; } html += '<li>' + inline(ol[1]) + '</li>'; return; }
      if (l.trim() === '') { flushPara(); closeLists(); return; }
      para.push(l);
    });
    flushPara(); closeLists();
    return html;
  }

  var docs = [];
  function abrir(path) {
    docs.forEach(function (d) { /* destaque */ });
    document.querySelectorAll('#toc a').forEach(function (a) { a.classList.toggle('on', a.dataset.path === path); });
    history.replaceState(null, '', '#' + path);
    $('doc').innerHTML = '<p class="empty">carregando…</p>';
    fetch('/api/v1/shandara/srd/' + path).then(function (r) { return r.ok ? r.text() : null; })
      .then(function (t) {
        $('doc').innerHTML = t ? md(t) : '<p class="empty">Doc não encontrado.</p>';
        $('doc').scrollIntoView({ block: 'start', behavior: 'smooth' });
        if (window.yggTelemetria) window.yggTelemetria.track('shandara_doc', { path: path });
      });
  }

  fetch('/api/v1/shandara/srd').then(function (r) { return r.ok ? r.json() : []; })
    .then(function (tree) {
      docs = tree || [];
      if (!docs.length) { $('toc').innerHTML = '<p class="empty">SRD indisponível.</p>'; return; }
      // agrupa por seção, preservando a ordem de chegada
      var bySec = {}; var order = [];
      docs.forEach(function (d) { if (!bySec[d.section]) { bySec[d.section] = []; order.push(d.section); } bySec[d.section].push(d); });
      $('toc').innerHTML = order.map(function (sec) {
        return '<div class="sec">' + esc(sec) + '</div>' + bySec[sec].map(function (d) {
          return '<a data-path="' + esc(d.path) + '" href="#' + esc(d.path) + '">' + esc(d.title) + '</a>';
        }).join('');
      }).join('');
      $('toc').querySelectorAll('a').forEach(function (a) {
        a.addEventListener('click', function (e) { e.preventDefault(); abrir(a.dataset.path); });
      });
      // deep-link ou index por padrão
      var hash = decodeURIComponent((location.hash || '').replace(/^#/, ''));
      var alvo = docs.find(function (d) { return d.path === hash; }) ? hash : (docs[0] && docs[0].path);
      if (alvo) abrir(alvo);
    });
})();
