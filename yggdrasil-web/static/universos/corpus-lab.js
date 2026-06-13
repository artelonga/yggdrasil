/* Laboratório de corpus (YG-139) — frequência + comparação cross-linguística.
 * Consome /api/v1/corpus, /corpus/{name}/freq e /corpus/compare (DuckDB). */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var esc = function (s) {
    return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  };
  var fmt = function (n) { return n == null ? '—' : Number(n).toLocaleString('pt-BR'); };
  function J(u) { return fetch(u).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }); }

  var mode = 'inner';

  function carregarFreq() {
    var a = $('sel-a').value;
    $('freq-title').textContent = 'Frequência de ' + a;
    J('/api/v1/corpus/' + encodeURIComponent(a) + '/freq?limit=25').then(function (rows) {
      rows = rows || [];
      $('freq-empty').hidden = rows.length > 0;
      $('freq').innerHTML = rows.map(function (r) {
        return '<tr><td class="n">' + r.rank + '</td><td class="t">' + esc(r.term) +
          '</td><td class="n">' + fmt(r.count) + '</td></tr>';
      }).join('');
    });
  }

  function carregarComparacao() {
    var a = $('sel-a').value, b = $('sel-b').value;
    $('cmp-title').textContent = a + ' ⟷ ' + b + '  ·  ' + mode;
    J('/api/v1/corpus/compare?a=' + encodeURIComponent(a) + '&b=' + encodeURIComponent(b) +
      '&mode=' + mode + '&limit=25').then(function (rows) {
      rows = rows || [];
      $('cmp-empty').hidden = rows.length > 0;
      $('cmp').innerHTML = rows.map(function (r) {
        var eq = r.term_b
          ? '<td class="eq">' + esc(r.term_b) + '</td><td class="n">' + fmt(r.count_b) + '</td>'
          : '<td class="none">— sem equivalente</td><td class="n">—</td>';
        return '<tr><td class="t">' + esc(r.term_a) + '</td><td class="n">' + fmt(r.count_a) + '</td>' + eq + '</tr>';
      }).join('');
    });
  }

  function recarregar() { carregarFreq(); carregarComparacao(); }

  $('mode').querySelectorAll('button').forEach(function (b) {
    b.addEventListener('click', function () {
      mode = b.dataset.mode;
      $('mode').querySelectorAll('button').forEach(function (x) { x.classList.toggle('on', x === b); });
      carregarComparacao();
    });
  });
  $('sel-a').addEventListener('change', recarregar);
  $('sel-b').addEventListener('change', carregarComparacao);

  // popula os seletores com os corpora disponíveis
  J('/api/v1/corpus').then(function (cs) {
    cs = cs || [];
    if (!cs.length) {
      $('freq-empty').hidden = false;
      $('freq-empty').textContent = 'Nenhum corpus disponível.';
      return;
    }
    var opts = cs.map(function (c) {
      return '<option value="' + esc(c.name) + '">' + esc(c.name) + ' (' + esc(c.lang) + ', ' + fmt(c.terms) + ')</option>';
    }).join('');
    $('sel-a').innerHTML = opts;
    $('sel-b').innerHTML = opts;
    // defaults úteis: A = ayvu-rapyta (ou 1º), B = yoruba-lexico (ou 2º)
    var nomes = cs.map(function (c) { return c.name; });
    $('sel-a').value = nomes.indexOf('mbya-lexico') >= 0 ? 'mbya-lexico' : nomes[0];
    $('sel-b').value = nomes.indexOf('yoruba-lexico') >= 0 ? 'yoruba-lexico' : (nomes[1] || nomes[0]);
    recarregar();
  });
})();
