/**
 * Sound-effect hooks for cozy feedback — currently NO-OP STUBS.
 *
 * Quiver v1.0 ships without audio assets, so these intentionally do nothing.
 * They give the UI a single, clearly-named seam to call at juice moments
 * (a commission pinned, a run finished, a tab gaining an update) so that
 * dropping in real cozy SFX later is a one-file change — no hunting through
 * components. Calling them today is free and never plays broken audio.
 *
 * To wire real audio later: load the clips here and play them in `play()`,
 * gated behind a settings toggle. Until then every call is a deliberate no-op.
 */

export type SoundCue = 'pin' | 'complete' | 'notify' | 'open' | 'close';

/** Play a cozy cue. No-op until an audio layer is added (see module docs). */
export function play(_cue: SoundCue): void {
  // Intentionally empty: no audio assets shipped in v1.0.
}
