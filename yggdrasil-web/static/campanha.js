/* /campanha — landing de campanha (YG-143 + YG-161). A prosa das recompensas
 * espelha docs/REWARDS.md (aqui); preço/slug vêm do backend
 * (/api/v1/campanha/tiers, fonte canônica) com fallback aos preços locais.
 * O CTA registra um APOIO (pledge) independente em /api/v1/campanha/apoiar —
 * crowdfunding próprio, sem Catarse; nada é cobrado aqui (PIX vem depois). */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var esc = function (s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };
  var fmt = function (n) { return n == null ? '—' : Number(n).toLocaleString('pt-BR'); };
  function J(u) { return fetch(u).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }); }
  function token() { try { return localStorage.getItem('yggdrasil-jwt'); } catch (e) { return null; } }

  // Prosa das recompensas (espelha docs/REWARDS.md). preco/slug aqui são fallback;
  // o backend (/api/v1/campanha/tiers) é a fonte canônica de preço quando responde.
  var TIERS = [
    { slug: 'semente', nome: 'Semente', preco: 25, rec: ['Acesso à v1.0 + nome nos créditos', 'Newsletter mensal de desenvolvimento'] },
    { slug: 'raiz', nome: 'Raiz', preco: 60, rec: ['Tudo da Semente', 'Skin "raiz dourada" + 1.000 sementes', 'Selo de apoiador'] },
    { slug: 'galho', nome: 'Galho', preco: 120, rec: ['Tudo da Raiz', 'Early access (3 meses)', 'Universo privado de mestres + sala de feedback'] },
    { slug: 'folhagem', nome: 'Folhagem', preco: 250, hl: true, rec: ['Tudo do Galho', '3 packs de skins (medieval / cyberpunk / folclore BR)', 'Early access vitalício'] },
    { slug: 'tronco', nome: 'Tronco', preco: 500, limite: '50 vagas', rec: ['Tudo da Folhagem', 'Closed beta de universos 3D', 'NPC nomeado em universo público'] },
    { slug: 'yggdrasil', nome: 'Yggdrasil', preco: 1500, limite: '10 vagas', rec: ['Tudo do Tronco', 'Universo 3D personalizado co-criado + 1h mentoria', 'Sem cobrança de premium futuro'] },
  ];

  function renderTiers() {
    $('tiers').innerHTML = TIERS.map(function (t) {
      return '<div class="tier' + (t.hl ? ' hl' : '') + '">' +
        '<div class="tname">' + esc(t.nome) + '</div>' +
        '<div class="price">R$ ' + t.preco + ' <small>apoio</small></div>' +
        (t.limite ? '<div class="limit">' + esc(t.limite) + '</div>' : '') +
        '<ul>' + t.rec.map(function (r) { return '<li>' + esc(r) + '</li>'; }).join('') + '</ul>' +
        '<button data-tier="' + esc(t.slug) + '" data-nome="' + esc(t.nome) + '">Apoiar</button>' +
      '</div>';
    }).join('');
    $('tiers').querySelectorAll('button[data-tier]').forEach(function (b) {
      b.addEventListener('click', function () { abrirApoio(b.dataset.tier, b.dataset.nome); });
    });
  }

  // Preço canônico do backend sobrescreve o fallback local (slug casa os dois).
  J('/api/v1/campanha/tiers').then(function (rows) {
    if (Array.isArray(rows)) {
      rows.forEach(function (bt) {
        var local = TIERS.find(function (t) { return t.slug === bt.slug; });
        if (local && typeof bt.preco === 'number') local.preco = bt.preco;
      });
    }
    renderTiers();
  });

  // ── stats ao vivo (credibilidade: a plataforma já existe) ──
  Promise.all([J('/api/v1/stats'), J('/api/v1/universos'), J('/api/v1/corpus')]).then(function (r) {
    var stats = r[0] || {}, universos = r[1] || [], corpora = r[2] || [];
    $('s-uni').textContent = fmt(universos.length || null);
    $('s-jog').textContent = fmt(stats.jogando_agora);
    $('s-ses').textContent = fmt(stats.sessoes_24h);
    var mbya = corpora.find(function (c) { return c.name === 'mbya-lexico'; });
    $('s-lex').textContent = fmt(mbya ? mbya.terms : null);
  });

  // ── modal de apoio → ledger independente (/api/v1/campanha/apoiar) ──
  var tierAtual = null;
  function abrirApoio(slug, nome) {
    tierAtual = { slug: slug, nome: nome };
    $('ap-title').textContent = 'Apoiar — tier ' + nome;
    $('ap-ok').hidden = true;
    $('ap-send').disabled = false;
    $('ap').classList.add('open');
  }
  function fechar() { $('ap').classList.remove('open'); }
  $('ap-cancel').addEventListener('click', fechar);
  $('ap').addEventListener('click', function (e) { if (e.target === $('ap')) fechar(); });
  $('ap-send').addEventListener('click', function () {
    if (!tierAtual) return;
    $('ap-send').disabled = true;
    var headers = { 'Content-Type': 'application/json' };
    var t = token();
    if (t) headers.Authorization = 'Bearer ' + t; // logado → sementes na confirmação
    fetch('/api/v1/campanha/apoiar', {
      method: 'POST',
      headers: headers,
      body: JSON.stringify({
        tier: tierAtual.slug,
        nome: ($('ap-nome').value || '').trim() || null,
        email: ($('ap-email').value || '').trim() || null,
        mensagem: ($('ap-msg').value || '').trim() || null,
        mostrar_creditos: !!$('ap-cred').checked,
      }),
    }).then(function (resp) {
      if (resp.ok) {
        $('ap-ok').hidden = false;
        if (window.yggTelemetria) window.yggTelemetria.track('campanha_apoio', { tier: tierAtual.slug });
        setTimeout(fechar, 2200);
      } else { $('ap-send').disabled = false; }
    }).catch(function () { $('ap-send').disabled = false; });
  });

  renderTiers();
})();
