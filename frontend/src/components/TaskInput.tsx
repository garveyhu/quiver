import { useState } from 'react';

interface TaskInputProps {
  running: boolean;
  /** Disable the Run button regardless of input (e.g. no project picked). */
  disabled: boolean;
  onRun: (prompt: string) => void;
}

const DEFAULT_PROMPT = 'Read README.md and reply with one sentence.';

export function TaskInput({ running, disabled, onRun }: TaskInputProps) {
  const [prompt, setPrompt] = useState(DEFAULT_PROMPT);

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
        placeholder="Describe a task for the agent…"
        disabled={running}
        aria-label="Task prompt"
      />
      <button type="submit" disabled={!canRun}>
        {running ? 'Running…' : 'Run task'}
      </button>
    </form>
  );
}
