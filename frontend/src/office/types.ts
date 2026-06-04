// The pixel-office domain types. Kept separate from Phaser so the pose state
// machine stays a pure, testable function with no rendering dependency.

// The 5 sprite poses v1 supports (a subset of §5.5 — coffee/sweating collapse
// into the tense `bow` pose for v1, see poseMachine.ts).
export type Pose = 'idle' | 'working' | 'tense' | 'celebrate' | 'sick';

// Pose -> sliced PNG suffix (scripts/slice_sprites.py output: char<N>-<suffix>.png).
export const POSE_FRAME: Record<Pose, string> = {
  idle: 'portrait',
  working: 'reading',
  tense: 'bow',
  celebrate: 'celebrate',
  sick: 'sick',
};

// Live view-model for one worker (one task). The Phaser scene renders this.
export interface WorkerView {
  taskId: string;
  slot: number; // stable 0-based index; character sheet = slot % 4 (4 archers)
  pose: Pose;
  bubble: string; // Chinese action text shown above the archer
  sparkle: boolean; // one-shot celebrate effect trigger
  terminal: boolean; // latched (celebrate/sick) — late events can't bounce it
}
