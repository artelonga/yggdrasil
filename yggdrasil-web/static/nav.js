/* nav.js — barra de navegação global. Auto-injetável, sem dependências, no mesmo
 * espírito de feedback.js: incluída via <script> em cada página para dar a TODAS
 * um caminho consistente de volta ao início, ao mapa (lobby), ao catálogo de
 * universos e à comunidade. Cada clique vira uma decisão clara dentro de uma
 * malha previsível de destinos (YG-92). */
(function () {
  "use strict";
  if (window.__ygNavLoaded) return;
  window.__ygNavLoaded = true;

  // Páginas que já têm cabeçalho próprio (ex.: a landing com header.top) não
  // recebem a barra — evita duplicar navegação.
  if (document.querySelector("header.top") || document.body.hasAttribute("data-yg-nav-skip")) {
    return;
  }

  // ── destinos ─────────────────────────────────────────────────────────────
  var LINKS = [
    { href: "/", icon: "⌂", label: "Início" },
    { href: "/lobby", icon: "🗺", label: "Lobby" },
    { href: "/universos", icon: "✦", label: "Universos" },
    { href: "/feedback", icon: "💬", label: "Comunidade" },
    { href: "/analytics", icon: "📈", label: "Ao vivo" },
    { href: "/campanha", icon: "🌱", label: "Apoie" },
  ];

  // Marca como ativo o destino que melhor casa com a URL atual (match por
  // prefixo, escolhendo o caminho mais específico — "/universos" antes de "/").
  function activeHref() {
    var p = location.pathname.replace(/\/+$/, "") || "/";
    var best = null;
    LINKS.forEach(function (l) {
      var base = l.href.replace(/\/+$/, "") || "/";
      var hit = base === "/" ? p === "/" : p === base || p.indexOf(base + "/") === 0;
      if (hit && (!best || base.length > best.length)) best = base;
    });
    return best;
  }

  function loggedIn() {
    try {
      return !!localStorage.getItem("yggdrasil-jwt");
    } catch (_) {
      return false;
    }
  }

  // ── estilos ──────────────────────────────────────────────────────────────
  // Igdrasil Core (YG-173): obsidiana + violeta elétrico, glass. Hex hardcoded —
  // nav.js roda em páginas que podem não carregar igdrasil.css.
  var css =
    ".yg-nav{position:fixed;top:0;left:0;right:0;z-index:99997;display:flex;align-items:center;" +
      "gap:.15rem;padding:.34rem .6rem;background:rgba(9,9,11,.82);backdrop-filter:blur(20px);" +
      "-webkit-backdrop-filter:blur(20px);border-bottom:1px solid rgba(255,255,255,.06);" +
      "font:600 13px 'Geist',system-ui,sans-serif;box-shadow:0 2px 14px rgba(0,0,0,.4)}" +
    ".yg-nav .yg-brand{color:#ecb2ff;font-weight:700;letter-spacing:.02em;margin-right:.55rem;" +
      "text-decoration:none;font-size:13px;white-space:nowrap;text-shadow:0 0 12px rgba(189,0,255,.45)}" +
    ".yg-nav a.yg-link{color:#a490a7;text-decoration:none;padding:.3rem .55rem;border-radius:8px;" +
      "display:flex;align-items:center;gap:.32rem;white-space:nowrap;transition:background .12s,color .12s}" +
    ".yg-nav a.yg-link:hover{background:rgba(255,255,255,.05);color:#e5e1e4}" +
    ".yg-nav a.yg-link.on{color:#ecb2ff;background:rgba(189,0,255,.12)}" +
    ".yg-nav .yg-spacer{flex:1}" +
    ".yg-nav a.yg-acct{color:#ecb2ff;text-decoration:none;border:1px solid #3f3244;border-radius:999px;" +
      "padding:.28rem .8rem;white-space:nowrap;font-family:'JetBrains Mono',ui-monospace,monospace;font-size:12px}" +
    ".yg-nav a.yg-acct:hover{background:rgba(189,0,255,.12);box-shadow:0 0 12px rgba(189,0,255,.2)}" +
    ".yg-nav .yg-ico{font-size:14px;line-height:1}" +
    ".yg-nav .yg-txt{display:inline}" +
    "@media(max-width:560px){.yg-nav .yg-txt{display:none}.yg-nav{gap:.1rem}" +
      ".yg-nav a.yg-link{padding:.3rem .42rem}}";
  var style = document.createElement("style");
  style.textContent = css;
  document.head.appendChild(style);

  // ── barra ────────────────────────────────────────────────────────────────
  var active = activeHref();
  var bar = document.createElement("nav");
  bar.className = "yg-nav";
  bar.setAttribute("aria-label", "Navegação principal");

  var html = '<a class="yg-brand" href="/">Yggdrasil</a>';
  LINKS.forEach(function (l) {
    var base = l.href.replace(/\/+$/, "") || "/";
    var on = base === active ? " on" : "";
    html +=
      '<a class="yg-link' + on + '" href="' + l.href + '"' +
      (on ? ' aria-current="page"' : "") + ">" +
      '<span class="yg-ico">' + l.icon + "</span>" +
      '<span class="yg-txt">' + l.label + "</span></a>";
  });
  html += '<span class="yg-spacer"></span>';
  // Sem página de perfil dedicada ainda (fase futura da visão "mapa RPG"); o
  // slot de conta logada aponta para a página de autenticação (`?force=1` evita
  // o bounce automático para o lobby), onde dá para gerenciar/encerrar sessão.
  html += loggedIn()
    ? '<a class="yg-acct" href="/login?force=1">👤 Conta</a>'
    : '<a class="yg-acct" href="/login">Entrar</a>';
  bar.innerHTML = html;

  // Insere a barra como primeiro filho do body para não depender de CSS prévio.
  if (document.body.firstChild) {
    document.body.insertBefore(bar, document.body.firstChild);
  } else {
    document.body.appendChild(bar);
  }

  // YG-133: a barra é fixa — sem isto ela COBRE o topo das páginas. Layouts em
  // fluxo ganham padding; toolbars fixas leem var(--yg-nav-h) no próprio CSS.
  var h = bar.offsetHeight || 36;
  document.documentElement.style.setProperty("--yg-nav-h", h + "px");
  var pad = parseFloat(getComputedStyle(document.body).paddingTop) || 0;
  document.body.style.paddingTop = (pad + h) + "px";
})();
