// The pixel-office domain types. Kept separate from Phaser so the pose state
// machine stays a pure, testable function with no rendering dependency.

// The behavioural states one archer can be in. Each maps to a frame animation
// (walk / bow) or a single sliced frame (idle / reading / celebrate / sick) in
// the Phaser scene — see OfficeScene.applyState.
export type Pose =
  | 'arriving' // walking in to the station (walk strip)
  | 'idle' // standing relaxed, waiting (idle frame + breathing bob)
  | 'working' // sitting reading — tool_use / output (reading frame + focus bob)
  | 'tense' // drawing the bow — Bash / pre-gate (bow strip loop)
  | 'celebrate' // fist-pump jump — verified (celebrate frame + hop)
  | 'sick'; // slumped / dejected — failed (sick frame + sag)

// Pose -> sliced asset suffix. Strips animate; singles are static frames the
// scene gives subtle tweens. (`arriving` reuses the walk strip.)
export const POSE_FRAME: Record<Pose, string> = {
  arriving: 'walk',
  idle: 'idle',
  working: 'reading',
  tense: 'bow',
  celebrate: 'celebrate',
  sick: 'sick',
};

// A compact record of one streamed event, surfaced live in the worker's detail
// panel (tool calls / file edits / outputs / cost).
export interface WorkerLogEntry {
  seq: number;
  kind: string;
  text: string;
}

// Live view-model for one worker (one task). The Phaser scene renders this.
export interface WorkerView {
  taskId: string;
  slot: number; // stable 0-based index; character sheet = slot % 4 (4 archers)
  pose: Pose;
  bubble: string; // Chinese action text shown above the archer
  detail: string; // richer current-action line for the tooltip / detail panel
  costUsd: number | null; // running / final cost, shown in tooltip + XP popup
  sparkle: boolean; // one-shot celebrate effect trigger
  terminal: boolean; // latched (celebrate/sick) — late events can't bounce it
  log: WorkerLogEntry[]; // append-only event trail for the detail panel
}
