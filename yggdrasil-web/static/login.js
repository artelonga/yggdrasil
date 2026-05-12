'use strict';

const STORAGE_KEY = 'yggdrasil-jwt';
const EMAIL_KEY = 'yggdrasil-email';

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

formEmail.addEventListener('submit', async (e) => {
  e.preventDefault();
  erroEmail.textContent = '';
  btnEmail.disabled = true;
  emailAtual = inputEmail.value.trim().toLowerCase();

  try {
    const res = await fetch('/api/v1/auth/code', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: emailAtual }),
    });
    if (!res.ok) throw new Error('falha');
    localStorage.setItem(EMAIL_KEY, emailAtual);
    emailConfirmado.textContent = emailAtual;
    formEmail.classList.add('hidden');
    formCodigo.classList.remove('hidden');
    inputCodigo.focus();
  } catch (err) {
    erroEmail.textContent = 'Não foi possível enviar o código. Tente novamente.';
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
    const res = await fetch('/api/v1/auth/verify', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: emailAtual, code: codigo }),
    });
    if (res.status === 422) {
      erroCodigo.textContent = 'Código incorreto. Tente de novo.';
      btnCodigo.disabled = false;
      return;
    }
    if (res.status === 410) {
      erroCodigo.textContent = 'Código expirado. Solicite um novo.';
      btnCodigo.disabled = false;
      return;
    }
    if (!res.ok) throw new Error('falha');
    const { token } = await res.json();
    localStorage.setItem(STORAGE_KEY, token);
    location.assign(redirectTarget());
  } catch (err) {
    erroCodigo.textContent = 'Erro inesperado. Tente novamente.';
    btnCodigo.disabled = false;
  }
});
