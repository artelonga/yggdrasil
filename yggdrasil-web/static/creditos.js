/* /creditos — rol de apoiadores da campanha (YG-161). Lê GET /api/v1/creditos
 * (só opt-in; nunca e-mail). Agrupa por tier na ordem da árvore e expõe
 * window.Creditos para testes determinísticos (e2e). */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var esc = function (s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };

  // Ordem da árvore (da semente ao topo) + rótulo de exibição.
  var ORDEM = [
    ['semente', 'Semente'], ['raiz', 'Raiz'], ['galho', 'Galho'],
    ['folhagem', 'Folhagem'], ['tronco', 'Tronco'], ['yggdrasil', 'Yggdrasil'],
  ];

  function cardHtml(c) {
    var nome = (c.nome && c.nome.trim()) ? c.nome : 'Apoiador anônimo';
    return '<div class="cred">' +
      '<div class="nome">' + esc(nome) + (c.confirmado ? '<span class="chk" title="apoio confirmado">✓</span>' : '') + '</div>' +
      (c.mensagem ? '<div class="msg">“' + esc(c.mensagem) + '”</div>' : '') +
    '</div>';
  }

  function render(creditos) {
    var roll = $('roll');
    var porTier = {};
    creditos.forEach(function (c) { (porTier[c.tier] = porTier[c.tier] || []).push(c); });

    var html = '';
    ORDEM.forEach(function (pair) {
      var slug = pair[0], label = pair[1];
      var items = porTier[slug];
      if (!items || !items.length) return;
      html += '<h2 class="tier">' + esc(label) + ' <small style="opacity:.55;font-size:.7em">·' +
        ' ' + items.length + '</small></h2>' +
        '<div class="roll">' + items.map(cardHtml).join('') + '</div>';
    });

    $('loading').hidden = true;
    if (!html) { $('empty').hidden = false; roll.innerHTML = ''; return; }
    roll.innerHTML = html;
  }

  function load() {
    return fetch('/api/v1/creditos')
      .then(function (r) { return r.ok ? r.json() : []; })
      .catch(function () { return []; })
      .then(function (creditos) {
        if (!Array.isArray(creditos)) creditos = [];
        render(creditos);
        window.Creditos = { count: creditos.length, data: creditos }; // hook de teste
        return creditos;
      });
  }

  window.Creditos = { count: null, data: null, reload: load };
  load();
})();
