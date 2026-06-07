/* /universos — índice unificado e filtrável de TODOS os universos do Yggdrasil.
 * Agrega, no cliente, quatro fontes: a lista de arcade (/api/v1/universos), o
 * atlas estático (anatomia), o feed público de salas de comunicação
 * (/api/v1/comunicacao/salas?published=true) e as instâncias autoradas
 * (/api/v1/instances?published=true). Sem dependências. Filtros: busca livre,
 * categoria, jogadores (solo/multi) e idioma. */
(function () {
  "use strict";

  // mapeia código de língua → nome legível (salas de comunicação)
  var LANGS = {
    "gn-mbya": "Mbyá Guarani", "mbya": "Mbyá Guarani",
    "yo": "Yorùbá", "yoruba": "Yorùbá",
    "pt": "Português", "pt-br": "Português"
  };
  function langName(code) {
    if (!code) return null;
    var k = String(code).toLowerCase();
    return LANGS[k] || code;
  }

  var CAT = {
    arcade:   { label: "arcade",   cls: "arcade" },
    atlas:    { label: "atlas",    cls: "atlas" },
    lingua:   { label: "língua",   cls: "lingua" },
    autorado: { label: "autorado", cls: "autorado" }
  };

  function getJSON(url) {
    return fetch(url, { headers: { Accept: "application/json" } })
      .then(function (r) { return r.ok ? r.json() : []; })
      .catch(function () { return []; });   // fonte ausente → ignora, não quebra o índice
  }

  // ── monta o catálogo a partir das fontes ──────────────────────────────────
  function buildCatalog(arcade, salas, instances) {
    var items = [];

    // 1. arcade + neuro (lista oficial)
    (arcade || []).forEach(function (u) {
      var atlas = u.id === "neuro";
      items.push({
        nome: u.name || u.id,
        desc: atlas ? "Atlas 3D — explore em colaboração." : "Jogo arcade.",
        cat: atlas ? "atlas" : "arcade",
        players: (u.max_players || 1) > 1 ? "multi" : "solo",
        lang: null,
        url: "/universos/" + u.id
      });
    });

    // 2. atlas estático — anatomia (peça interativa do cérebro)
    items.push({
      nome: "neuroanatomy",
      desc: "Atlas do cérebro — explore as partes brincando.",
      cat: "atlas", players: "multi", lang: null,
      url: "/static/anatomia/"
    });

    // 3. salas de comunicação publicadas (léxico interativo, por língua)
    (salas || []).forEach(function (s) {
      items.push({
        nome: s.title || s.id,
        desc: "Sala de léxico interativo.",
        cat: "lingua", players: "multi",
        lang: langName(s.lang),
        url: "/universos/comunicacao?id=" + encodeURIComponent(s.id)
      });
    });

    // 4. instâncias autoradas publicadas (data-driven)
    (instances || []).forEach(function (i) {
      items.push({
        nome: i.title || i.id,
        desc: (i.description && i.description.trim()) || "Universo autorado.",
        cat: "autorado", players: "solo", lang: null,
        url: "/universos/instance/" + encodeURIComponent(i.id)
      });
    });

    return items;
  }

  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"]/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c];
    });
  }

  function cardHTML(it) {
    var c = CAT[it.cat] || { label: it.cat, cls: "" };
    return '<a class="card" href="' + esc(it.url) + '">' +
      '<div class="ct">' +
        '<span class="badge ' + c.cls + '">' + esc(c.label) + '</span>' +
        (it.lang ? '<span class="lang">' + esc(it.lang) + '</span>' : "") +
        '<span class="meta">' + (it.players === "multi" ? "multiplayer" : "solo") + '</span>' +
      '</div>' +
      '<h3>' + esc(it.nome) + '</h3>' +
      '<div class="desc">' + esc(it.desc) + '</div>' +
    '</a>';
  }

  function uniqueSorted(arr) {
    var seen = {}, out = [];
    arr.forEach(function (v) { if (v && !seen[v]) { seen[v] = 1; out.push(v); } });
    return out.sort();
  }

  function init(catalog) {
    var elGrid = document.getElementById("grid");
    var elCount = document.getElementById("count");
    var fQ = document.getElementById("f-q");
    var fCat = document.getElementById("f-cat");
    var fPlayers = document.getElementById("f-players");
    var fLang = document.getElementById("f-lang");

    // popula dropdowns a partir dos dados presentes
    uniqueSorted(catalog.map(function (i) { return i.cat; })).forEach(function (cat) {
      var o = document.createElement("option");
      o.value = cat; o.textContent = (CAT[cat] ? CAT[cat].label : cat);
      fCat.appendChild(o);
    });
    uniqueSorted(catalog.map(function (i) { return i.lang; })).forEach(function (lang) {
      var o = document.createElement("option");
      o.value = lang; o.textContent = lang;
      fLang.appendChild(o);
    });

    function render() {
      var q = (fQ.value || "").trim().toLowerCase();
      var cat = fCat.value, players = fPlayers.value, lang = fLang.value;
      var list = catalog.filter(function (it) {
        if (cat && it.cat !== cat) return false;
        if (players && it.players !== players) return false;
        if (lang && it.lang !== lang) return false;
        if (q) {
          var hay = (it.nome + " " + it.desc + " " + (it.lang || "")).toLowerCase();
          if (hay.indexOf(q) === -1) return false;
        }
        return true;
      });
      elCount.textContent = list.length + (list.length === 1 ? " universo" : " universos");
      elGrid.innerHTML = list.length
        ? list.map(cardHTML).join("")
        : '<div class="empty">Nenhum universo encontrado com esses filtros.</div>';
    }

    [fQ, fCat, fPlayers, fLang].forEach(function (el) {
      el.addEventListener("input", render);
      el.addEventListener("change", render);
    });
    render();
  }

  Promise.all([
    getJSON("/api/v1/universos"),
    getJSON("/api/v1/comunicacao/salas?published=true"),
    getJSON("/api/v1/instances?published=true")
  ]).then(function (res) {
    init(buildCatalog(res[0], res[1], res[2]));
  });
})();
