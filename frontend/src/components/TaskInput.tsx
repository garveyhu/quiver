import { useState } from 'react';
import { STR } from '@/strings';

interface TaskInputProps {
  running: boolean;
  /** Disable the Run button regardless of input (e.g. no project picked). */
  disabled: boolean;
  onRun: (prompt: string) => void;
}

const DEFAULT_PROMPT: string = STR.taskPlaceholder;

export function TaskInput({ running, disabled, onRun }: TaskInputProps) {
  const [prompt, setPrompt] = useState<string>(DEFAULT_PROMPT);

  const canRun = !running && !disabled && prompt.trim().length > 0;

  return (
    <form
      className="task-input"
      onSubmit={e => {
        e.preventDefault();
        if (canRun) onRun(prompt.trim());
      }}
    >
      <input
        type="text"
        value={prompt}
        onChange={e => setPrompt(e.target.value)}
        placeholder={STR.taskPlaceholder}
        disabled={running}
        aria-label={STR.taskInputAriaLabel}
      />
      <button type="submit" disabled={!canRun}>
        {running ? STR.running : STR.runTask}
      </button>
    </form>
  );
}
