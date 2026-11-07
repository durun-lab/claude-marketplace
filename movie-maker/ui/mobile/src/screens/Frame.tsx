import type { ComponentChildren } from "preact";
import type { ProjectInfo } from "../api/types.ts";

/** 中身が 2 回目の担当で入るタブの枠。タブごとの役割の 1 行と、つながっている PC の情報だけを出す。 */
export const Frame = ({ title, role, project, children }: {
  title: string;
  role: string;
  project: ProjectInfo | null;
  children?: ComponentChildren;
}) => (
  <section class="screen frame">
    <h1>{title}</h1>
    <p class="hint">{role}</p>
    {project ? (
      <dl class="facts">
        <dt>Compositions</dt>
        <dd>{project.compositions.length}</dd>
        <dt>Sequences</dt>
        <dd>{project.sequences.length}</dd>
        <dt>Project version</dt>
        <dd>{project.version}</dd>
      </dl>
    ) : (
      <p class="empty">Waiting for the PC.</p>
    )}
    {children}
  </section>
);
