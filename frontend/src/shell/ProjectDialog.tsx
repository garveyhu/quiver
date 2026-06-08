import { useState } from 'react';
import { Dialog, Button, ProjectCard, TextField, PALETTE } from '@/assets';
import { useProjects } from '@/hooks/useProjects';

export interface ProjectDialogProps {
  open: boolean;
  onClose: () => void;
  projectPath: string | null;
  onPick: () => Promise<void>;
  onSelect: (path: string) => Promise<void>;
}

/** 项目管理:列出最近项目,可切换 / 添加 / 改名 / 移除。 */
export function ProjectDialog({ open, onClose, projectPath, onPick, onSelect }: ProjectDialogProps) {
  const { recent, remove, rename } = useProjects();
  const [editing, setEditing] = useState<string | null>(null);
  const [alias, setAlias] = useState('');

  if (!open) return null;

  const commit = (path: string) => {
    void rename(path, alias.trim() || null);
    setEditing(null);
  };

  return (
    <Dialog
      open={open}
      title="项目 ▰"
      onClose={onClose}
      width={460}
      actions={<Button onClick={() => void onPick().then(onClose)}>＋ 添加项目</Button>}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {recent.length === 0 && <div style={{ color: PALETTE.dim, fontSize: 11 }}>还没有最近项目</div>}
        {recent.map((r) => (
          <div key={r.path} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            {editing === r.path ? (
              <TextField
                autoFocus
                value={alias}
                placeholder={r.path.split('/').pop() ?? r.path}
                onChange={(e) => setAlias(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') commit(r.path);
                  if (e.key === 'Escape') setEditing(null);
                }}
                style={{ flex: 1 }}
              />
            ) : (
              <ProjectCard
                name={(r.alias ?? r.path.split('/').pop()) || r.path}
                path={r.path}
                meta={r.path === projectPath ? '● 当前工坊' : undefined}
                onClick={() => void onSelect(r.path).then(onClose)}
                style={{ flex: 1 }}
              />
            )}
            <Button
              variant="ghost"
              title="改名"
              onClick={() => {
                if (editing === r.path) commit(r.path);
                else {
                  setEditing(r.path);
                  setAlias(r.alias ?? '');
                }
              }}
            >
              {editing === r.path ? '✓' : '✎'}
            </Button>
            <Button variant="ghost" onClick={() => void remove(r.path)}>
              移除
            </Button>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
