'use strict';

const STORAGE_KEY = 'yggdrasil-jwt';
const EMAIL_KEY = 'yggdrasil-email';

// O fluxo de email-código é roteado pelo CO (mesma origem de quilombo e
// outros universos artelonga). O backend Yggdrasil só vê o JWT final via
// handover ES256. CO tem SMTP configurado; Yggdrasil não precisa.
const CO_BASE = 'https://co.artelonga.com.br';
const YGG_RECEIVER = window.location.origin + '/auth/co-handover-receive';

const formEmail = document.getElementById('form-email');
const formCodigo = document.getElementById('form-codigo');
const inputEmail = document.getElementById('email');
const inputCodigo = document.getElementById('codigo');
const btnEmail = document.getElementById('btn-email');
const btnCodigo = document.getElementById('btn-codigo');
const erroEmail = document.getElementById('erro-email');
const erroCodigo = document.getElementById('erro-codigo');
const emailConfirmado = document.getElementById('email-confirmado');

let emailAtual = '';

if (localStorage.getItem(STORAGE_KEY)) {
  // Already logged in — bounce to lobby unless ?force=1
  if (!new URLSearchParams(location.search).get('force')) {
    location.assign(redirectTarget());
  }
}

function redirectTarget() {
  const params = new URLSearchParams(location.search);
  const next = params.get('next');
  if (next && next.startsWith('/')) return next;
  return '/lobby';
}

// Propagate ?next= to the "Continuar com Google" button so post-login redirect
// matches the email-code flow.
{
  const params = new URLSearchParams(location.search);
  const next = params.get('next');
  if (next && next.startsWith('/')) {
    const coBtn = document.getElementById('btn-co-login');
    if (coBtn) {
      const u = new URL(coBtn.href, location.origin);
      u.searchParams.set('next', next);
      coBtn.href = u.toString();
    }
  }
}

/**
 * URL para o handover final do CO. CO assina ES256 e redireciona para
 * `/auth/co-handover-receive?co_token=...&next=...`. Aceita `next` opcional
 * (rota local pós-login).
 */
function coHandoverUrl() {
  const params = new URLSearchParams(location.search);
  const next = params.get('next');
  const receiver = next && next.startsWith('/')
    ? `${YGG_RECEIVER}?next=${encodeURIComponent(next)}`
    : YGG_RECEIVER;
  return `${CO_BASE}/auth/co-handover?return_to=${encodeURIComponent(receiver)}`;
}

formEmail.addEventListener('submit', async (e) => {
  e.preventDefault();
  erroEmail.textContent = '';
  btnEmail.disabled = true;
  emailAtual = inputEmail.value.trim().toLowerCase();

  try {
    const res = await fetch(`${CO_BASE}/api/v1/auth/onboard-with-email`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: emailAtual,
        return_to: YGG_RECEIVER,
        intent: 'login_or_signup',
      }),
    });
    if (!res.ok && res.status !== 202) {
      let msg = 'Não foi possível enviar o código.';
      try { const j = await res.json(); if (j && j.message) msg = j.message; } catch (_) {}
      throw new Error(msg);
    }
    localStorage.setItem(EMAIL_KEY, emailAtual);
    emailConfirmado.textContent = emailAtual;
    formEmail.classList.add('hidden');
    formCodigo.classList.remove('hidden');
    inputCodigo.focus();
  } catch (err) {
    erroEmail.textContent = err.message || 'Não foi possível enviar o código. Tente novamente.';
  } finally {
    btnEmail.disabled = false;
  }
});

formCodigo.addEventListener('submit', async (e) => {
  e.preventDefault();
  erroCodigo.textContent = '';
  btnCodigo.disabled = true;
  const codigo = inputCodigo.value.trim();

  try {
    const res = await fetch(`${CO_BASE}/api/v1/auth/onboard-with-email/verify`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: emailAtual, code: codigo }),
    });
    if (res.status === 422 || res.status === 400) {
      let msg = 'Código incorreto. Tente de novo.';
      try { const j = await res.json(); if (j && j.message) msg = j.message; } catch (_) {}
      erroCodigo.textContent = msg;
      btnCodigo.disabled = false;
      return;
    }
    if (res.status === 410) {
      erroCodigo.textContent = 'Código expirado. Solicite um novo.';
      btnCodigo.disabled = false;
      return;
    }
    if (!res.ok) {
      let msg = 'Erro inesperado. Tente novamente.';
      try { const j = await res.json(); if (j && j.message) msg = j.message; } catch (_) {}
      throw new Error(msg);
    }
    // CO setou o cookie de sessão. Agora navegamos para o /auth/co-handover do CO
    // que vai assinar um co_token e redirecionar de volta para nós.
    location.assign(coHandoverUrl());
  } catch (err) {
    erroCodigo.textContent = err.message || 'Erro inesperado. Tente novamente.';
    btnCodigo.disabled = false;
  }
});
