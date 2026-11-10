import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import type { CommandsApi, UiCommand } from "@mm/sdk";
import { flatten, sections, type Section } from "./filter.ts";

export interface ShortcutsProps {
  commands: CommandsApi;
  /** ウィンドウを閉じる。Enter で実行した後と Esc で呼ぶ。 */
  close: () => void | Promise<void>;
}

/** 今の表。押せるかが変わると送られる新しい表に置き換える。 */
const useTable = (commands: CommandsApi): UiCommand[] => {
  const [table, setTable] = useState<UiCommand[]>([]);
  useEffect(() => {
    let off: (() => void) | undefined;
    let alive = true;
    void commands.table().then((t) => alive && setTable(t));
    void commands.onChanged(setTable).then((stop) => (alive ? (off = stop) : stop()));
    return () => {
      alive = false;
      off?.();
    };
  }, [commands]);
  return table;
};

/** ウィンドウが前に出るたびに、絞り込みの入力へキーを戻す。 */
const useFocusOnShow = (input: { current: HTMLInputElement | null }): void => {
  useEffect(() => {
    const focus = () => input.current?.focus();
    focus();
    window.addEventListener("focus", focus);
    return () => window.removeEventListener("focus", focus);
  }, [input]);
};

/** 選んだ行。選びが無い（絞り込みで消えた）ときは先頭。上下の移動は両端で止める。 */
const useSelection = (flat: readonly UiCommand[]) => {
  const [selectedId, select] = useState<string | null>(null);
  const index = Math.max(0, flat.findIndex((c) => c.id === selectedId));
  const step = (delta: number) => select(flat[Math.min(Math.max(index + delta, 0), flat.length - 1)]?.id ?? null);
  return { selected: flat[index], select, step };
};

/** 実行できたら閉じる。押せないコマンドはアプリの `run_command` が理由をつけて拒否するので、その理由を出して開いたままにする
 * （表が古くて、もう押せるようになっていれば実行される）。 */
const useRunner = (commands: CommandsApi, close: ShortcutsProps["close"]) => {
  const [error, setError] = useState<string | null>(null);
  const run = async (command: UiCommand | undefined) => {
    if (!command) return;
    const failed = await commands.run(command.id);
    if (failed) return setError(failed);
    await close();
  };
  return { error, setError, run };
};

/** 下端のキーの案内。キーは行と同じ形の枠で出す。 */
const HINTS = [
  ["↑↓", "Select"],
  ["Enter", "Run"],
  ["Esc", "Close"],
] as const;

const Footer = ({ error }: { error: string | null }) => (
  <footer class="mm-shortcuts-footer">
    {error ? (
      <span class="mm-shortcuts-error" role="alert">
        {error}
      </span>
    ) : (
      HINTS.map(([key, action]) => (
        <span key={key} class="mm-shortcuts-hint">
          <kbd class="mm-shortcuts-key">{key}</kbd> {action}
        </span>
      ))
    )}
  </footer>
);

const SearchBox = ({ input, value, onQuery }: { input: { current: HTMLInputElement | null }; value: string; onQuery: (text: string) => void }) => (
  <input
    ref={input}
    class="mm-shortcuts-search"
    type="search"
    placeholder="Search commands and keys"
    aria-label="Search commands and keys"
    value={value}
    onInput={(e) => onQuery(e.currentTarget.value)}
  />
);

/** 1 行。入/切のあるコマンドは、メニューと同じく入のときに左にチェックを出す。キーは割り当てた全部を並べる。 */
const Row = ({ command: c, selected, onRun }: { command: UiCommand; selected: boolean; onRun: (c: UiCommand) => void }) => (
  <div
    class="mm-shortcuts-row"
    role="option"
    aria-selected={selected}
    aria-disabled={!c.enabled}
    aria-checked={c.checked ?? undefined}
    title={c.description ?? undefined}
    onClick={() => onRun(c)}
  >
    <span class="mm-shortcuts-check" aria-hidden="true">
      {c.checked ? "✓" : ""}
    </span>
    <span class="mm-shortcuts-name">{c.name}</span>
    <span class="mm-shortcuts-keys">
      {c.key_labels.map((label) => (
        <kbd key={label} class="mm-shortcuts-key">
          {label}
        </kbd>
      ))}
    </span>
  </div>
);

const Rows = ({ list, selectedId, onRun }: { list: Section[]; selectedId: string | undefined; onRun: (c: UiCommand) => void }) => (
  <>
    {list.map((section) => (
      <section key={section.title} class="mm-shortcuts-section" aria-label={section.title}>
        <h2 class="mm-shortcuts-heading">{section.title}</h2>
        {section.commands.map((c) => (
          <Row key={c.id} command={c} selected={c.id === selectedId} onRun={onRun} />
        ))}
      </section>
    ))}
  </>
);

/** コマンドの表を、メニューの組ごとに名前とキーで並べて絞り込み、選んで実行する。キーの割り当ての変更はまだ無い。 */
export const Shortcuts = ({ commands, close }: ShortcutsProps) => {
  const table = useTable(commands);
  const [query, setQuery] = useState("");
  const { error, setError, run } = useRunner(commands, close);
  const input = useRef<HTMLInputElement>(null);
  const listEl = useRef<HTMLDivElement>(null);
  useFocusOnShow(input);

  const shown = useMemo(() => sections(table, query), [table, query]);
  const flat = useMemo(() => flatten(shown), [shown]);
  const { selected, select, step } = useSelection(flat);

  useEffect(() => {
    listEl.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [selected?.id]);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.isComposing) return;
    if (e.key === "Escape") return void close();
    if (e.key === "Enter") return void run(selected);
    const delta = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!delta) return;
    e.preventDefault();
    step(delta);
  };
  const onQuery = (text: string) => {
    setQuery(text);
    select(null);
    setError(null);
  };

  return (
    <div class="mm-shortcuts" onKeyDown={onKeyDown}>
      <SearchBox input={input} value={query} onQuery={onQuery} />
      <div class="mm-shortcuts-list" role="listbox" aria-label="Commands" ref={listEl}>
        <Rows
          list={shown}
          selectedId={selected?.id}
          onRun={(c) => {
            select(c.id);
            void run(c);
          }}
        />
        {flat.length === 0 && <p class="mm-shortcuts-empty">No matching commands</p>}
      </div>
      <Footer error={error} />
    </div>
  );
};
