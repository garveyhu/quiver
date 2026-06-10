import { Fragment, type ReactNode } from 'react';

/**
 * 轻量 markdown 渲染(无依赖):AI 输出常带 ``` 代码块、`行内码`、**加粗**、# 标题、- 列表 ——
 * 原生字符串直接铺出来很难读。这里只覆盖 LLM 输出最常见的几种,让思考/产出易读:
 * 代码块用等宽背景框、行内码高亮、加粗、标题、列表项。不追求全量 markdown 规范。
 */

/** 一行内的 `行内码` 与 **加粗** → 渲染节点。 */
function inline(line: string, base: string): ReactNode[] {
  const out: ReactNode[] = [];
  // 先按 `code` 切,行内码段原样保留;非码段里再处理 **bold**。
  line.split(/(`[^`]+`)/g).forEach((part, i) => {
    if (part.length > 1 && part.startsWith('`') && part.endsWith('`')) {
      out.push(
        <code className="md-inline" key={`${base}-c${i}`}>
          {part.slice(1, -1)}
        </code>,
      );
      return;
    }
    part.split(/(\*\*[^*]+\*\*)/g).forEach((seg, j) => {
      if (seg.length > 2 && seg.startsWith('**') && seg.endsWith('**')) {
        out.push(<b key={`${base}-b${i}-${j}`}>{seg.slice(2, -2)}</b>);
      } else if (seg) {
        out.push(<Fragment key={`${base}-t${i}-${j}`}>{seg}</Fragment>);
      }
    });
  });
  return out;
}

export function MarkdownLite({ text }: { text: string }) {
  const lines = text.split('\n');
  const nodes: ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    // 代码块 ```lang … ```
    if (line.trimStart().startsWith('```')) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith('```')) {
        code.push(lines[i]);
        i++;
      }
      i++; // 跳过结束 ```
      nodes.push(
        <pre className="md-code" key={key++}>
          <code>{code.join('\n')}</code>
        </pre>,
      );
      continue;
    }

    // 标题 # / ## / ###
    const h = line.match(/^(#{1,3})\s+(.*)/);
    if (h) {
      nodes.push(
        <div className="md-h" key={key++}>
          {inline(h[2], `h${key}`)}
        </div>,
      );
      i++;
      continue;
    }

    // 列表 - / * / 1.
    const li = line.match(/^\s*(?:[-*]|\d+\.)\s+(.*)/);
    if (li) {
      nodes.push(
        <div className="md-li" key={key++}>
          <span className="md-bullet">·</span>
          <span>{inline(li[1], `l${key}`)}</span>
        </div>,
      );
      i++;
      continue;
    }

    // 空行 → 段间距
    if (!line.trim()) {
      nodes.push(<div className="md-sp" key={key++} />);
      i++;
      continue;
    }

    // 普通段落
    nodes.push(
      <div className="md-p" key={key++}>
        {inline(line, `p${key}`)}
      </div>,
    );
    i++;
  }

  return <div className="md">{nodes}</div>;
}
