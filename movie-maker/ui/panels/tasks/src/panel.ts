import { h } from "preact";
import { definePanel } from "@mm/sdk";
import { TasksPanel } from "./TasksPanel.tsx";

/** Tasks の項目は UI のコマンドの表の ID を指す。押せるかとツールチップは表から決め、実行は Shell に任せる。 */
export const panel = definePanel({
  id: "tasks",
  label: "Tasks",
  column: "left",
  order: 10,
  render: ({ runCommand, commands }) => h(TasksPanel, { onInvoke: runCommand, commands }),
});
