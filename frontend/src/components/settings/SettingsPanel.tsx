import { useEffect } from 'react';
import { STR } from '@/strings';
import type { Settings, SettingsPatch } from '@/types/persistence.types';
import type { SaveStatus } from '@/hooks/useSettings';
import { SettingField } from './SettingField';
import { Stepper } from './Stepper';

interface SettingsPanelProps {
  settings: Settings;
  saveStatus: SaveStatus;
  patch: (patch: SettingsPatch) => void;
  onClose: () => void;
}

const MODEL_OPTIONS = ['sonnet', 'opus', 'haiku'] as const;

/**
 * The cozy settings ledger (Phase B, v1.0 module 1): a parchment + carved-timber
 * modal organized into clearly labeled sections — 运行 / 预算 / Agent / 外观.
 * Every control has a Chinese label + hover tooltip; edits persist immediately
 * via the debounced `patch`. Closes on Esc or backdrop click.
 */
export function SettingsPanel({ settings, saveStatus, patch, onClose }: SettingsPanelProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="settings-overlay" onClick={onClose} role="presentation">
      <div
        className="settings-panel"
        role="dialog"
        aria-modal="true"
        aria-label={STR.settingsTitle}
        onClick={e => e.stopPropagation()}
      >
        <header className="settings-head">
          <div className="settings-head-text">
            <h2 className="settings-title">{STR.settingsTitle}</h2>
            <p className="settings-subtitle">{STR.settingsSubtitle}</p>
          </div>
          <SaveChip status={saveStatus} />
          <button
            type="button"
            className="settings-close"
            aria-label={STR.settingsCloseAria}
            title={STR.settingsClose}
            onClick={onClose}
          >
            ✕
          </button>
        </header>

        <div className="settings-body">
          {/* --- 运行 --- */}
          <Section title={STR.settingsSectionRun} hint={STR.settingsSectionRunHint}>
            <SettingField label={STR.settingDefaultMode} tip={STR.settingDefaultModeTip}>
              <Segmented
                ariaLabel={STR.settingDefaultMode}
                value={settings.defaultMode}
                options={[
                  { value: 'simulate', label: STR.settingDefaultModeSimulate },
                  { value: 'real', label: STR.settingDefaultModeReal },
                ]}
                onChange={v => patch({ defaultMode: v })}
              />
            </SettingField>

            <SettingField label={STR.settingModel} tip={STR.settingModelTip}>
              <select
                className="setting-select"
                aria-label={STR.settingModel}
                value={settings.model}
                onChange={e => patch({ model: e.target.value })}
              >
                {[...new Set([...MODEL_OPTIONS, settings.model])].map(m => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </SettingField>

            <SettingField label={STR.settingMaxWorkers} tip={STR.settingMaxWorkersTip}>
              <Stepper
                value={settings.maxWorkers}
                min={1}
                max={8}
                ariaLabel={STR.settingMaxWorkers}
                onChange={v => patch({ maxWorkers: v })}
              />
            </SettingField>

            <SettingField
              label={STR.settingFakeDelay}
              tip={STR.settingFakeDelayTip}
              hint={`${settings.fakeDelayMs} ms`}
            >
              <input
                className="setting-slider"
                type="range"
                min={0}
                max={600}
                step={20}
                aria-label={STR.settingFakeDelay}
                value={settings.fakeDelayMs}
                onChange={e => patch({ fakeDelayMs: Number(e.target.value) })}
              />
            </SettingField>
          </Section>

          {/* --- 预算 --- */}
          <Section title={STR.settingsSectionBudget} hint={STR.settingsSectionBudgetHint}>
            <SettingField label={STR.settingMonthlyCap} tip={STR.settingMonthlyCapTip}>
              <MoneyInput
                ariaLabel={STR.settingMonthlyCap}
                value={settings.monthlyCreditCapUsd}
                onChange={v => patch({ monthlyCreditCapUsd: v })}
              />
            </SettingField>

            <SettingField label={STR.settingNightlyBudget} tip={STR.settingNightlyBudgetTip}>
              <MoneyInput
                ariaLabel={STR.settingNightlyBudget}
                value={settings.nightlyBudgetUsd}
                onChange={v => patch({ nightlyBudgetUsd: v })}
              />
            </SettingField>
          </Section>

          {/* --- Agent --- */}
          <Section title={STR.settingsSectionAgent} hint={STR.settingsSectionAgentHint}>
            <SettingField label={STR.settingAgentBin} tip={STR.settingAgentBinTip}>
              <input
                className="setting-text"
                type="text"
                spellCheck={false}
                placeholder={STR.settingAgentBinPlaceholder}
                aria-label={STR.settingAgentBin}
                value={settings.agentBinOverride ?? ''}
                onChange={e => {
                  const v = e.target.value;
                  patch({ agentBinOverride: v.trim() === '' ? null : v });
                }}
              />
            </SettingField>
          </Section>

          {/* --- 外观 --- */}
          <Section title={STR.settingsSectionAppearance} hint={STR.settingsSectionAppearanceHint}>
            <SettingField label={STR.settingTheme} tip={STR.settingThemeTip}>
              <Segmented
                ariaLabel={STR.settingTheme}
                value={settings.theme}
                options={[
                  { value: 'cozy', label: STR.settingThemeCozy },
                  { value: 'midnight', label: STR.settingThemeMidnight },
                ]}
                onChange={v => patch({ theme: v })}
              />
            </SettingField>

            <SettingField
              label={STR.settingUiScale}
              tip={STR.settingUiScaleTip}
              hint={`${Math.round(settings.uiScale * 100)}%`}
            >
              <input
                className="setting-slider"
                type="range"
                min={0.8}
                max={1.4}
                step={0.05}
                aria-label={STR.settingUiScale}
                value={settings.uiScale}
                onChange={e => patch({ uiScale: Number(e.target.value) })}
              />
            </SettingField>
          </Section>
        </div>
      </div>
    </div>
  );
}

interface SectionProps {
  title: string;
  hint: string;
  children: React.ReactNode;
}

function Section({ title, hint, children }: SectionProps) {
  return (
    <section className="settings-section">
      <div className="settings-section-head">
        <h3 className="settings-section-title">{title}</h3>
        <span className="settings-section-hint">{hint}</span>
      </div>
      <div className="settings-section-body">{children}</div>
    </section>
  );
}

interface SegmentedOption {
  value: string;
  label: string;
}

interface SegmentedProps {
  ariaLabel: string;
  value: string;
  options: SegmentedOption[];
  onChange: (value: string) => void;
}

function Segmented({ ariaLabel, value, options, onChange }: SegmentedProps) {
  return (
    <div className="segmented" role="radiogroup" aria-label={ariaLabel}>
      {options.map(opt => (
        <button
          key={opt.value}
          type="button"
          role="radio"
          aria-checked={value === opt.value}
          className={`segmented-option${value === opt.value ? ' segmented-active' : ''}`}
          onClick={() => onChange(opt.value)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

interface MoneyInputProps {
  ariaLabel: string;
  value: number | null;
  /** `null` clears the cap (sent as JSON null = "clear"). */
  onChange: (value: number | null) => void;
}

function MoneyInput({ ariaLabel, value, onChange }: MoneyInputProps) {
  return (
    <div className="money-input">
      <span className="money-prefix">$</span>
      <input
        className="setting-text money-field"
        type="number"
        min={0}
        step={1}
        inputMode="decimal"
        placeholder={STR.settingBudgetUnset}
        aria-label={ariaLabel}
        value={value ?? ''}
        onChange={e => {
          const raw = e.target.value;
          onChange(raw === '' ? null : Number(raw));
        }}
      />
      {value != null && (
        <button
          type="button"
          className="money-clear"
          title={STR.settingBudgetClear}
          aria-label={STR.settingBudgetClear}
          onClick={() => onChange(null)}
        >
          {STR.settingBudgetClear}
        </button>
      )}
    </div>
  );
}

function SaveChip({ status }: { status: SaveStatus }) {
  if (status === 'idle') return null;
  const label =
    status === 'saving'
      ? STR.settingsSaving
      : status === 'saved'
        ? STR.settingsSaved
        : STR.settingsError;
  return <span className={`settings-savechip settings-savechip-${status}`}>{label}</span>;
}
