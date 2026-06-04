import { useEffect, useRef } from 'react';
import Phaser from 'phaser';
import type { AgentEvent } from '@/types/agentEvent.types';
import { OfficeScene } from '@/office/OfficeScene';
import { createWorker, reduceWorker } from '@/office/poseMachine';
import type { WorkerView } from '@/office/types';
import { STR } from '@/strings';

interface PixelOfficeProps {
  events: AgentEvent[];
}

// Cream-theme canvas background (matches --bg in styles.css).
const CANVAS_BG = '#fbf6ec';

/**
 * Hosts the Phaser pixel office and feeds it the live AgentEvent stream.
 *
 * The component owns the per-task `WorkerView` map + slot assignment (stable,
 * first-seen order; the character sheet is slot % 4) and replays the pose state
 * machine, then pushes each updated view into the Phaser scene. The scene only
 * renders — all event reduction stays here in plain TS.
 */
export function PixelOffice({ events }: PixelOfficeProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const gameRef = useRef<Phaser.Game | null>(null);
  const sceneRef = useRef<OfficeScene | null>(null);

  // per-task view-model + slot bookkeeping, persisted across renders.
  const workersRef = useRef<Map<string, WorkerView>>(new Map());
  const nextSlotRef = useRef(0);
  // how many events we've already folded into the scene (incremental replay).
  const cursorRef = useRef(0);
  // latest replay closure, so the scene's onReady can flush the backlog.
  const flushRef = useRef<() => void>(() => {});

  useEffect(() => {
    if (!hostRef.current) return;
    const scene = new OfficeScene();
    scene.onReady = () => flushRef.current();
    sceneRef.current = scene;
    const game = new Phaser.Game({
      type: Phaser.AUTO,
      parent: hostRef.current,
      backgroundColor: CANVAS_BG,
      pixelArt: true, // crisp nearest-neighbour upscaling
      scale: {
        mode: Phaser.Scale.RESIZE,
        width: '100%',
        height: '100%',
      },
      scene,
    });
    gameRef.current = game;

    // DEV-ONLY visual harness: expose applyWorker so a headless screenshot can
    // drive archers through poses without the Tauri backend. Stripped in prod.
    if (import.meta.env.DEV) {
      (window as unknown as { __office?: unknown }).__office = (v: WorkerView) =>
        sceneRef.current?.applyWorker(v);
    }

    return () => {
      sceneRef.current = null;
      game.destroy(true);
      gameRef.current = null;
    };
  }, []);

  // Fold events into worker views and push to the scene. Re-runs whenever the
  // event array grows (or resets on a new run, when length drops to 0).
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;

    const pushAll = () => {
      // a fresh run clears the array — detect the reset and wipe the office.
      if (events.length < cursorRef.current) {
        workersRef.current.clear();
        nextSlotRef.current = 0;
        cursorRef.current = 0;
        scene.reset();
      }
      for (let i = cursorRef.current; i < events.length; i++) {
        const event = events[i];
        let view = workersRef.current.get(event.taskId);
        if (!view) {
          view = createWorker(event.taskId, nextSlotRef.current++);
        }
        view = reduceWorker(view, event);
        workersRef.current.set(event.taskId, view);
        scene.applyWorker(view);
      }
      cursorRef.current = events.length;
    };

    // keep the latest closure reachable by the scene's onReady callback.
    flushRef.current = pushAll;

    if (scene.ready) {
      pushAll();
    }
    // if the scene isn't ready yet, scene.onReady -> flushRef.current() runs it.
  }, [events]);

  return (
    <div className="pixel-office">
      <div ref={hostRef} className="pixel-office-canvas" />
      {events.length === 0 && <p className="pixel-office-empty">{STR.officeEmpty}</p>}
    </div>
  );
}
