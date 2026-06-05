import Phaser from 'phaser';
import { EventBus, BUS, type SettingsPatch } from '@/game/EventBus';
import { requestTextInput } from '@/game/requestTextInput';
import { PALETTE, CJK_FONT, SCENE } from '@/game/palette';
import { fadeIn, fadeOutThen } from '@/game/transition';
import { STR } from '@/strings';
import { play } from '@/utils/sound';
import type { Settings } from '@/types/persistence.types';
import type { SaveStatus } from '@/hooks/useSettings';
import type { EnvironmentCheck, CheckStatus } from '@/types/environment.types';
import type { HealthSnapshot } from '@/game/EventBus';

// The settings groups become the ledger's page tabs. The first four are lifted
// verbatim from the old React SettingsPanel sections; the fifth (工坊体检) hosts
// the pre-flight self-diagnosis panel (M4-UI / DESIGN §9).
type GroupKey = 'run' | 'budget' | 'agent' | 'appearance' | 'health';

interface GroupSpec {
  key: GroupKey;
  label: string;
}

const GROUPS: GroupSpec[] = [
  { key: 'run', label: STR.settingsSectionRun },
  { key: 'budget', label: STR.settingsSectionBudget },
  { key: 'agent', label: STR.settingsSectionAgent },
  { key: 'appearance', label: STR.settingsSectionAppearance },
  { key: 'health', label: STR.settingsSectionHealth },
];

// Status glyph + colour for one health row.
const HEALTH_GLYPH: Record<CheckStatus, string> = { ok: '✓', warn: '⚠', fail: '✗' };

const MODEL_OPTIONS = ['sonnet', 'opus', 'haiku'] as const;

// Layout metrics for one field row inside a page.
const ROW = {
  height: 86,
  labelSize: '16px',
  hintSize: '12px',
} as const;

/**
 * The in-world 设置账本 (P3): a diegetic open ledger book. Clicking the Hall's
 * ledger hotspot sleeps the Hall (the world stays warm) and launches this scene —
 * a dim wash, a leather-bound book opened to two parchment pages, with the four
 * settings groups as leather page-tabs down the side.
 *
 * Fields are world objects, not an HTML form:
 *  - boolean / enum (默认模式, 主题) → a carved wooden lever that slides between
 *    the two options;
 *  - whole numbers (并发弓匠数, 模拟节奏, 界面缩放) → a ◀ n ▶ brass knob stepper;
 *  - free text / money ($预算上限, 模型名) → a parchment cell that, on click,
 *    summons the IME-safe React text overlay and writes the committed value back.
 *
 * Every edit flows over the EventBus as a {@link SettingsPatch} command — the
 * scene never touches IPC. GameBridge routes it to the unchanged useSettings.patch
 * (the debounced settings seam). The scene subscribes in `create()` and tears
 * every listener down on `shutdown` (the §6 scene-recreate footgun).
 */
export class SettingsScene extends Phaser.Scene {
  private scrim?: Phaser.GameObjects.Rectangle;
  // An invisible interactive backstop over the book + its tabs. It swallows
  // pointer events so a click on the parchment (or a stray pointerup left behind
  // when the IME overlay closes) never falls through to the scrim's close handler.
  private backstop?: Phaser.GameObjects.Rectangle;
  private book?: Phaser.GameObjects.Container;
  private tabs: Phaser.GameObjects.Container[] = [];
  private pageLayer?: Phaser.GameObjects.Container;
  private backBtn?: Phaser.GameObjects.Container;
  private saveChip?: Phaser.GameObjects.Container;
  private saveChipText?: Phaser.GameObjects.Text;

  // book geometry, recomputed on every (re)layout so field controls can place
  // their interactive zones in scene space.
  private bookRect = { x: 0, y: 0, w: 0, h: 0 };

  private settings: Settings | null = null;
  private saveStatus: SaveStatus = 'idle';
  private activeGroup: GroupKey = 'run';

  // Workshop pre-flight self-diagnosis (DESIGN §9), pushed from useEnvironmentCheck
  // over BUS.health. Only the 工坊体检 page reads it; null until the first push.
  private health: HealthSnapshot | null = null;

  // True while the text overlay is open, so a second cell click can't double-open.
  private cellEditing = false;

  constructor() {
    super(SCENE.settings);
  }

  create(): void {
    // Phaser reuses the scene instance across stop→start, so clear every stale
    // GameObject handle before re-layout (the §6 scene-recreate footgun).
    this.resetState();

    this.layout();
    this.scale.on('resize', this.layout, this);

    EventBus.on(BUS.settings, this.onSettings, this);
    EventBus.on(BUS.settingsSave, this.onSaveStatus, this);
    EventBus.on(BUS.health, this.onHealth, this);
    // Ask the bridge to flush the current settings snapshot now that we're up.
    EventBus.emit(BUS.sceneReady);

    // Flip the ledger in with a camera fade over the slept world (honours motion).
    fadeIn(this, PALETTE.ledgerScrim);

    this.events.once(Phaser.Scenes.Events.SHUTDOWN, this.teardown, this);

    if (import.meta.env.DEV) {
      (window as unknown as { __settingsScene?: unknown }).__settingsScene = this;
    }
  }

  private resetState(): void {
    this.scrim = undefined;
    this.backstop = undefined;
    this.book = undefined;
    this.tabs = [];
    this.pageLayer = undefined;
    this.backBtn = undefined;
    this.saveChip = undefined;
    this.saveChipText = undefined;
    this.activeGroup = 'run';
    this.cellEditing = false;
  }

  private teardown(): void {
    EventBus.off(BUS.settings, this.onSettings, this);
    EventBus.off(BUS.settingsSave, this.onSaveStatus, this);
    EventBus.off(BUS.health, this.onHealth, this);
    this.scale.off('resize', this.layout, this);
  }

  // Close the book: fade out, then stop this scene and wake the (slept, still-warm)
  // world. The hall fades itself back in on WAKE. Reduced-motion = instant cut.
  private close(): void {
    play('close');
    fadeOutThen(
      this,
      () => {
        this.scene.stop(SCENE.settings);
        this.scene.wake(SCENE.hall);
      },
      PALETTE.ledgerScrim,
    );
  }

  // --- bus handlers -------------------------------------------------------

  private onSettings(settings: Settings | null): void {
    this.settings = settings;
    this.renderPage();
  }

  private onSaveStatus(status: SaveStatus): void {
    this.saveStatus = status;
    this.renderSaveChip();
  }

  private onHealth(snapshot: HealthSnapshot): void {
    this.health = snapshot;
    // Only the 工坊体检 page renders health; avoid churning other pages.
    if (this.activeGroup === 'health') this.renderPage();
  }

  // The single mutation seam: every control hands a partial patch here, which the
  // bridge forwards to useSettings.patch (debounced). Optimistically reflect it so
  // the control redraws immediately (the hook's own settings echo confirms it).
  private applyPatch(patch: SettingsPatch): void {
    if (this.settings) {
      this.settings = { ...this.settings, ...stripUndefined(patch) };
    }
    EventBus.emit(BUS.settingsPatch, patch);
    this.renderPage();
  }

  // --- layout (scrim / book / tabs / chrome) ------------------------------

  private layout(): void {
    const { width, height } = this.scale;

    if (!this.scrim) {
      this.scrim = this.add
        .rectangle(0, 0, width, height, PALETTE.ledgerScrim, 0.5)
        .setOrigin(0, 0)
        .setDepth(0)
        .setInteractive();
      // a click on the wash (outside the book) closes the ledger.
      this.scrim.on('pointerup', () => this.close());
    } else {
      this.scrim.setSize(width, height).setPosition(0, 0);
    }

    const bookW = Math.min(720, width - 80);
    const bookH = Math.min(560, height - 110);
    const cx = width / 2;
    const cy = height / 2 + 14;
    this.bookRect = { x: cx, y: cy, w: bookW, h: bookH };

    this.buildBackstop(cx, cy, bookW, bookH);
    this.buildBook(cx, cy, bookW, bookH);
    this.buildTabs(cx, cy, bookW);
    this.buildChrome(cx, cy, bookW, bookH);
    this.renderPage();
  }

  // The interactive backstop: an invisible rect over the book + the tab column on
  // its right edge. It sits above the scrim (depth 0) but below the book art
  // (depth 10) and every control, so non-control clicks on the parchment are
  // simply swallowed here instead of falling through to the scrim → close().
  private buildBackstop(cx: number, cy: number, w: number, h: number): void {
    const padX = 24; // generous margin so the tab column (right) is covered too
    const padY = 16;
    const bw = w + padX * 2 + 100; // +100 reaches past the right-edge tabs
    const bh = h + padY * 2;
    if (!this.backstop) {
      this.backstop = this.add
        .rectangle(cx, cy, bw, bh, 0x000000, 0)
        .setDepth(5)
        .setInteractive();
      // swallow the event so it never bubbles to the scrim below.
      this.backstop.on('pointerup', (p: Phaser.Input.Pointer) => {
        p.event.stopPropagation();
      });
    } else {
      this.backstop.setSize(bw, bh).setPosition(cx, cy);
    }
  }

  // The leather-bound open book: cover, two parchment pages, a centre spine.
  private buildBook(cx: number, cy: number, w: number, h: number): void {
    this.book?.destroy();
    const book = this.add.container(cx, cy).setDepth(10);

    const shadow = this.add.graphics();
    shadow.fillStyle(0x000000, 0.35);
    shadow.fillRoundedRect(-w / 2 + 8, -h / 2 + 12, w, h, 18);

    const g = this.add.graphics();
    // leather cover (outer edge).
    g.fillStyle(PALETTE.ledgerCoverEdge, 1);
    g.fillRoundedRect(-w / 2, -h / 2, w, h, 18);
    g.fillStyle(PALETTE.ledgerCover, 1);
    g.fillRoundedRect(-w / 2 + 6, -h / 2 + 6, w - 12, h - 12, 14);

    // two parchment pages, meeting at the spine.
    const pad = 22;
    const pageW = (w - pad * 2 - 12) / 2;
    const pageH = h - pad * 2;
    const pageTop = -h / 2 + pad;
    const leftX = -w / 2 + pad;
    const rightX = 6;
    for (const px of [leftX, rightX]) {
      g.fillStyle(PALETTE.ledgerPage, 1);
      g.fillRoundedRect(px, pageTop, pageW, pageH, 6);
      // inner-margin shade near the spine gives the open-book curl.
      const shadeX = px === leftX ? px + pageW - 16 : px;
      g.fillStyle(PALETTE.ledgerPageShade, 0.5);
      g.fillRoundedRect(shadeX, pageTop, 16, pageH, 6);
      g.lineStyle(1.5, PALETTE.ledgerPageEdge, 0.8);
      g.strokeRoundedRect(px, pageTop, pageW, pageH, 6);
    }
    // centre spine.
    g.fillStyle(PALETTE.ledgerSpine, 1);
    g.fillRect(-6, pageTop, 12, pageH);

    // title engraved on the leather top band (above the pages, off the spine).
    const title = this.add
      .text(0, -h / 2 + pad / 2 + 1, STR.settingsSceneTitle, {
        fontFamily: CJK_FONT,
        fontSize: '12px',
        color: PALETTE.ledgerTabInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);

    book.add([shadow, g, title]);
    this.book = book;

    // a fresh page layer sits above the book art; renderPage fills it.
    this.pageLayer?.destroy();
    this.pageLayer = this.add.container(cx, cy).setDepth(20);
  }

  // The four groups as leather page-tabs riding the book's right edge.
  private buildTabs(cx: number, cy: number, w: number): void {
    for (const t of this.tabs) t.destroy();
    this.tabs = [];

    const tabW = 96;
    const tabH = 44;
    const gap = 8;
    const totalH = GROUPS.length * tabH + (GROUPS.length - 1) * gap;
    const startY = cy - totalH / 2 + tabH / 2;
    // Ride the book's right edge, but clamp so the tabs never spill past the
    // window's right side on narrow widths (640px), where the book + tab gutter
    // would otherwise overflow.
    const x = Math.min(cx + w / 2 + tabW / 2 - 10, this.scale.width - tabW / 2 - 4);

    GROUPS.forEach((group, i) => {
      const y = startY + i * (tabH + gap);
      const tab = this.makeTab(group, tabW, tabH);
      tab.setPosition(x, y).setDepth(8);
      this.tabs.push(tab);
    });
  }

  private makeTab(group: GroupSpec, w: number, h: number): Phaser.GameObjects.Container {
    const active = group.key === this.activeGroup;
    const c = this.add.container(0, 0);
    const bg = this.add.graphics();
    const fill = active ? PALETTE.ledgerTabActive : PALETTE.ledgerTab;
    bg.fillStyle(fill, 1);
    // round only the outer (right) edge so the tab reads as a page-marker.
    bg.fillRoundedRect(-w / 2, -h / 2, w, h, { tl: 0, tr: 10, bl: 0, br: 10 });
    bg.lineStyle(2, PALETTE.ledgerCoverEdge, 0.5);
    bg.strokeRoundedRect(-w / 2, -h / 2, w, h, { tl: 0, tr: 10, bl: 0, br: 10 });
    const label = this.add
      .text(6, 0, group.label, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: active ? PALETTE.ledgerTabInkActive : PALETTE.ledgerTabInk,
        fontStyle: active ? 'bold' : 'normal',
      })
      .setOrigin(0.5, 0.5);
    c.add([bg, label]);
    c.setSize(w, h);
    c.setInteractive({ useHandCursor: true });
    c.on('pointerup', () => {
      if (this.activeGroup === group.key) return;
      play('open');
      this.activeGroup = group.key;
      this.buildTabs(this.bookRect.x, this.bookRect.y, this.bookRect.w);
      this.renderPage();
    });
    c.on('pointerover', () => {
      if (group.key !== this.activeGroup) c.setScale(1.04);
    });
    c.on('pointerout', () => c.setScale(1));
    return c;
  }

  private buildChrome(cx: number, cy: number, w: number, h: number): void {
    this.backBtn?.destroy();
    this.backBtn = this.makeButton(
      cx - w / 2 + 64,
      cy + h / 2 - 26,
      STR.settingsSceneBack,
      () => this.close(),
    ).setDepth(30);

    this.saveChip?.destroy();
    const chip = this.add.container(cx + w / 2 - 64, cy + h / 2 - 26).setDepth(30);
    const bg = this.add.graphics();
    const text = this.add
      .text(0, 0, '', {
        fontFamily: CJK_FONT,
        fontSize: '13px',
        color: PALETTE.ledgerSaveInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    chip.add([bg, text]);
    chip.setData('bg', bg);
    this.saveChip = chip;
    this.saveChipText = text;
    this.renderSaveChip();
  }

  private renderSaveChip(): void {
    if (!this.saveChip || !this.saveChipText) return;
    if (this.saveStatus === 'idle') {
      this.saveChip.setVisible(false);
      return;
    }
    const { label, color } =
      this.saveStatus === 'saving'
        ? { label: STR.settingsSaving, color: PALETTE.ledgerSaveSaving }
        : this.saveStatus === 'saved'
          ? { label: STR.settingsSaved, color: PALETTE.ledgerSaveSaved }
          : { label: STR.settingsError, color: PALETTE.ledgerSaveError };
    this.saveChip.setVisible(true);
    this.saveChipText.setText(label);
    const bg = this.saveChip.getData('bg') as Phaser.GameObjects.Graphics;
    const padX = 10;
    const padY = 5;
    const bw = this.saveChipText.width + padX * 2;
    const bh = this.saveChipText.height + padY * 2;
    bg.clear();
    bg.fillStyle(color, 1);
    bg.fillRoundedRect(-bw / 2, -bh / 2, bw, bh, 8);
  }

  // --- page contents (one group of field rows per page) -------------------

  private renderPage(): void {
    if (!this.pageLayer) return;
    this.pageLayer.removeAll(true);

    // The 工坊体检 page is a bespoke status panel, not a field-rows form — it reads
    // the health snapshot (not Settings), so it renders ahead of the settings gate.
    if (this.activeGroup === 'health') {
      this.renderHealthPage();
      return;
    }

    if (!this.settings) {
      const loading = this.add
        .text(0, 0, STR.settingsSaving, {
          fontFamily: CJK_FONT,
          fontSize: '15px',
          color: PALETTE.ledgerInkDim,
        })
        .setOrigin(0.5, 0.5);
      this.pageLayer.add(loading);
      return;
    }

    const rows = this.rowsForGroup(this.activeGroup, this.settings);
    // Lay the rows down the two pages: fill the left page first, then the right.
    const { w, h } = this.bookRect;
    const pad = 22;
    const pageW = (w - pad * 2 - 12) / 2;
    const pageTop = -h / 2 + pad + 26;
    const usableH = h - pad * 2 - 40;
    const perPage = Math.max(1, Math.floor(usableH / ROW.height));
    const leftColX = -w / 2 + pad + 18;
    const rightColX = 6 + 18;

    rows.forEach((row, i) => {
      const onLeft = i < perPage;
      const colX = onLeft ? leftColX : rightColX;
      const idx = onLeft ? i : i - perPage;
      const y = pageTop + idx * ROW.height;
      const node = this.buildFieldRow(row, colX, y, pageW - 36);
      this.pageLayer!.add(node);
    });
  }

  // --- 工坊体检 page (M4-UI / DESIGN §9) -----------------------------------

  // The pre-flight self-diagnosis panel: a one-line overall verdict across the top
  // band, then one ✓/⚠/✗ row per probe flowing left page → right page, and a
  // 重新检查 button anchored to the right page's foot. Reads BUS.health (not
  // Settings), so it owns its own loading / error / empty states.
  private renderHealthPage(): void {
    if (!this.pageLayer) return;
    const { w, h } = this.bookRect;
    const pad = 22;
    const pageW = (w - pad * 2 - 12) / 2;
    const pageTop = -h / 2 + pad + 24;
    const leftColX = -w / 2 + pad + 16;
    const rightColX = 6 + 16;

    const snap = this.health;

    // command-level failure or in-flight / empty states.
    if (!snap || (snap.loading && snap.checks.length === 0)) {
      this.addHealthCentreNote(snap?.loading ? STR.healthChecking : STR.healthEmpty);
      this.buildRecheckButton();
      return;
    }
    if (snap.error) {
      this.addHealthCentreNote(STR.healthError, PALETTE.healthFail);
      this.buildRecheckButton();
      return;
    }
    if (snap.checks.length === 0) {
      this.addHealthCentreNote(STR.healthEmpty);
      this.buildRecheckButton();
      return;
    }

    // overall verdict banner (full width across both pages, above the rows).
    const verdict = this.healthVerdict(snap.checks);
    const banner = this.add
      .text(0, pageTop - 6, verdict.text, {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color: verdict.color,
        fontStyle: 'bold',
        align: 'center',
      })
      .setOrigin(0.5, 0);
    this.pageLayer.add(banner);

    // Flow one row per probe down the left page, spilling to the right page when
    // the column fills. Rows are variable-height (warn/fail rows expand their
    // remediation), so we pack by each row's measured height instead of a grid —
    // this keeps a wrapped 2-line remediation from overlapping the next row.
    const rowsTop = pageTop + 30;
    const colBottom = h / 2 - pad - 70; // leave room for the 重新检查 button foot
    const rowGap = 12;
    let cursorY = rowsTop;
    let colX = leftColX;
    let onRight = false;

    for (const check of snap.checks) {
      const { node, height } = this.buildHealthRow(check, colX, cursorY, pageW - 28);
      // if this row would overrun the current column, jump to the right page once.
      if (cursorY + height > colBottom && !onRight) {
        onRight = true;
        colX = rightColX;
        cursorY = rowsTop;
        node.setPosition(colX, cursorY);
      }
      this.pageLayer!.add(node);
      cursorY += height + rowGap;
    }

    this.buildRecheckButton();
  }

  private healthVerdict(checks: EnvironmentCheck[]): { text: string; color: string } {
    if (checks.some(c => c.status === 'fail')) {
      return { text: STR.healthHasFail, color: PALETTE.healthFail };
    }
    if (checks.some(c => c.status === 'warn')) {
      return { text: STR.healthHasWarn, color: PALETTE.healthWarn };
    }
    return { text: STR.healthAllReady, color: PALETTE.healthOk };
  }

  private addHealthCentreNote(text: string, color: string = PALETTE.ledgerInkDim): void {
    const note = this.add
      .text(0, -10, text, {
        fontFamily: CJK_FONT,
        fontSize: '15px',
        color,
        align: 'center',
        wordWrap: { width: this.bookRect.w - 120 },
      })
      .setOrigin(0.5, 0.5);
    this.pageLayer!.add(note);
  }

  // A single health row: a status glyph + label on the first line, the message on
  // the second, and (for warn/fail) the remediation indented below in muted ink.
  // Returns the container + its measured height so the caller can pack rows by
  // their real (variable) extent rather than a fixed grid.
  private buildHealthRow(
    check: EnvironmentCheck,
    x: number,
    y: number,
    width: number,
  ): { node: Phaser.GameObjects.Container; height: number } {
    const c = this.add.container(x, y);
    const glyphColor =
      check.status === 'ok'
        ? PALETTE.healthOk
        : check.status === 'warn'
          ? PALETTE.healthWarn
          : PALETTE.healthFail;

    const glyph = this.add
      .text(0, 0, HEALTH_GLYPH[check.status], {
        fontFamily: CJK_FONT,
        fontSize: '16px',
        color: glyphColor,
        fontStyle: 'bold',
      })
      .setOrigin(0, 0);
    c.add(glyph);

    const label = this.add
      .text(24, 0, check.label, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: PALETTE.ledgerInk,
        fontStyle: 'bold',
      })
      .setOrigin(0, 0);
    c.add(label);

    const message = this.add
      .text(24, 18, check.message, {
        fontFamily: CJK_FONT,
        fontSize: '11px',
        color: PALETTE.ledgerInkDim,
        wordWrap: { width: width - 24 },
        lineSpacing: 2,
      })
      .setOrigin(0, 0);
    c.add(message);

    // the row's vertical extent: label/glyph line + the wrapped message, plus the
    // remediation block for warn/fail rows.
    let height = 18 + message.height + 4;

    // warn/fail rows expand their remediation (the §9 "what to do about it").
    if (check.remediation) {
      const fixY = height;
      const fix = this.add
        .text(24, fixY, `${STR.healthFixTitle}：${check.remediation}`, {
          fontFamily: CJK_FONT,
          fontSize: '11px',
          color: PALETTE.healthRemediation,
          fontStyle: 'italic',
          wordWrap: { width: width - 24 },
          lineSpacing: 2,
        })
        .setOrigin(0, 0);
      c.add(fix);
      height = fixY + fix.height + 2;
    }

    return { node: c, height };
  }

  // The 重新检查 button on the right page's foot. Emits BUS.healthRefresh so the
  // bridge re-runs check_environment (the scene never touches IPC).
  private buildRecheckButton(): void {
    const { w, h } = this.bookRect;
    const x = 6 + (w / 2 - 22) / 2;
    const y = h / 2 - 56;
    const btn = this.makeButton(x, y, STR.healthRecheck, () => {
      play('open');
      EventBus.emit(BUS.healthRefresh);
    });
    this.pageLayer!.add(btn);
  }

  // Each group's fields, mapped 1:1 from the old SettingsPanel sections. The
  // 工坊体检 page is bespoke (renderHealthPage), so it's excluded from this switch.
  private rowsForGroup(group: Exclude<GroupKey, 'health'>, s: Settings): FieldRow[] {
    switch (group) {
      case 'run':
        return [
          {
            kind: 'lever',
            label: STR.settingDefaultMode,
            value: s.defaultMode,
            options: [
              { value: 'simulate', label: STR.settingDefaultModeSimulate },
              { value: 'real', label: STR.settingDefaultModeReal },
            ],
            onChange: v => this.applyPatch({ defaultMode: v }),
          },
          {
            // The model is free text (the §1 map: 模型名 → summon the IME overlay),
            // so it's a click-to-edit parchment cell, not a fixed lever.
            kind: 'text',
            label: STR.settingModel,
            value: s.model,
            placeholder: STR.settingModelTapHint,
            onChange: v => this.applyPatch({ model: v ?? MODEL_OPTIONS[0] }),
          },
          {
            kind: 'knob',
            label: STR.settingMaxWorkers,
            value: s.maxWorkers,
            min: 1,
            max: 8,
            step: 1,
            unit: STR.settingMaxWorkersUnit,
            onChange: v => this.applyPatch({ maxWorkers: v }),
          },
          {
            kind: 'knob',
            label: STR.settingFakeDelay,
            value: s.fakeDelayMs,
            min: 0,
            max: 600,
            step: 20,
            unit: STR.settingFakeDelayUnit,
            onChange: v => this.applyPatch({ fakeDelayMs: v }),
          },
        ];
      case 'budget':
        return [
          {
            kind: 'cell',
            label: STR.settingMonthlyCap,
            value: s.monthlyCreditCapUsd,
            prefix: '$',
            placeholder: STR.settingBudgetTapHint,
            onChange: v => this.applyPatch({ monthlyCreditCapUsd: v }),
          },
          {
            kind: 'cell',
            label: STR.settingNightlyBudget,
            value: s.nightlyBudgetUsd,
            prefix: '$',
            placeholder: STR.settingBudgetTapHint,
            onChange: v => this.applyPatch({ nightlyBudgetUsd: v }),
          },
        ];
      case 'agent':
        return [
          {
            kind: 'text',
            label: STR.settingAgentBin,
            value: s.agentBinOverride,
            placeholder: STR.settingAgentBinPlaceholder,
            onChange: v => this.applyPatch({ agentBinOverride: v }),
          },
        ];
      case 'appearance':
        return [
          {
            kind: 'lever',
            label: STR.settingTheme,
            value: s.theme,
            options: [
              { value: 'cozy', label: STR.settingThemeCozy },
              { value: 'midnight', label: STR.settingThemeMidnight },
            ],
            onChange: v => this.applyPatch({ theme: v }),
          },
          {
            kind: 'knob',
            label: STR.settingUiScale,
            value: Math.round(s.uiScale * 100),
            min: 80,
            max: 140,
            step: 5,
            unit: STR.settingUiScaleUnit,
            onChange: pct => this.applyPatch({ uiScale: pct / 100 }),
          },
        ];
    }
  }

  // A field row = a label (top) + its control (bottom) + a faint rule line. The
  // control type is chosen by `row.kind`.
  private buildFieldRow(
    row: FieldRow,
    x: number,
    y: number,
    width: number,
  ): Phaser.GameObjects.Container {
    const c = this.add.container(x, y);

    const label = this.add
      .text(0, 0, row.label, {
        fontFamily: CJK_FONT,
        fontSize: ROW.labelSize,
        color: PALETTE.ledgerInk,
        fontStyle: 'bold',
      })
      .setOrigin(0, 0);
    c.add(label);

    const controlY = 30;
    let control: Phaser.GameObjects.Container;
    switch (row.kind) {
      case 'lever':
        control = this.buildLever(row, width);
        break;
      case 'knob':
        control = this.buildKnob(row, width);
        break;
      case 'cell':
        control = this.buildMoneyCell(row, width);
        break;
      case 'text':
        control = this.buildTextCell(row, width);
        break;
    }
    control.setPosition(0, controlY);
    c.add(control);

    // faint rule line under the row.
    const rule = this.add.graphics();
    rule.lineStyle(1, PALETTE.ledgerRule, 0.7);
    rule.lineBetween(0, ROW.height - 16, width, ROW.height - 16);
    c.add(rule);

    return c;
  }

  // --- carved wooden lever (boolean / enum) -------------------------------

  private buildLever(row: LeverRow, width: number): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const trackW = Math.min(220, width);
    const trackH = 34;
    const half = trackW / 2;
    const activeIdx = Math.max(0, row.options.findIndex(o => o.value === row.value));

    const track = this.add.graphics();
    track.fillStyle(PALETTE.leverTrack, 1);
    track.fillRoundedRect(0, 0, trackW, trackH, trackH / 2);
    track.lineStyle(2, PALETTE.leverTrackEdge, 1);
    track.strokeRoundedRect(0, 0, trackW, trackH, trackH / 2);
    c.add(track);

    // the carved knob sits over the active half.
    const knobX = activeIdx === 0 ? half / 2 : half + half / 2;
    const knob = this.add.graphics();
    knob.fillStyle(PALETTE.leverKnob, 1);
    knob.fillRoundedRect(-half / 2 + 3, 3, half - 6, trackH - 6, (trackH - 6) / 2);
    knob.lineStyle(2, PALETTE.leverKnobEdge, 1);
    knob.strokeRoundedRect(-half / 2 + 3, 3, half - 6, trackH - 6, (trackH - 6) / 2);
    const knobC = this.add.container(knobX, 0);
    knobC.add(knob);
    c.add(knobC);

    // both option labels, the active one inked dark over the knob.
    row.options.forEach((opt, i) => {
      const ox = i === 0 ? half / 2 : half + half / 2;
      const t = this.add
        .text(ox, trackH / 2, opt.label, {
          fontFamily: CJK_FONT,
          fontSize: '13px',
          color: i === activeIdx ? PALETTE.leverInk : PALETTE.leverInkDim,
          fontStyle: i === activeIdx ? 'bold' : 'normal',
        })
        .setOrigin(0.5, 0.5)
        .setDepth(1);
      c.add(t);
    });

    // a click toggles to the next option (cycles for >2).
    const hit = this.add
      .zone(0, 0, trackW, trackH)
      .setOrigin(0, 0)
      .setInteractive({ useHandCursor: true });
    hit.on('pointerup', () => {
      play('open');
      const next = row.options[(activeIdx + 1) % row.options.length];
      row.onChange(next.value);
    });
    c.add(hit);
    return c;
  }

  // --- brass knob stepper (◀ n ▶) -----------------------------------------

  private buildKnob(row: KnobRow, width: number): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const wellW = 110;
    const wellH = 36;
    const arrowGap = 40;
    const centreX = Math.min(wellW / 2 + arrowGap, width / 2);

    // recessed value well.
    const well = this.add.graphics();
    well.fillStyle(PALETTE.knobWell, 1);
    well.fillRoundedRect(centreX - wellW / 2, 0, wellW, wellH, 8);
    well.lineStyle(2, PALETTE.knobWellEdge, 1);
    well.strokeRoundedRect(centreX - wellW / 2, 0, wellW, wellH, 8);
    c.add(well);

    const valueText = this.add
      .text(centreX, wellH / 2, `${row.value}${row.unit ?? ''}`, {
        fontFamily: CJK_FONT,
        fontSize: '16px',
        color: PALETTE.knobInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    c.add(valueText);

    const canDec = row.value > row.min;
    const canInc = row.value < row.max;
    const clamp = (n: number) => Math.min(row.max, Math.max(row.min, n));

    const left = this.makeArrow('◀', canDec);
    left.setPosition(centreX - wellW / 2 - arrowGap / 2, wellH / 2);
    if (canDec) {
      left.on('pointerup', () => {
        play('open');
        row.onChange(clamp(row.value - row.step));
      });
    }
    c.add(left);

    const right = this.makeArrow('▶', canInc);
    right.setPosition(centreX + wellW / 2 + arrowGap / 2, wellH / 2);
    if (canInc) {
      right.on('pointerup', () => {
        play('open');
        row.onChange(clamp(row.value + row.step));
      });
    }
    c.add(right);
    return c;
  }

  private makeArrow(glyph: string, enabled: boolean): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const t = this.add
      .text(0, 0, glyph, {
        fontFamily: CJK_FONT,
        fontSize: '20px',
        color: enabled ? PALETTE.knobInk : PALETTE.ledgerCellPlaceholder,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    c.add(t);
    c.setSize(28, 28);
    if (enabled) {
      c.setInteractive({ useHandCursor: true });
      c.on('pointerover', () => c.setScale(1.2));
      c.on('pointerout', () => c.setScale(1));
    }
    return c;
  }

  // --- click-to-edit money cell (summons IME overlay) ---------------------

  private buildMoneyCell(row: MoneyRow, width: number): Phaser.GameObjects.Container {
    const display =
      row.value == null ? row.placeholder : `${row.prefix}${row.value.toFixed(2)}`;
    const isPlaceholder = row.value == null;
    return this.buildCell(display, isPlaceholder, width, async () => {
      const result = await this.promptText(
        row.value == null ? '' : String(row.value),
        row.placeholder,
      );
      if (result === null) return;
      const trimmed = result.trim();
      if (trimmed === '') {
        row.onChange(null);
        return;
      }
      const n = Number(trimmed);
      if (Number.isFinite(n) && n >= 0) row.onChange(n);
    });
  }

  // --- click-to-edit free-text cell (summons IME overlay) -----------------

  private buildTextCell(row: TextRow, width: number): Phaser.GameObjects.Container {
    const display = row.value == null || row.value === '' ? row.placeholder : row.value;
    const isPlaceholder = row.value == null || row.value === '';
    return this.buildCell(display, isPlaceholder, width, async () => {
      const result = await this.promptText(row.value ?? '', row.placeholder);
      if (result === null) return;
      const trimmed = result.trim();
      row.onChange(trimmed === '' ? null : trimmed);
    });
  }

  // A parchment cell that reads its value and, on click, runs `onEdit`.
  private buildCell(
    display: string,
    isPlaceholder: boolean,
    width: number,
    onEdit: () => Promise<void>,
  ): Phaser.GameObjects.Container {
    const c = this.add.container(0, 0);
    const cellW = Math.min(260, width);
    const cellH = 36;
    const bg = this.add.graphics();
    bg.fillStyle(PALETTE.ledgerCell, 1);
    bg.fillRoundedRect(0, 0, cellW, cellH, 8);
    bg.lineStyle(2, PALETTE.ledgerCellEdge, 1);
    bg.strokeRoundedRect(0, 0, cellW, cellH, 8);
    c.add(bg);

    const text = this.add
      .text(12, cellH / 2, display, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: isPlaceholder ? PALETTE.ledgerCellPlaceholder : PALETTE.ledgerCellInk,
        fontStyle: isPlaceholder ? 'italic' : 'normal',
      })
      .setOrigin(0, 0.5);
    c.add(text);

    // a small ✎ affordance, right-aligned.
    const pencil = this.add
      .text(cellW - 12, cellH / 2, '✎', {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: PALETTE.ledgerInkDim,
      })
      .setOrigin(1, 0.5);
    c.add(pencil);

    const hit = this.add
      .zone(0, 0, cellW, cellH)
      .setOrigin(0, 0)
      .setInteractive({ useHandCursor: true });
    hit.on('pointerup', () => {
      void onEdit();
    });
    hit.on('pointerover', () => c.setScale(1.02));
    hit.on('pointerout', () => c.setScale(1));
    c.add(hit);
    return c;
  }

  // Summon the IME-safe React overlay centred over the book and await the result.
  private async promptText(value: string, placeholder: string): Promise<string | null> {
    if (this.cellEditing) return null;
    this.cellEditing = true;
    const { width, height } = this.scale;
    const fieldW = Math.min(360, width - 80);
    const result = await requestTextInput({
      anchor: {
        x: width / 2 - fieldW / 2,
        y: height / 2 - 40,
        width: fieldW,
        height: 56,
      },
      value,
      multiline: false,
      placeholder,
      submitLabel: STR.settingsFieldSave,
    });
    this.cellEditing = false;
    return result;
  }

  private makeButton(
    x: number,
    y: number,
    label: string,
    onClick: () => void,
  ): Phaser.GameObjects.Container {
    const c = this.add.container(x, y);
    const text = this.add
      .text(0, 0, label, {
        fontFamily: CJK_FONT,
        fontSize: '14px',
        color: PALETTE.ledgerTabInk,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5);
    const padX = 14;
    const padY = 8;
    const bw = text.width + padX * 2;
    const bh = text.height + padY * 2;
    const bg = this.add.graphics();
    bg.fillStyle(PALETTE.ledgerTab, 1);
    bg.fillRoundedRect(-bw / 2, -bh / 2, bw, bh, 9);
    bg.lineStyle(2, PALETTE.ledgerCoverEdge, 0.5);
    bg.strokeRoundedRect(-bw / 2, -bh / 2, bw, bh, 9);
    c.add([bg, text]);
    c.setSize(bw, bh);
    c.setInteractive({ useHandCursor: true });
    c.on('pointerup', onClick);
    c.on('pointerover', () => c.setScale(1.05));
    c.on('pointerout', () => c.setScale(1));
    return c;
  }
}

// --- field-row model -----------------------------------------------------

interface LeverOption {
  value: string;
  label: string;
}

interface LeverRow {
  kind: 'lever';
  label: string;
  value: string;
  options: LeverOption[];
  onChange: (value: string) => void;
}

interface KnobRow {
  kind: 'knob';
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit?: string;
  onChange: (value: number) => void;
}

interface MoneyRow {
  kind: 'cell';
  label: string;
  value: number | null;
  prefix: string;
  placeholder: string;
  onChange: (value: number | null) => void;
}

interface TextRow {
  kind: 'text';
  label: string;
  value: string | null;
  placeholder: string;
  onChange: (value: string | null) => void;
}

type FieldRow = LeverRow | KnobRow | MoneyRow | TextRow;

/** Drop `undefined`-valued keys (a `null` is meaningful — it clears a cap). */
function stripUndefined(patch: SettingsPatch): Partial<Settings> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(patch)) {
    if (v !== undefined) out[k] = v;
  }
  return out as Partial<Settings>;
}
