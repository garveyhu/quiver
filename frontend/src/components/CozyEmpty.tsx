interface CozyEmptyProps {
  /** A single emoji/glyph anchor for the placard. */
  glyph: string;
  title: string;
  hint?: string;
}

/**
 * A friendly parchment empty-state placard: glyph + headline + soft hint.
 * Shared by the workshop, board, and first-run prompt so a "nothing here yet"
 * panel reads as cozy and intentional rather than blank. Presentation only.
 */
export function CozyEmpty({ glyph, title, hint }: CozyEmptyProps) {
  return (
    <div className="cozy-empty">
      <span className="cozy-empty-glyph" aria-hidden>
        {glyph}
      </span>
      <p className="cozy-empty-title">{title}</p>
      {hint && <p className="cozy-empty-hint">{hint}</p>}
    </div>
  );
}
