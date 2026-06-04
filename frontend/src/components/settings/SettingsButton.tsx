import { STR } from '@/strings';

interface SettingsButtonProps {
  onClick: () => void;
}

/**
 * The settings entry in the cozy HUD: a carved-timber control plate mounted on
 * the wall, with an engraved gear glyph + "设置" label. Styled like a workshop
 * fixture, not a plain web button. Pinned top-right of the app shell.
 */
export function SettingsButton({ onClick }: SettingsButtonProps) {
  return (
    <button
      type="button"
      className="settings-fixture"
      title={STR.settingsOpenTitle}
      aria-label={STR.settingsOpenTitle}
      onClick={onClick}
    >
      <span className="settings-fixture-gear" aria-hidden>
        ⚙
      </span>
      <span className="settings-fixture-label">{STR.settingsOpen}</span>
    </button>
  );
}
