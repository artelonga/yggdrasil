/* feedback.js — "Fale conosco". Botão flutuante + modal, em todos os universos
 * e na raiz/lobby. Envia para POST /api/v1/feedback (JWT opcional → anônimo vs
 * usuário). Auto-injeta seus estilos; nenhuma dependência. */
(function () {
  "use strict";
  if (window.__faleConoscoLoaded) return;
  window.__faleConoscoLoaded = true;

  // ── universo a partir da URL ───────────────────────────────────────────────
  function universeSlug() {
    var p = location.pathname.replace(/\/+$/, "");
    if (p === "" || p === "/lobby" || p === "/") return "root";
    if (p === "/neuro" || p === "/anatomia") return "neuro";
    var m = p.match(/^\/universos\/([^/]+)/);
    if (m) return m[1] === "instance" ? "instance" : m[1];
    return p.slice(1).split("/")[0] || "root";
  }

  var KINDS = [
    { v: "feedback", label: "Feedback" },
    { v: "duvida", label: "Dúvida" },
    { v: "sugestao", label: "Sugestão" },
  ];

  // ── estilos ────────────────────────────────────────────────────────────────
  var css =
    ".fc-btn{position:fixed;right:18px;bottom:18px;z-index:99998;border:0;cursor:pointer;" +
      "background:#0d0d12;color:#d4af37;border:1px solid #2a2a44;border-radius:999px;" +
      "padding:.6rem .95rem;font:600 14px system-ui,sans-serif;box-shadow:0 4px 14px rgba(0,0,0,.35);" +
      "display:flex;align-items:center;gap:.45rem;transition:transform .12s,background .12s}" +
    ".fc-btn:hover{transform:translateY(-1px);background:#15151f}" +
    ".fc-ov{position:fixed;inset:0;z-index:99999;background:rgba(6,6,10,.62);" +
      "display:flex;align-items:center;justify-content:center;padding:1rem}" +
    ".fc-card{width:min(460px,100%);max-height:90vh;overflow:auto;background:#0f0f16;" +
      "color:#e8e3d3;border:1px solid #2a2a44;border-radius:14px;padding:1.25rem 1.25rem 1.1rem;" +
      "font-family:system-ui,sans-serif;box-shadow:0 10px 40px rgba(0,0,0,.5)}" +
    ".fc-card h2{margin:0 0 .15rem;font-size:1.15rem;color:#d4af37;font-weight:700}" +
    ".fc-sub{margin:0 0 .9rem;font-size:.82rem;color:#9a9aab}" +
    ".fc-kinds{display:flex;gap:.4rem;margin-bottom:.7rem}" +
    ".fc-kind{flex:1;text-align:center;padding:.45rem;border:1px solid #2a2a44;border-radius:8px;" +
      "background:#14141d;color:#c9c9d6;cursor:pointer;font-size:.85rem;user-select:none}" +
    ".fc-kind.on{border-color:#d4af37;color:#d4af37;background:#1c1c12}" +
    ".fc-card label{display:block;font-size:.74rem;color:#9a9aab;margin:.6rem 0 .25rem}" +
    ".fc-card textarea,.fc-card input{width:100%;box-sizing:border-box;background:#08080c;" +
      "color:#e8e3d3;border:1px solid #2a2a44;border-radius:8px;padding:.55rem .65rem;" +
      "font:inherit;font-size:.9rem}" +
    ".fc-card textarea{min-height:96px;resize:vertical}" +
    ".fc-card input:focus,.fc-card textarea:focus{outline:0;border-color:#d4af37}" +
    ".fc-hint{font-size:.72rem;color:#7e7e8c;margin-top:.25rem}" +
    ".fc-who{font-size:.74rem;color:#7e7e8c;margin:.55rem 0 0}" +
    ".fc-actions{display:flex;gap:.5rem;justify-content:flex-end;margin-top:1rem}" +
    ".fc-actions button{border-radius:8px;padding:.5rem .9rem;font:600 .88rem system-ui;cursor:pointer}" +
    ".fc-cancel{background:transparent;color:#9a9aab;border:1px solid #2a2a44}" +
    ".fc-send{background:#d4af37;color:#1a1a10;border:0}" +
    ".fc-send[disabled]{opacity:.55;cursor:default}" +
    ".fc-msg{font-size:.85rem;margin-top:.7rem}" +
    ".fc-ok{color:#7bd88f}.fc-err{color:#e06c6c}";
  var style = document.createElement("style");
  style.textContent = css;
  document.head.appendChild(style);

  // ── botão flutuante ─────────────────────────────────────────────────────────
  var btn = document.createElement("button");
  btn.className = "fc-btn";
  btn.type = "button";
  btn.setAttribute("aria-haspopup", "dialog");
  btn.innerHTML = "💬 Fale conosco";
  btn.addEventListener("click", open);
  document.body.appendChild(btn);

  var kind = "feedback";

  function loggedIn() {
    try {
      return !!localStorage.getItem("yggdrasil-jwt");
    } catch (_) {
      return false;
    }
  }

  function open() {
    var ov = document.createElement("div");
    ov.className = "fc-ov";
    ov.innerHTML =
      '<div class="fc-card" role="dialog" aria-modal="true" aria-label="Fale conosco">' +
        '<h2>Fale conosco</h2>' +
        '<p class="fc-sub">Feedback, dúvidas e sugestões — sobre <b>' + universeSlug() + "</b>.</p>" +
        '<div class="fc-kinds"></div>' +
        '<label for="fc-message">Mensagem</label>' +
        '<textarea id="fc-message" maxlength="5000" placeholder="Escreva aqui…"></textarea>' +
        '<label for="fc-name">Nome <span style="opacity:.6">(opcional)</span></label>' +
        '<input id="fc-name" type="text" maxlength="120" placeholder="Como podemos te chamar">' +
        '<label for="fc-email">E-mail <span style="opacity:.6">(opcional)</span></label>' +
        '<input id="fc-email" type="email" maxlength="200" placeholder="voce@exemplo.com">' +
        '<div class="fc-hint">Se gostar de receber resposta, deixe seu e-mail.</div>' +
        '<p class="fc-who"></p>' +
        '<div class="fc-msg" aria-live="polite"></div>' +
        '<div class="fc-actions">' +
          '<button class="fc-cancel" type="button">Cancelar</button>' +
          '<button class="fc-send" type="button">Enviar</button>' +
        "</div>" +
      "</div>";
    document.body.appendChild(ov);

    // chips de tipo
    var kinds = ov.querySelector(".fc-kinds");
    kind = "feedback";
    KINDS.forEach(function (k) {
      var c = document.createElement("div");
      c.className = "fc-kind" + (k.v === kind ? " on" : "");
      c.textContent = k.label;
      c.addEventListener("click", function () {
        kind = k.v;
        kinds.querySelectorAll(".fc-kind").forEach(function (e) { e.classList.remove("on"); });
        c.classList.add("on");
      });
      kinds.appendChild(c);
    });

    ov.querySelector(".fc-who").textContent = loggedIn()
      ? "Você está logado — a mensagem virá identificada com sua conta."
      : "Enviando de forma anônima.";

    var msgEl = ov.querySelector(".fc-msg");
    var sendBtn = ov.querySelector(".fc-send");

    function close() {
      ov.remove();
      document.removeEventListener("keydown", onKey);
    }
    function onKey(e) { if (e.key === "Escape") close(); }
    document.addEventListener("keydown", onKey);
    ov.addEventListener("click", function (e) { if (e.target === ov) close(); });
    ov.querySelector(".fc-cancel").addEventListener("click", close);
    ov.querySelector("#fc-message").focus();

    sendBtn.addEventListener("click", function () {
      var message = ov.querySelector("#fc-message").value.trim();
      msgEl.className = "fc-msg";
      if (!message) {
        msgEl.classList.add("fc-err");
        msgEl.textContent = "Escreva uma mensagem antes de enviar.";
        return;
      }
      var payload = {
        universe: universeSlug(),
        kind: kind,
        message: message,
        name: ov.querySelector("#fc-name").value.trim() || null,
        email: ov.querySelector("#fc-email").value.trim() || null,
      };
      var headers = { "Content-Type": "application/json" };
      try {
        var jwt = localStorage.getItem("yggdrasil-jwt");
        if (jwt) headers.Authorization = "Bearer " + jwt;
      } catch (_) {}
      sendBtn.disabled = true;
      sendBtn.textContent = "Enviando…";
      fetch("/api/v1/feedback", {
        method: "POST",
        headers: headers,
        body: JSON.stringify(payload),
      })
        .then(function (r) { return r.ok ? r.json() : r.json().then(function (e) { throw e; }); })
        .then(function () {
          ov.querySelector(".fc-card").innerHTML =
            '<h2>Obrigado! 🙏</h2><p class="fc-sub">Recebemos sua mensagem.' +
            (payload.email ? " Se precisarmos, respondemos no e-mail informado." : "") +
            '</p><div class="fc-actions"><button class="fc-cancel" type="button">Fechar</button></div>';
          ov.querySelector(".fc-cancel").addEventListener("click", close);
        })
        .catch(function (err) {
          sendBtn.disabled = false;
          sendBtn.textContent = "Enviar";
          msgEl.classList.add("fc-err");
          var map = {
            email_invalido: "E-mail inválido — confira ou deixe em branco.",
            mensagem_vazia: "Escreva uma mensagem antes de enviar.",
            mensagem_muito_longa: "Mensagem muito longa.",
            tipo_invalido: "Tipo de mensagem inválido.",
          };
          msgEl.textContent = (err && map[err.error]) || "Não foi possível enviar. Tente de novo.";
        });
    });
  }
})();
