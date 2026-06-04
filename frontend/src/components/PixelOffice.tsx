import { useEffect, useReducer, useRef, useState } from 'react';
import Phaser from 'phaser';
import type { AgentEvent } from '@/types/agentEvent.types';
import { OfficeScene } from '@/office/OfficeScene';
import { createWorker, reduceWorker } from '@/office/poseMachine';
import type { WorkerView } from '@/office/types';
import { WorkerDetail } from '@/components/WorkerDetail';
import { STR } from '@/strings';

interface PixelOfficeProps {
  events: AgentEvent[];
}

// Cream-theme canvas background (matches --bg in styles.css).
const CANVAS_BG = '#fbf6ec';

/**
 * Hosts the Phaser cozy workshop and feeds it the live AgentEvent stream.
 *
 * The component owns the per-task `WorkerView` map + stable slot assignment and
 * replays the pose state machine, pushing each updated view into the scene
 * (which only renders). It also surfaces hover tooltips + a click-through detail
 * panel + drag, wired through the scene's interaction callbacks.
 */
export function PixelOffice({ events }: PixelOfficeProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const gameRef = useRef<Phaser.Game | null>(null);
  const sceneRef = useRef<OfficeScene | null>(null);

  // per-task view-model + slot bookkeeping, persisted across renders.
  const workersRef = useRef<Map<string, WorkerView>>(new Map());
  const nextSlotRef = useRef(0);
  const cursorRef = useRef(0);
  const flushRef = useRef<() => void>(() => {});
  // bump to force a re-render when worker views change (for tooltip/detail).
  const [, bump] = useReducer((n: number) => n + 1, 0);

  const [hovered, setHovered] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    if (!hostRef.current) return;
    const scene = new OfficeScene();
    scene.onReady = () => flushRef.current();
    scene.onHover = id => setHovered(id);
    scene.onSelect = id => setSelected(id);
    sceneRef.current = scene;
    const game = new Phaser.Game({
      type: Phaser.AUTO,
      parent: hostRef.current,
      backgroundColor: CANVAS_BG,
      pixelArt: true,
      scale: { mode: Phaser.Scale.RESIZE, width: '100%', height: '100%' },
      scene,
    });
    gameRef.current = game;

    // DEV-ONLY visual harness: drive archers through states without the backend.
    if (import.meta.env.DEV) {
      const w = window as unknown as { __office?: unknown; __officeScene?: unknown };
      w.__office = (v: WorkerView) => {
        workersRef.current.set(v.taskId, v);
        sceneRef.current?.applyWorker(v);
        bump();
      };
      w.__officeScene = scene;
    }

    return () => {
      sceneRef.current = null;
      game.destroy(true);
      gameRef.current = null;
    };
  }, []);

  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;

    const pushAll = () => {
      if (events.length < cursorRef.current) {
        workersRef.current.clear();
        nextSlotRef.current = 0;
        cursorRef.current = 0;
        setSelected(null);
        setHovered(null);
        scene.reset();
      }
      for (let i = cursorRef.current; i < events.length; i++) {
        const event = events[i];
        let view = workersRef.current.get(event.taskId);
        if (!view) view = createWorker(event.taskId, nextSlotRef.current++);
        view = reduceWorker(view, event);
        workersRef.current.set(event.taskId, view);
        scene.applyWorker(view);
      }
      cursorRef.current = events.length;
      bump();
    };

    flushRef.current = pushAll;
    if (scene.ready) pushAll();
  }, [events]);

  const hoveredView = hovered ? workersRef.current.get(hovered) : undefined;
  const selectedView = selected ? workersRef.current.get(selected) : undefined;
  const hasWorkers = workersRef.current.size > 0;

  return (
    <div className="pixel-office">
      <div ref={hostRef} className="pixel-office-canvas" />

      {!hasWorkers && <p className="pixel-office-empty">{STR.officeEmpty}</p>}
      {hasWorkers && <p className="pixel-office-hint">{STR.officeHint}</p>}

      {hoveredView && hoveredView.taskId !== selected && (
        <div className="worker-tooltip">
          <span className="worker-tooltip-action">{hoveredView.detail}</span>
          <span className="worker-tooltip-cost">
            {hoveredView.costUsd === null ? '' : `花费 $${hoveredView.costUsd.toFixed(2)}`}
          </span>
        </div>
      )}

      {selectedView && (
        <WorkerDetail worker={selectedView} onClose={() => setSelected(null)} />
      )}
    </div>
  );
}
