/* nee/sequencer.js — Sequencer espacial YG-170.
 *
 * "Compor andando": cada passo gravado = um SeqStep (freqs + duração = 1 beat).
 * Memory-light: apenas params de síntese (freqs, duration_ms) — sem bytes de áudio.
 * Teto de MAX_STEPS passos por motivo; MAX_VOICES por passo herdado do engine.
 *
 * Reusa AudioEngine do YG-155 para tocar — nenhum código de áudio próprio. O
 * mesmo contrato `EntryAudio::Sequence` é produzido pelo método `toSequence()`,
 * compatível com `AudioEngine.play()` e com a API de motivos (YG-170). */

import { MAX_VOICES } from './audio-engine.js';

/** Teto de passos por motivo (memory-light: só params, mas limita buffer). */
export const MAX_STEPS = 64;

export class Sequencer {
  /**
   * @param {import('./audio-engine.js').AudioEngine} engine
   * @param {{ bpm?: number, waveform?: string }} [opts]
   */
  constructor(engine, { bpm = 120, waveform = 'triangle' } = {}) {
    this._engine = engine;
    this.bpm = bpm;
    this.waveform = waveform;
    this._steps = [];
    this._recording = false;
    // Stats p/ e2e/observabilidade (audiosFired = quantas vezes engine.play foi
    // chamado pelo sequencer; stepsRecorded = passos no buffer).
    this.stats = { stepsRecorded: 0, audiosFired: 0 };
  }

  get recording() { return this._recording; }

  /** Cópia imutável dos passos gravados. */
  get steps() { return this._steps.slice(); }

  /** Duração de 1 beat em ms conforme BPM atual (60 000 / BPM). */
  get stepDurationMs() { return Math.round(60_000 / this.bpm); }

  /** Inicia nova gravação (zera o buffer anterior). */
  startRecording() {
    this._steps = [];
    this._recording = true;
    this.stats.stepsRecorded = 0;
    this.stats.audiosFired = 0;
  }

  /** Para a gravação (mantém o buffer para replay/save). */
  stopRecording() {
    this._recording = false;
  }

  /**
   * Avança 1 passo musical: toca o áudio da entrada via AudioEngine e,
   * se gravando, registra o SeqStep no buffer.
   *
   * Extrai freqs do PackEntry.audio (synth → freqs; sequence → 1º passo;
   * speech → ignorado). Retorna o array de freqs tocadas ou null se sem som.
   *
   * @param {{ audio?: object }} entry - PackEntry com campo `audio`.
   */
  step(entry) {
    const audio = entry && entry.audio;
    if (!audio || audio.mode === 'speech') return null;

    let freqs;
    if (audio.mode === 'synth') {
      freqs = (audio.freqs || []).slice(0, MAX_VOICES);
    } else {
      // sequence → usa o 1º passo como representante do acorde/nota
      const first = (audio.steps || [])[0];
      freqs = first ? (first.freqs || []).slice(0, MAX_VOICES) : [];
    }
    if (!freqs.length) return null;

    // Toca via AudioEngine (reuso YG-155 — zero código de áudio próprio)
    this._engine.play(audio);
    this.stats.audiosFired++;

    if (this._recording && this._steps.length < MAX_STEPS) {
      this._steps.push({ freqs, duration_ms: this.stepDurationMs });
      this.stats.stepsRecorded++;
    }

    return freqs;
  }

  /**
   * Produz um objeto `EntryAudio::Sequence` válido (contrato YG-155) a partir
   * do buffer gravado. Retorna null se não há passos.
   *
   * @param {object} [envelope] - ADSR sobrescrito; usa default se omitido.
   */
  toSequence(envelope) {
    if (!this._steps.length) return null;
    return {
      mode: 'sequence',
      waveform: this.waveform,
      steps: this._steps.slice(),
      envelope: envelope || { attack_ms: 8, decay_ms: 120, sustain: 0.6, release_ms: 240 },
    };
  }

  /**
   * Reproduz o motivo gravado via AudioEngine. Retorna false se sem passos.
   */
  replay() {
    const seq = this.toSequence();
    if (!seq) return false;
    this._engine.unlock();
    return this._engine.play(seq);
  }
}
