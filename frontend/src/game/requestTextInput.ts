import { EventBus, BUS, type TextInputAnchor, type TextInputRequest } from '@/game/EventBus';

let channelSeq = 0;

export interface RequestTextInputOptions {
  /** Where on the canvas the field should appear (page/canvas pixels). */
  anchor: TextInputAnchor;
  /** Initial value (e.g. an empty string for a new commission). */
  value?: string;
  multiline?: boolean;
  placeholder?: string;
  /** Submit button label (Chinese). */
  submitLabel: string;
}

/**
 * Summon the IME-safe React text overlay from a Phaser scene and await its
 * result (§0 option-b). Resolves with the committed, trimmed string, or `null`
 * if the user cancelled (Escape / outside-click / empty submit).
 *
 * Each call mints a fresh one-shot reply channel so concurrent requests can't
 * cross wires, and always tears its listener down on resolve — no leak.
 */
export function requestTextInput(opts: RequestTextInputOptions): Promise<string | null> {
  const channel = `text-input:reply:${++channelSeq}`;
  const request: TextInputRequest = {
    channel,
    anchor: opts.anchor,
    value: opts.value ?? '',
    multiline: opts.multiline ?? false,
    placeholder: opts.placeholder ?? '',
    submitLabel: opts.submitLabel,
  };

  return new Promise(resolve => {
    const onReply = (result: string | null) => {
      EventBus.off(channel, onReply);
      resolve(result);
    };
    EventBus.once(channel, onReply);
    EventBus.emit(BUS.textInput, request);
  });
}
