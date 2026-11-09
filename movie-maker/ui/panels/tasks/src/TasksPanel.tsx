import { useState } from "preact/hooks";
import { commandTitle, type UiCommand } from "@mm/sdk";
import { DEFAULT_GROUPS, type TaskGroup, type TaskItem } from "./groups.ts";

/** 項目を押したときに呼ぶコールバック。コマンドの ID を渡す。 */
export type OnInvoke = (command: string) => void;

export interface TasksPanelProps {
  groups?: TaskGroup[];
  onInvoke: OnInvoke;
  /** UI のコマンドの表。項目を押せるかとツールチップの文を決める。アプリから読めるまでは null。 */
  commands?: readonly UiCommand[] | null;
  /** 最初から閉じておくグループの id。 */
  initiallyClosed?: readonly string[];
}

type Commands = TasksPanelProps["commands"];

/** 表が読めるまでは押せる見た目のまま。読めたら、表に無いものと押せないものを薄くする。 */
const itemView = (command: string, commands: Commands): { off: boolean; title: string | undefined } => {
  if (!commands) return { off: false, title: undefined };
  const found = commands.find((c) => c.id === command);
  return { off: !found?.enabled, title: commandTitle(found) };
};

/** 項目 1 つ。薄い項目もツールチップとフォーカスで理由が分かるよう、disabled ではなく aria-disabled にする。 */
const ItemLink = ({ item, commands, onInvoke }: { item: TaskItem; commands: Commands; onInvoke: OnInvoke }) => {
  const { off, title } = itemView(item.command, commands);
  return (
    <button
      type="button"
      class="mm-tasks-link"
      title={title}
      aria-disabled={off ? "true" : undefined}
      onClick={off ? undefined : () => onInvoke(item.command)}
    >
      <span class="mm-tasks-bullet" aria-hidden="true" />
      {item.label}
    </button>
  );
};

interface GroupViewProps {
  group: TaskGroup;
  open: boolean;
  onToggle: () => void;
  onInvoke: OnInvoke;
  commands: Commands;
}

/** 見出しと、開いているときの項目の並び。 */
const GroupView = ({ group, open, onToggle, onInvoke, commands }: GroupViewProps) => {
  const bodyId = `mm-tasks-${group.id}`;
  return (
    <section class="mm-tasks-group" data-open={open}>
      <h2 class="mm-tasks-title">
        <button
          type="button"
          class="mm-tasks-toggle"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={onToggle}
        >
          <span class="mm-tasks-chevron" aria-hidden="true" />
          {group.title}
        </button>
      </h2>
      {open && (
        <ul class="mm-tasks-body" id={bodyId}>
          {group.items.map((item) => (
            <li key={item.id}>
              <ItemLink item={item} commands={commands} onInvoke={onInvoke} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
};

/** XP のムービーメーカー風の作業ウィンドウ。グループごとに開け閉めできる。 */
export const TasksPanel = ({
  groups = DEFAULT_GROUPS,
  onInvoke,
  commands,
  initiallyClosed = [],
}: TasksPanelProps) => {
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set(initiallyClosed));

  const toggle = (id: string) => {
    const next = new Set(closed);
    if (!next.delete(id)) next.add(id);
    setClosed(next);
  };

  return (
    <nav class="mm-tasks" aria-label="Movie Tasks">
      {groups.map((group) => (
        <GroupView
          key={group.id}
          group={group}
          open={!closed.has(group.id)}
          onToggle={() => toggle(group.id)}
          onInvoke={onInvoke}
          commands={commands}
        />
      ))}
    </nav>
  );
};
