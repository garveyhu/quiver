import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { EventBus, BUS, type TextInputRequest } from '@/game/EventBus';
import { STR } from '@/strings';

/**
 * The IME-safe text-input overlay — the single keyboard seam every world scene
 * reuses (§0 裁决 option-b: you NEVER type inside the canvas).
 *
 * A scene that needs typed input emits `BUS.textInput` with a {@link TextInputRequest}
 * carrying an anchor rect (canvas pixels), an initial value, single/multi-line,
 * a placeholder, and a one-shot reply `channel`. This component mounts a real DOM
 * `<textarea>`/`<input>` positioned over that rect — so WKWebView's native CJK
 * IME + 拼音候选条 come for free — and on commit/cancel emits ONCE on the reply
 * channel with the resolved string (or `null` if cancelled), then unmounts.
 *
 * IME gate (the §6 first-priority risk). Commit is gated on the composition
 * state, so selecting a 拼音 candidate with Enter is NEVER mistaken for submit:
 *  - `compositionstart`/`compositionend` track whether the IME is mid-candidate;
 *  - on Enter we also re-check `e.nativeEvent.isComposing` (the authoritative,
 *    per-keystroke flag) before committing;
 *  - multi-line: Enter = newline, ⌘/Ctrl+Enter = submit; single-line: Enter =
 *    submit, but always exempt while composing.
 */
export function TextInputOverlay() {
  const [request, setRequest] = useState<TextInputRequest | null>(null);
  const [value, setValue] = useState('');

  // Mid-IME-composition flag. Kept in a ref (not state) so the keydown handler
  // reads the live value synchronously within the same event burst.
  const composingRef = useRef(false);
  const fieldRef = useRef<HTMLTextAreaElement | HTMLInputElement | null>(null);

  // A request asks for input: stash it, seed the field, and (once mounted) focus.
  useEffect(() => {
    const onRequest = (req: TextInputRequest) => {
      composingRef.current = false;
      setValue(req.value);
      setRequest(req);
    };
    EventBus.on(BUS.textInput, onRequest);
    return () => {
      EventBus.off(BUS.textInput, onRequest);
    };
  }, []);

  // Focus the freshly-mounted field + select its seed text, so the caret is live
  // without a stray click (autoFocus is unreliable across remounts).
  useLayoutEffect(() => {
    if (!request) return;
    const el = fieldRef.current;
    if (!el) return;
    el.focus();
    el.select();
  }, [request]);

  // Reply exactly once on the request's channel, then tear the overlay down.
  const resolve = useCallback(
    (result: string | null) => {
      if (!request) return;
      EventBus.emit(request.channel, result);
      setRequest(null);
      setValue('');
      composingRef.current = false;
    },
    [request],
  );

  const submit = useCallback(() => {
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      resolve(null);
      return;
    }
    resolve(trimmed);
  }, [value, resolve]);

  const cancel = useCallback(() => resolve(null), [resolve]);

  // Reposition the overlay if the canvas resizes while it's open: the scene
  // re-emits the request with a fresh anchor (see TextInputAnchorBridge below),
  // but Escape-to-cancel + outside-click are handled here regardless.
  useEffect(() => {
    if (!request) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cancel();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [request, cancel]);

  if (!request) return null;

  const { anchor, multiline, placeholder } = request;

  const onFieldKeyDown = (
    e: React.KeyboardEvent<HTMLTextAreaElement | HTMLInputElement>,
  ): void => {
    // IME gate: the per-keystroke truth. While the 拼音 candidate list is open,
    // `isComposing` is true and Enter belongs to the IME, never to us.
    if (e.key === 'Enter') {
      const composing = e.nativeEvent.isComposing || composingRef.current;
      if (composing) return;
      if (multiline) {
        // multi-line: Enter is a newline; only ⌘/Ctrl+Enter commits.
        if (e.metaKey || e.ctrlKey) {
          e.preventDefault();
          submit();
        }
        return;
      }
      // single-line: Enter commits (composition already excluded above).
      e.preventDefault();
      submit();
    }
  };

  const fieldStyle: React.CSSProperties = {
    left: anchor.x,
    top: anchor.y,
    width: anchor.width,
    height: anchor.height,
  };

  return (
    <div className="text-overlay" role="dialog" aria-modal="true">
      <div className="text-overlay-scrim" onClick={cancel} role="presentation" />
      <div className="text-overlay-field" style={fieldStyle}>
        {multiline ? (
          <textarea
            ref={fieldRef as React.RefObject<HTMLTextAreaElement>}
            className="text-overlay-input"
            value={value}
            placeholder={placeholder}
            aria-label={placeholder}
            onChange={e => setValue(e.target.value)}
            onCompositionStart={() => {
              composingRef.current = true;
            }}
            onCompositionEnd={() => {
              composingRef.current = false;
            }}
            onKeyDown={onFieldKeyDown}
          />
        ) : (
          <input
            ref={fieldRef as React.RefObject<HTMLInputElement>}
            type="text"
            className="text-overlay-input"
            value={value}
            placeholder={placeholder}
            aria-label={placeholder}
            onChange={e => setValue(e.target.value)}
            onCompositionStart={() => {
              composingRef.current = true;
            }}
            onCompositionEnd={() => {
              composingRef.current = false;
            }}
            onKeyDown={onFieldKeyDown}
          />
        )}
        <div className="text-overlay-actions">
          <button type="button" className="text-overlay-cancel" onClick={cancel}>
            {STR.textInputCancel}
          </button>
          <button
            type="button"
            className="text-overlay-submit"
            disabled={value.trim().length === 0}
            onClick={submit}
          >
            {request.submitLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
