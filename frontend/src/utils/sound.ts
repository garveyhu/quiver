/**
 * Cozy sound-effect cues, synthesized with WebAudio (no vendored assets).
 *
 * We ship NO audio files: vendoring Kenney CC0 clips needs a stable download at
 * build time (their CDN paths rotate), and only CC0 may live in this repo. So
 * instead each cue is a tiny on-the-fly WebAudio blip — page-turn, pin-thunk,
 * verify-chime, fire-whoomph — giving real feedback at juice moments with zero
 * binary dependencies. To swap in real SFX later, load clips and play them in
 * `play()`; the call sites never change.
 *
 * Audio is created lazily on the first cue (browsers block an AudioContext until
 * a user gesture) and degrades to a silent no-op wherever WebAudio is missing —
 * a cue is always free to call and never throws.
 */

export type SoundCue = 'pin' | 'complete' | 'notify' | 'open' | 'close';

interface CueSpec {
  /** oscillator type. */
  type: OscillatorType;
  /** note sequence: [frequency Hz, start offset s, duration s, peak gain]. */
  notes: Array<[number, number, number, number]>;
}

// Each cue is a short two/three-note motif in the warm cozy register.
const CUES: Record<SoundCue, CueSpec> = {
  // a soft wooden "thunk" — a commission pinned to the board.
  pin: { type: 'triangle', notes: [[220, 0, 0.07, 0.18], [180, 0.04, 0.1, 0.12]] },
  // a bright rising chime — a run verified.
  complete: {
    type: 'sine',
    notes: [[523.25, 0, 0.12, 0.16], [659.25, 0.1, 0.12, 0.16], [783.99, 0.2, 0.18, 0.18]],
  },
  // a gentle two-note ping — something gained an update.
  notify: { type: 'sine', notes: [[587.33, 0, 0.08, 0.12], [880, 0.07, 0.1, 0.12]] },
  // a warm "page/scroll opens" swell.
  open: { type: 'triangle', notes: [[330, 0, 0.09, 0.12], [440, 0.06, 0.12, 0.12]] },
  // the reverse — a book/scroll closing.
  close: { type: 'triangle', notes: [[440, 0, 0.08, 0.12], [294, 0.06, 0.12, 0.1]] },
};

let ctx: AudioContext | null = null;
let unavailable = false;

function context(): AudioContext | null {
  if (unavailable) return null;
  if (ctx) return ctx;
  try {
    const Ctor =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctor) {
      unavailable = true;
      return null;
    }
    ctx = new Ctor();
    return ctx;
  } catch {
    unavailable = true;
    return null;
  }
}

/** Play a cozy cue. Lazily boots WebAudio; a silent no-op if audio is unavailable. */
export function play(cue: SoundCue): void {
  const ac = context();
  if (!ac) return;
  // resume if the gesture-gate suspended it; ignore failures (still pre-gesture).
  if (ac.state === 'suspended') void ac.resume().catch(() => {});

  const spec = CUES[cue];
  const now = ac.currentTime;
  for (const [freq, at, dur, peak] of spec.notes) {
    const osc = ac.createOscillator();
    const gain = ac.createGain();
    osc.type = spec.type;
    osc.frequency.value = freq;
    // a quick attack + smooth exponential decay so cues read as soft, not beepy.
    const t0 = now + at;
    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.exponentialRampToValueAtTime(peak, t0 + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
    osc.connect(gain).connect(ac.destination);
    osc.start(t0);
    osc.stop(t0 + dur + 0.02);
  }
}
