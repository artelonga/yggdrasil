/* nee/audio-engine.js — camada de SOM do ÑE'Ẽ (YG-155).
 *
 * Sintetiza áudio EM MEMÓRIA via Web Audio (sem arquivos de sample):
 *   - música  → OscillatorNode + GainNode (ADSR) por voz; acorde = vozes juntas;
 *               motivo = passos em sequência no tempo.
 *   - idioma  → pronúncia via SpeechSynthesis (Web Speech); fallback silencioso.
 *
 * Engine-neutro: NÃO sabe desenhar nada. Recebe o `audio` de uma entrada de
 * `LexiconPack` (o MESMO contrato que um cliente 3D futuro consumiria) e o toca.
 *
 * Política de autoplay: o AudioContext só é criado/resumido no 1º gesto do
 * usuário (`unlock()` / qualquer `play*`). Memory-light: um único contexto, teto
 * de vozes simultâneas (MAX_VOICES) — osciladores são one-shot (start/stop),
 * coletados pelo browser; nada residente além do contexto. */

export const MAX_VOICES = 16;

const WAVES = { sine: 'sine', square: 'square', sawtooth: 'sawtooth', triangle: 'triangle' };

export class AudioEngine {
  constructor() {
    this.ctx = null;
    // telemetria leve p/ testes/observabilidade (sem depender de som real).
    this.stats = { voicesStarted: 0, lastFreqs: [], spoken: 0, dropped: 0 };
    this._active = 0; // vozes vivas agora (teto MAX_VOICES)
  }

  /** Cria/resume o AudioContext — chamar no 1º gesto (autoplay policy). */
  unlock() {
    if (!this.ctx) {
      const Ctx = window.AudioContext || window.webkitAudioContext;
      if (!Ctx) return null; // plataforma sem Web Audio → fallback silencioso
      this.ctx = new Ctx();
    }
    if (this.ctx.state === 'suspended' && this.ctx.resume) this.ctx.resume();
    return this.ctx;
  }

  get now() {
    return this.ctx ? this.ctx.currentTime : 0;
  }

  /** Agenda UMA voz (uma freq) com envelope ADSR a partir de `startAt`. */
  _voice(freq, waveform, env, durationMs, startAt) {
    if (!this.ctx || !(freq > 0)) return;
    if (this._active >= MAX_VOICES) {
      this.stats.dropped++;
      return; // teto de vozes — descarta em vez de saturar a memória/CPU
    }
    const osc = this.ctx.createOscillator();
    const gain = this.ctx.createGain();
    osc.type = WAVES[waveform] || 'sine';
    osc.frequency.value = freq;

    const a = (env.attack_ms || 0) / 1000;
    const d = (env.decay_ms || 0) / 1000;
    const s = env.sustain == null ? 0.7 : env.sustain;
    const r = (env.release_ms || 0) / 1000;
    const dur = Math.max(0.02, durationMs / 1000);
    const t0 = startAt;
    const peak = 0.25; // headroom — várias vozes não estouram

    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.linearRampToValueAtTime(peak, t0 + a);
    gain.gain.linearRampToValueAtTime(peak * s, t0 + a + d);
    const stop = t0 + a + d + dur;
    gain.gain.setValueAtTime(peak * s, stop);
    gain.gain.linearRampToValueAtTime(0.0001, stop + r);

    osc.connect(gain).connect(this.ctx.destination);
    osc.start(t0);
    osc.stop(stop + r);
    this._active++;
    osc.onended = () => {
      this._active--;
      try { gain.disconnect(); } catch (_) {}
    };

    this.stats.voicesStarted++;
    this.stats.lastFreqs.push(Math.round(freq * 100) / 100);
    if (this.stats.lastFreqs.length > 64) this.stats.lastFreqs.shift();
  }

  /** Toca um `EntryAudio` (mode = synth | sequence | speech). */
  play(audio) {
    if (!audio) return false;
    if (audio.mode === 'speech') return this.speak(audio);
    if (!this.unlock()) return false;
    const env = audio.envelope || {};
    if (audio.mode === 'synth') {
      const start = this.now + 0.01;
      for (const f of audio.freqs || []) this._voice(f, audio.waveform, env, audio.duration_ms, start);
      return true;
    }
    if (audio.mode === 'sequence') {
      let at = this.now + 0.01;
      for (const step of audio.steps || []) {
        for (const f of step.freqs || []) this._voice(f, audio.waveform, env, step.duration_ms, at);
        at += (step.duration_ms || 0) / 1000;
      }
      return true;
    }
    return false;
  }

  /** Pronúncia via Web Speech. Fallback silencioso se indisponível. */
  speak(audio) {
    const synth = window.speechSynthesis;
    if (!synth || typeof window.SpeechSynthesisUtterance !== 'function') return false;
    const u = new window.SpeechSynthesisUtterance(audio.text || '');
    if (audio.lang) u.lang = audio.lang;
    u.rate = 0.9;
    synth.speak(u);
    this.stats.spoken++;
    return true;
  }
}
