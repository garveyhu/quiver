import Phaser from 'phaser';
import { EventBus, BUS, type TaskBoardSummary } from '@/game/EventBus';
import { PALETTE, CJK_FONT, SCENE } from '@/game/palette';
import { STR } from '@/strings';
import type { Settings } from '@/types/persistence.types';

const HUD_H = 44;
const BAR_W = 120; // budget power-slot width

interface ToastPayload {
  xp: number;
  cost: number | null;
}

/**
 * The top HUD, run as a parallel scene over the resident HallScene (never
 * blocks it). It shows three things, all diegetic and always-on:
 *
 *  - a budget power-slot ("壁炉/电力槽"): a fill bar + 余额 $x/$y text bound to
 *    the settings' budget cap (placeholder spend until the IPC surfaces it);
 *  - a soft clock;
 *  - a toast lane that flashes "完成 +XP · 花费 $x.xx" when a run finishes.
 *
 * It subscribes to the EventBus in `create()` and unsubscribes on `shutdown`.
 */
export class UIScene extends Phaser.Scene {
  private barBg?: Phaser.GameObjects.Rectangle;
  private barFill?: Phaser.GameObjects.Rectangle;
  private budgetText?: Phaser.GameObjects.Text;
  private clockText?: Phaser.GameObjects.Text;
  private busyText?: Phaser.GameObjects.Text;
  private panel?: Phaser.GameObjects.Rectangle;

  private budgetCap: number | null = null;
  private budgetSpent = 0; // placeholder — not surfaced by the current IPC

  constructor() {
    super({ key: SCENE.ui, active: false });
  }

  create(): void {
    // Phaser reuses the scene instance across stop→start, so clear every stale
    // GameObject handle before re-layout — otherwise `this.panel.setSize()` hits a
    // destroyed object (the §6 scene-recreate footgun the sub-scenes already guard).
    this.resetState();

    this.layout();
    this.scale.on('resize', this.layout, this);

    EventBus.on(BUS.settings, this.onSettings, this);
    EventBus.on(BUS.board, this.onBoard, this);
    EventBus.on('hud:toast', this.onToast, this);

    // tick the clock once a second.
    this.time.addEvent({ delay: 1000, loop: true, callback: this.tickClock, callbackScope: this });
    this.tickClock();

    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);
  }

  private resetState(): void {
    this.barBg = undefined;
    this.barFill = undefined;
    this.budgetText = undefined;
    this.clockText = undefined;
    this.busyText = undefined;
    this.panel = undefined;
  }

  private teardown(): void {
    EventBus.off(BUS.settings, this.onSettings, this);
    EventBus.off(BUS.board, this.onBoard, this);
    EventBus.off('hud:toast', this.onToast, this);
    this.scale.off('resize', this.layout, this);
  }

  private layout(): void {
    const { width } = this.scale;

    if (!this.panel) {
      this.panel = this.add.rectangle(0, 0, width, HUD_H, PALETTE.hudPanel, 0.92).setOrigin(0, 0);
      this.panel.setDepth(10_000).setScrollFactor(0);
    } else {
      this.panel.setSize(width, HUD_H);
    }

    // budget power-slot, left.
    if (!this.barBg) {
      this.barBg = this.add
        .rectangle(16, HUD_H / 2, BAR_W, 14, 0x000000, 0.3)
        .setOrigin(0, 0.5)
        .setStrokeStyle(1, PALETTE.hudPanelEdge)
        .setDepth(10_001);
      this.barFill = this.add
        .rectangle(17, HUD_H / 2, BAR_W - 2, 12, PALETTE.emberFull, 1)
        .setOrigin(0, 0.5)
        .setDepth(10_002);
      this.budgetText = this.add
        .text(16 + BAR_W + 10, HUD_H / 2, '', {
          fontFamily: CJK_FONT,
          fontSize: '14px',
          color: PALETTE.hudInk,
          fontStyle: 'bold',
        })
        .setOrigin(0, 0.5)
        .setDepth(10_002);
    }

    // clock, centre.
    if (!this.clockText) {
      this.clockText = this.add
        .text(width / 2, HUD_H / 2, '', {
          fontFamily: CJK_FONT,
          fontSize: '14px',
          color: PALETTE.hudInk,
        })
        .setOrigin(0.5, 0.5)
        .setDepth(10_002);
    } else {
      this.clockText.setX(width / 2);
    }

    // workshop-busy summary, right.
    if (!this.busyText) {
      this.busyText = this.add
        .text(width - 16, HUD_H / 2, '', {
          fontFamily: CJK_FONT,
          fontSize: '13px',
          color: PALETTE.hudInkDim,
        })
        .setOrigin(1, 0.5)
        .setDepth(10_002);
    } else {
      this.busyText.setX(width - 16);
    }

    this.renderBudget();
  }

  private onSettings(settings: Settings | null): void {
    this.budgetCap = settings ? (settings.nightlyBudgetUsd ?? settings.monthlyCreditCapUsd) : null;
    this.renderBudget();
  }

  private onBoard(summary: TaskBoardSummary): void {
    if (!this.busyText) return;
    const busy = summary.running;
    this.busyText.setText(
      busy > 0
        ? `${STR.hudBusyPrefix}${busy}${STR.hudBusySuffix}`
        : STR.hudIdle,
    );
  }

  private renderBudget(): void {
    if (!this.barFill || !this.budgetText) return;
    if (this.budgetCap == null || this.budgetCap <= 0) {
      // No cap configured → placeholder slot, neutral text.
      this.barFill.setSize(BAR_W - 2, 12).setFillStyle(PALETTE.emberLow, 0.8);
      this.budgetText.setText(STR.hudBudgetUnset);
      return;
    }
    const remaining = Math.max(0, this.budgetCap - this.budgetSpent);
    const ratio = Phaser.Math.Clamp(remaining / this.budgetCap, 0, 1);
    this.barFill.setSize(Math.max(1, (BAR_W - 2) * ratio), 12);
    // hot when full, banked embers when low.
    this.barFill.setFillStyle(ratio > 0.25 ? PALETTE.emberFull : PALETTE.emberLow, 1);
    this.budgetText.setText(
      `${STR.hudBudgetPrefix} $${remaining.toFixed(2)}/$${this.budgetCap.toFixed(2)}`,
    );
  }

  private tickClock(): void {
    if (!this.clockText) return;
    const d = new Date();
    const hh = String(d.getHours()).padStart(2, '0');
    const mm = String(d.getMinutes()).padStart(2, '0');
    this.clockText.setText(`☾ ${hh}:${mm}`);
  }

  private onToast(payload: ToastPayload): void {
    const { width } = this.scale;
    const costStr = payload.cost == null ? '—' : `$${payload.cost.toFixed(2)}`;
    const toast = this.add
      .text(width / 2, HUD_H + 24, `${STR.hudToastDone} +${payload.xp} XP · 花费 ${costStr}`, {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color: PALETTE.hudInk,
        backgroundColor: PALETTE.hudAccent,
        padding: { x: 14, y: 7 },
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5)
      .setDepth(10_010)
      .setAlpha(0);

    this.tweens.add({
      targets: toast,
      alpha: 1,
      y: HUD_H + 34,
      duration: 260,
      ease: 'Quad.easeOut',
      hold: 2200,
      yoyo: true,
      onComplete: () => toast.destroy(),
    });
  }
}
