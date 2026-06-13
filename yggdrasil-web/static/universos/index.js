/* /universos — catálogo unificado e filtrável de TODOS os universos do
 * Yggdrasil (YG-70). Fonte primária: GET /api/v1/universos?format=catalog —
 * o merge servidor-side de REGISTRY.yaml (embedados + planejados + externos).
 * Agrega ainda, no cliente, as salas de comunicação publicadas e as instâncias
 * autoradas publicadas. Sem dependências. Filtros: busca, situação (status),
 * tipo, origem, gênero. */
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

  // status → rótulo + emoji (jogável / planejado / externo)
  var STATUS = {
    embedded: { label: "jogável",   icon: "🟢" },
    planned:  { label: "planejado", icon: "🟡" },
    external: { label: "externo",   icon: "🔗" }
  };

  function getJSON(url) {
    return fetch(url, { headers: { Accept: "application/json" } })
      .then(function (r) { return r.ok ? r.json() : null; })
      .catch(function () { return null; });   // fonte ausente → ignora, não quebra o índice
  }

  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"]/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c];
    });
  }

  // destino e rótulo da ação por status
  function targetFor(it) {
    if (it.status === "embedded") return { url: it.url, ext: false };
    if (it.status === "external" && it.external_url) return { url: it.external_url, ext: true };
    // planejado → leva ao template de issue "quero portar"
    return { url: it.contribute_url || "#", ext: !!it.contribute_url };
  }

  // ── monta o catálogo (REGISTRY + fontes vivas) ────────────────────────────
  function buildCatalog(registry, salas, instances) {
    var items = [];

    // 1. catálogo servidor-side (embedados + planejados + externos)
    (registry || []).forEach(function (u) {
      items.push({
        slug: u.slug || u.id,
        nome: u.title || u.name || u.slug,
        desc: u.description || "",
        status: u.status || "embedded",
        type: u.type || "",
        origin: u.origin || "",
        genre: Array.isArray(u.genre) ? u.genre : [],
        external_url: u.external_url || null,
        target_release: u.target_release || null,
        url: "/universos/" + (u.slug || u.id),
        contribute_url: u.status === "planned"
          ? "https://github.com/artelonga/yggdrasil/issues/new?template=portar-rpg.md&title=" +
            encodeURIComponent("Portar " + (u.title || u.slug))
          : null
      });
    });

    // 2. salas de comunicação publicadas (léxico interativo, por língua)
    (salas || []).forEach(function (s) {
      items.push({
        nome: s.title || s.id,
        desc: "Sala de léxico interativo.",
        status: "embedded", type: "lingua", origin: "original",
        genre: [], lang: langName(s.lang),
        url: "/universos/comunicacao?id=" + encodeURIComponent(s.id)
      });
    });

    // 2b. Ayvu Rapyta — corpus de leitura (YG-132): antes só alcançável por
    // URL direta; agora é card de primeira classe do catálogo.
    items.push({
      nome: "Ayvu Rapyta (corpus)",
      desc: "Leitura interlinear Mbyá ⟷ Español — capítulos, glosas, partículas e Caderno.",
      status: "embedded", type: "lingua", origin: "original",
      genre: [], lang: "Mbyá Guarani",
      url: "/universos/corpus"
    });

    // 2c. Laboratório de corpus (YG-139): frequência + comparação cross-linguística.
    items.push({
      slug: "corpus-lab",
      nome: "Laboratório de corpus (NLP)",
      desc: "Palavras mais usadas por corpus e comparação cross-linguística (Mbyá ⟷ Iorubá) via join.",
      status: "embedded", type: "ferramentas", origin: "original",
      genre: [], url: "/universos/corpus-lab"
    });

    // 3. instâncias autoradas publicadas (data-driven)
    (instances || []).forEach(function (i) {
      items.push({
        nome: i.title || i.id,
        desc: (i.description && i.description.trim()) || "Universo autorado.",
        status: "embedded", type: "autorado", origin: "original",
        genre: [],
        url: "/universos/instance/" + encodeURIComponent(i.id)
      });
    });

    return items;
  }

  // YG-137: dois comportamentos por card — (1) clicar/↗ leva à ORIGEM (página
  // do universo embedado, ou external_url, ou template de contribuição p/
  // planejados); (2) "✦ criar perfil aqui" cria o perfil do usuário NESTE
  // universo (vale até para os que ainda não existem). slug identifica o alvo.
  function slugFor(it) {
    if (it.slug) return it.slug;
    // deriva do url quando não veio do registry (salas/instâncias/corpus)
    var m = (it.url || "").match(/\/universos\/(?:instance\/)?([^/?#]+)/);
    return m ? decodeURIComponent(m[1]) : (it.nome || "").toLowerCase().replace(/\s+/g, "-");
  }
  function cardHTML(it) {
    var st = STATUS[it.status] || { label: it.status, icon: "" };
    var t = targetFor(it);
    var typeCls = (it.type || "").replace(/[^a-z]/gi, "").toLowerCase();
    var origem = t.url && t.url !== "#"
      ? '<a class="cta-origem" href="' + esc(t.url) + '"' + (t.ext ? ' target="_blank" rel="noopener"' : "") + '>↗ origem</a>'
      : "";
    return '<div class="card" data-slug="' + esc(slugFor(it)) + '">' +
      '<div class="ct">' +
        '<span class="status ' + esc(it.status) + '">' + st.icon + " " + esc(st.label) + '</span>' +
        (it.type ? '<span class="badge ' + esc(typeCls) + '">' + esc(it.type) + '</span>' : "") +
        (it.lang ? '<span class="lang">' + esc(it.lang) + '</span>' : "") +
        (it.origin ? '<span class="meta">' + esc(it.origin) + '</span>' : "") +
      '</div>' +
      '<h3>' + esc(it.nome) + '</h3>' +
      '<div class="desc">' + esc(it.desc) + '</div>' +
      '<div class="acts">' + origem +
        '<button class="cta-perfil" data-slug="' + esc(slugFor(it)) + '">✦ criar perfil aqui</button>' +
      '</div>' +
    '</div>';
  }

  // CTA "criar perfil aqui" — logado: PUT /profile/universos/{slug}; senão login.
  function criarPerfil(slug, btn) {
    var token = null;
    try { token = localStorage.getItem("yggdrasil-jwt"); } catch (e) { /* sem LS */ }
    if (!token) { location.assign("/login?next=" + encodeURIComponent("/universos")); return; }
    btn.disabled = true;
    fetch("/api/v1/profile/universos/" + encodeURIComponent(slug), {
      method: "PUT",
      headers: { "Content-Type": "application/json", Authorization: "Bearer " + token },
      body: "{}",
    }).then(function (r) {
      if (r.status === 401) { location.assign("/login?next=" + encodeURIComponent("/universos")); return; }
      btn.textContent = r.ok ? "✓ perfil criado" : "⚠ falhou";
      if (!r.ok) btn.disabled = false;
    }).catch(function () { btn.textContent = "⚠ offline"; btn.disabled = false; });
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
    var fStatus = document.getElementById("f-status");
    var fType = document.getElementById("f-type");
    var fOrigin = document.getElementById("f-origin");
    var fGenre = document.getElementById("f-genre");

    // popula dropdowns a partir dos dados presentes
    function fill(sel, values) {
      uniqueSorted(values).forEach(function (v) {
        var o = document.createElement("option");
        o.value = v; o.textContent = v;
        sel.appendChild(o);
      });
    }
    fill(fType, catalog.map(function (i) { return i.type; }));
    fill(fOrigin, catalog.map(function (i) { return i.origin; }));
    var allGenres = [];
    catalog.forEach(function (i) { (i.genre || []).forEach(function (g) { allGenres.push(g); }); });
    fill(fGenre, allGenres);

    function render() {
      var q = (fQ.value || "").trim().toLowerCase();
      var status = fStatus.value, type = fType.value, origin = fOrigin.value, genre = fGenre.value;
      var list = catalog.filter(function (it) {
        if (status && it.status !== status) return false;
        if (type && it.type !== type) return false;
        if (origin && it.origin !== origin) return false;
        if (genre && (it.genre || []).indexOf(genre) === -1) return false;
        if (q) {
          var hay = (it.nome + " " + it.desc + " " + (it.genre || []).join(" ")).toLowerCase();
          if (hay.indexOf(q) === -1) return false;
        }
        return true;
      });
      var by = { embedded: 0, planned: 0, external: 0 };
      list.forEach(function (it) { if (by[it.status] != null) by[it.status]++; });
      elCount.textContent = list.length + (list.length === 1 ? " universo" : " universos") +
        " · 🟢 " + by.embedded + " · 🟡 " + by.planned + " · 🔗 " + by.external;
      elGrid.innerHTML = list.length
        ? list.map(cardHTML).join("")
        : '<div class="empty">Nenhum universo encontrado com esses filtros.</div>';
      elGrid.querySelectorAll(".cta-perfil").forEach(function (b) {
        b.addEventListener("click", function () { criarPerfil(b.dataset.slug, b); });
      });
    }

    [fQ, fStatus, fType, fOrigin, fGenre].forEach(function (el) {
      el.addEventListener("input", render);
      el.addEventListener("change", render);
    });
    render();
  }

  Promise.all([
    getJSON("/api/v1/universos?format=catalog"),
    getJSON("/api/v1/comunicacao/salas?published=true"),
    getJSON("/api/v1/instances?published=true")
  ]).then(function (res) {
    var registry = (res[0] && res[0].universos) || [];
    init(buildCatalog(registry, res[1] || [], res[2] || []));
  });
})();
