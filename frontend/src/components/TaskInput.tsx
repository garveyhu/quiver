import { useState } from 'react';
import { STR } from '@/strings';

interface TaskInputProps {
  /** Disable the submit regardless of input (e.g. no project picked). */
  disabled: boolean;
  /** Submit label — defaults to the "加入公告板" enqueue verb. */
  submitLabel?: string;
  /** Pin/enqueue the prompt to the board. Cleared on success. */
  onSubmit: (prompt: string) => void;
}

const DEFAULT_PROMPT: string = STR.taskPlaceholder;

/**
 * The commission writer: type a task and pin it to the bulletin board. Unlike
 * the old single-shot input it does NOT lock while a run is in flight — the
 * board runs many tasks concurrently, so the user keeps pinning more.
 */
export function TaskInput({ disabled, submitLabel = STR.enqueueTask, onSubmit }: TaskInputProps) {
  const [prompt, setPrompt] = useState<string>(DEFAULT_PROMPT);

  const canSubmit = !disabled && prompt.trim().length > 0;

  return (
    <form
      className="task-input"
      onSubmit={e => {
        e.preventDefault();
        if (canSubmit) onSubmit(prompt.trim());
      }}
    >
      <input
        type="text"
        value={prompt}
        onChange={e => setPrompt(e.target.value)}
        placeholder={STR.taskPlaceholder}
        aria-label={STR.taskInputAriaLabel}
      />
      <button type="submit" disabled={!canSubmit}>
        {submitLabel}
      </button>
    </form>
  );
}
