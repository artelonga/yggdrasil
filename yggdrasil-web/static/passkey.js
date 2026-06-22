/* passkey.js — cliente WebAuthn (YG-174). Login biométrico/security-key real:
 * converte as opções do webauthn-rs (campos base64url) ↔ ArrayBuffer do browser,
 * chama navigator.credentials, e serializa de volta p/ o finish. Expõe
 * window.Passkey = { supported, register, login }. */
(function () {
  'use strict';
  var JWT_KEY = 'yggdrasil-jwt';

  // ── base64url ↔ ArrayBuffer ──────────────────────────────────────────────
  function b64urlToBuf(s) {
    s = s.replace(/-/g, '+').replace(/_/g, '/');
    var pad = s.length % 4; if (pad) s += '===='.slice(pad);
    var bin = atob(s), buf = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
    return buf.buffer;
  }
  function bufToB64url(buf) {
    var bytes = new Uint8Array(buf), bin = '';
    for (var i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
    return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }

  function supported() {
    return !!(window.PublicKeyCredential && navigator.credentials && navigator.credentials.create);
  }

  // converte os campos base64url das opções (challenge/user.id/cred ids) p/ buffer
  function decodeCreation(pk) {
    pk.challenge = b64urlToBuf(pk.challenge);
    if (pk.user && pk.user.id) pk.user.id = b64urlToBuf(pk.user.id);
    (pk.excludeCredentials || []).forEach(function (c) { c.id = b64urlToBuf(c.id); });
    return pk;
  }
  function decodeRequest(pk) {
    pk.challenge = b64urlToBuf(pk.challenge);
    (pk.allowCredentials || []).forEach(function (c) { c.id = b64urlToBuf(c.id); });
    return pk;
  }
  // serializa a credencial criada/obtida no formato que o webauthn-rs espera
  function encodeAttestation(cred) {
    return {
      id: cred.id, rawId: bufToB64url(cred.rawId), type: cred.type,
      extensions: cred.getClientExtensionResults ? cred.getClientExtensionResults() : {},
      response: {
        attestationObject: bufToB64url(cred.response.attestationObject),
        clientDataJSON: bufToB64url(cred.response.clientDataJSON),
      },
    };
  }
  function encodeAssertion(cred) {
    var r = cred.response;
    return {
      id: cred.id, rawId: bufToB64url(cred.rawId), type: cred.type,
      extensions: cred.getClientExtensionResults ? cred.getClientExtensionResults() : {},
      response: {
        authenticatorData: bufToB64url(r.authenticatorData),
        clientDataJSON: bufToB64url(r.clientDataJSON),
        signature: bufToB64url(r.signature),
        userHandle: r.userHandle ? bufToB64url(r.userHandle) : null,
      },
    };
  }

  function tok() { try { return localStorage.getItem(JWT_KEY); } catch (e) { return null; } }

  // ── registrar passkey (requer estar logado) ──────────────────────────────
  function register(label) {
    var t = tok();
    if (!t) return Promise.reject(new Error('precisa estar logado'));
    return fetch('/api/v1/auth/passkey/register/start', {
      method: 'POST', headers: { Authorization: 'Bearer ' + t },
    }).then(function (r) { if (!r.ok) throw new Error('start ' + r.status); return r.json(); })
      .then(function (data) {
        return navigator.credentials.create({ publicKey: decodeCreation(data.options.publicKey) })
          .then(function (cred) {
            return fetch('/api/v1/auth/passkey/register/finish', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + t },
              body: JSON.stringify({ id: data.id, credential: encodeAttestation(cred), label: label || null }),
            });
          });
      }).then(function (r) { if (!r.ok) throw new Error('finish ' + r.status); return true; });
  }

  // ── login por passkey (anônimo, dica de e-mail) ──────────────────────────
  function login(email) {
    return fetch('/api/v1/auth/passkey/login/start', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: email }),
    }).then(function (r) {
      if (r.status === 404) throw new Error('sem_passkey');
      if (!r.ok) throw new Error('start ' + r.status);
      return r.json();
    }).then(function (data) {
      return navigator.credentials.get({ publicKey: decodeRequest(data.options.publicKey) })
        .then(function (cred) {
          return fetch('/api/v1/auth/passkey/login/finish', {
            method: 'POST', headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id: data.id, credential: encodeAssertion(cred) }),
          });
        });
    }).then(function (r) {
      if (!r.ok) throw new Error('login ' + r.status);
      return r.json();
    }).then(function (data) {
      if (!data.token) throw new Error('sem token');
      localStorage.setItem(JWT_KEY, data.token);
      return data.token;
    });
  }

  window.Passkey = { supported: supported, register: register, login: login };
})();
