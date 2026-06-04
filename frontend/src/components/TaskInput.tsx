import { useState } from 'react';

interface TaskInputProps {
  running: boolean;
  onRun: (prompt: string) => void;
}

const DEFAULT_PROMPT = 'Read README.md and reply with one sentence.';

export function TaskInput({ running, onRun }: TaskInputProps) {
  const [prompt, setPrompt] = useState(DEFAULT_PROMPT);

  return (
    <form
      className="task-input"
      onSubmit={e => {
        e.preventDefault();
        if (!running && prompt.trim()) onRun(prompt.trim());
      }}
    >
      <input
        type="text"
        value={prompt}
        onChange={e => setPrompt(e.target.value)}
        placeholder="Describe a task for the fake-claude worker…"
        disabled={running}
        aria-label="Task prompt"
      />
      <button type="submit" disabled={running || !prompt.trim()}>
        {running ? 'Running…' : 'Run task'}
      </button>
    </form>
  );
}
