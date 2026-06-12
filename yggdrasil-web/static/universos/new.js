/* /universos/instance/new — criar um universo autorado (instância) e listar os
 * seus. Fecha o loop do CTA "Criar universo": modelo (GET /api/v1/templates) +
 * título → POST /api/v1/instances → abre o player /universos/instance/{id}. */
(function () {
  'use strict';

  var API = '/api/v1';
  var token = null;
  try { token = localStorage.getItem('yggdrasil-jwt'); } catch (_) {}

  var elGate = document.getElementById('gate');
  var elForm = document.getElementById('form');
  var elTemplates = document.getElementById('templates');
  var elMeus = document.getElementById('meus');
  var elTitulo = document.getElementById('titulo');
  var elBtn = document.getElementById('btn-criar');
  var elErro = document.getElementById('erro');
  var selecionado = null;

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  }

  function authHeaders() {
    return token ? { Authorization: 'Bearer ' + token } : {};
  }

  if (!token) {
    elGate.classList.remove('hidden');
    return;
  }
  elForm.classList.remove('hidden');

  // ── modelos ────────────────────────────────────────────────────────────────
  fetch(API + '/templates')
    .then(function (r) { return r.ok ? r.json() : []; })
    .catch(function () { return []; })
    .then(function (templates) {
      if (!templates.length) {
        elTemplates.innerHTML = '<div class="empty">Nenhum modelo disponível.</div>';
        return;
      }
      elTemplates.innerHTML = templates.map(function (t) {
        return '<div class="card" role="radio" aria-checked="false" tabindex="0" data-slug="' + esc(t.slug) + '">' +
          '<h3>' + esc(t.title) + '</h3>' +
          '<div class="desc">' + esc(t.description || '') + '</div>' +
        '</div>';
      }).join('');
      var cards = elTemplates.querySelectorAll('.card');
      function select(card) {
        cards.forEach(function (c) { c.classList.remove('sel'); c.setAttribute('aria-checked', 'false'); });
        card.classList.add('sel');
        card.setAttribute('aria-checked', 'true');
        selecionado = card.getAttribute('data-slug');
        elBtn.disabled = false;
      }
      cards.forEach(function (card) {
        card.addEventListener('click', function () { select(card); });
        card.addEventListener('keydown', function (e) {
          if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); select(card); }
        });
      });
      // pré-seleciona "universo" (tipo único, YG-126), senão o primeiro
      var pref = elTemplates.querySelector('[data-slug="universo"]') || cards[0];
      if (pref) select(pref);
    });

  // ── criar ──────────────────────────────────────────────────────────────────
  elBtn.addEventListener('click', function () {
    if (!selecionado) return;
    elErro.textContent = '';
    elBtn.disabled = true;
    var titulo = elTitulo.value.trim();
    var qs = '?template=' + encodeURIComponent(selecionado) +
      (titulo ? '&title=' + encodeURIComponent(titulo) : '');
    fetch(API + '/instances' + qs, { method: 'POST', headers: authHeaders() })
      .then(function (r) {
        if (r.status === 401) {
          try { localStorage.removeItem('yggdrasil-jwt'); } catch (_) {}
          location.assign('/login?next=/universos/instance/new');
          return null;
        }
        if (!r.ok) { throw new Error('Falha ao criar (' + r.status + '). Tente novamente.'); }
        return r.json();
      })
      .then(function (inst) {
        if (inst && inst.id) location.assign('/universos/instance/' + encodeURIComponent(inst.id));
      })
      .catch(function (err) {
        elErro.textContent = err.message || 'Erro inesperado. Tente novamente.';
        elBtn.disabled = false;
      });
  });

  // ── meus universos ─────────────────────────────────────────────────────────
  fetch(API + '/instances', { headers: authHeaders() })
    .then(function (r) { return r.ok ? r.json() : []; })
    .catch(function () { return []; })
    .then(function (list) {
      if (!list.length) {
        elMeus.innerHTML = '<div class="empty">Você ainda não criou nenhum universo.</div>';
        return;
      }
      elMeus.innerHTML = list.map(function (i) {
        return '<a class="card" href="/universos/instance/' + encodeURIComponent(i.id) + '">' +
          '<span class="badge ' + (i.published ? 'pub' : 'priv') + '">' +
            (i.published ? 'público' : 'privado') + '</span>' +
          '<h3>' + esc(i.title || i.id) + '</h3>' +
          '<div class="desc">' + esc((i.description && i.description.trim()) || 'Universo autorado.') + '</div>' +
        '</a>';
      }).join('');
    });
})();
