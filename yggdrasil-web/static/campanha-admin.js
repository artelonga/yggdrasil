/* /campanha/admin — admin de apoios (YG-165). Lê GET /api/v1/campanha/pledges
 * com o admin token (Bearer) e confirma pagamentos. O token fica só em memória
 * (sessionStorage), nunca no markup. Expõe window.AdminApoios p/ testes. */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var esc = function (s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };
  var TKEY = 'ygg-admin-token';
  var TIER = { semente: 'Semente', raiz: 'Raiz', galho: 'Galho', folhagem: 'Folhagem', tronco: 'Tronco', yggdrasil: 'Yggdrasil' };

  function tok() { try { return sessionStorage.getItem(TKEY) || ''; } catch (e) { return ''; } }
  function setTok(t) { try { sessionStorage.setItem(TKEY, t); } catch (e) {} }
  function msg(t, cls) { var m = $('msg'); m.textContent = t || ''; m.className = 'msg ' + (cls || ''); }

  function fmtData(ms) {
    try { return new Date(ms).toLocaleString('pt-BR'); } catch (e) { return String(ms); }
  }

  function row(p) {
    var quem = esc(p.nome || '—') + (p.email ? '<div class="em">' + esc(p.email) + '</div>' : '');
    var acao = p.status === 'pendente'
      ? '<button class="conf" data-id="' + esc(p.id) + '">✓ Confirmar</button>'
      : '';
    // YG-172: estado do comprovante (aguardando / enviado + nota + arquivo)
    var comp;
    if (p.comprovante_em) {
      comp = '<span class="comp-yes">📎 enviado</span>' +
        (p.comprovante_nota ? '<div class="comp-nota">' + esc(p.comprovante_nota) + '</div>' : '') +
        (p.comprovante_arquivo ? '<button class="dl" data-id="' + esc(p.id) + '">ver arquivo</button>' : '');
    } else {
      comp = '<span class="comp-no">aguardando</span>';
    }
    return '<tr data-id="' + esc(p.id) + '">' +
      '<td>' + esc(fmtData(p.created_at)) + '</td>' +
      '<td>' + esc(TIER[p.tier] || p.tier) + '</td>' +
      '<td class="val">R$ ' + esc(p.valor) + '</td>' +
      '<td>' + quem + '</td>' +
      '<td>' + (p.mensagem ? esc(p.mensagem) : '') + '</td>' +
      '<td>' + comp + '</td>' +
      '<td><span class="st ' + esc(p.status) + '">' + esc(p.status) + '</span></td>' +
      '<td>' + acao + '</td>' +
    '</tr>';
  }

  function render(pledges) {
    $('rows').innerHTML = pledges.map(row).join('');
    $('tbl').hidden = pledges.length === 0;
    var conf = pledges.filter(function (p) { return p.status === 'confirmado'; }).length;
    var comps = pledges.filter(function (p) { return p.comprovante_em; }).length;
    $('sum').textContent = pledges.length + ' apoios · ' + conf + ' confirmados · ' +
      (pledges.length - conf) + ' pendentes · ' + comps + ' com comprovante';
    $('rows').querySelectorAll('button.conf').forEach(function (b) {
      b.addEventListener('click', function () { confirmar(b.dataset.id, b); });
    });
    // download do arquivo: fetch autenticado → blob → abre em nova aba
    $('rows').querySelectorAll('button.dl').forEach(function (b) {
      b.addEventListener('click', function () { baixarArquivo(b.dataset.id); });
    });
    window.AdminApoios = { count: pledges.length, confirmados: conf, comComprovante: comps };
  }

  function baixarArquivo(id) {
    fetch('/api/v1/campanha/pledges/' + encodeURIComponent(id) + '/comprovante/arquivo', {
      headers: { Authorization: 'Bearer ' + tok() },
    }).then(function (r) {
      if (!r.ok) { msg('Falha ao baixar comprovante (' + r.status + ').', 'err'); return null; }
      return r.blob();
    }).then(function (blob) {
      if (!blob) return;
      var url = URL.createObjectURL(blob);
      window.open(url, '_blank');
      setTimeout(function () { URL.revokeObjectURL(url); }, 60000);
    }).catch(function () { msg('Falha de rede ao baixar.', 'err'); });
  }

  function carregar() {
    var t = tok();
    if (!t) { msg('Informe o admin token.', 'err'); return Promise.resolve(); }
    msg('Carregando…');
    return fetch('/api/v1/campanha/pledges', { headers: { Authorization: 'Bearer ' + t } })
      .then(function (r) {
        if (r.status === 401) { msg('Token inválido ou não configurado (401).', 'err'); $('tbl').hidden = true; return null; }
        if (!r.ok) { msg('Erro ' + r.status, 'err'); return null; }
        return r.json();
      })
      .then(function (pledges) {
        if (!pledges) return;
        msg('');
        render(pledges);
      })
      .catch(function () { msg('Falha de rede.', 'err'); });
  }

  function confirmar(id, btn) {
    if (btn) { btn.disabled = true; btn.textContent = '…'; }
    fetch('/api/v1/campanha/pledges/' + encodeURIComponent(id) + '/confirmar', {
      method: 'POST', headers: { Authorization: 'Bearer ' + tok() },
    }).then(function (r) {
      if (r.ok) { msg('Pagamento confirmado.', 'ok'); return carregar(); }
      msg('Falha ao confirmar (' + r.status + ').', 'err');
      if (btn) { btn.disabled = false; btn.textContent = '✓ Confirmar'; }
    }).catch(function () {
      msg('Falha de rede.', 'err');
      if (btn) { btn.disabled = false; btn.textContent = '✓ Confirmar'; }
    });
  }

  $('load').addEventListener('click', function () { setTok($('tok').value.trim()); carregar(); });
  $('tok').addEventListener('keydown', function (e) { if (e.key === 'Enter') { setTok($('tok').value.trim()); carregar(); } });

  // se já há token na sessão, pré-preenche e carrega
  if (tok()) { $('tok').value = tok(); carregar(); }
  window.AdminApoios = { count: null, confirmados: null, reload: carregar };
})();
