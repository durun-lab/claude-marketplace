import { useState } from "preact/hooks";
import type { Controller } from "./controller.ts";
import { useStore } from "./hooks.ts";
import { PairScreen, codeFromSearch, guessDeviceName } from "./pairing/PairScreen.tsx";
import { CommentsScreen } from "./screens/CommentsScreen.tsx";
import { Frame } from "./screens/Frame.tsx";
import { StatusChip } from "./StatusChip.tsx";

export const TABS = ["Watch", "Story", "Comments", "Proposals"] as const;
export type Tab = (typeof TABS)[number];

const ROLES: Record<Exclude<Tab, "Comments">, string> = {
  Watch: "Low-bitrate playback and comments pinned to a time.",
  Story: "Pick, reject and reorder transcript ranges.",
  Proposals: "Compare before and after, then approve or reject.",
};

export interface AppProps {
  controller: Controller;
  search?: string;
  userAgent?: string;
  initialTab?: Tab;
}

/** 下のタブ 4 つ、上の接続の表示、未登録なら登録の画面。 */
export const App = ({ controller, search = "", userAgent = "", initialTab = "Comments" }: AppProps) => {
  const { connection } = controller;
  const snap = useStore(connection.subscribe, connection.get);
  const pending = useStore(controller.outbox.subscribe, controller.outbox.list);
  const [tab, setTab] = useState<Tab>(initialTab);
  const body =
    snap.state === "unpaired" ? (
      <PairScreen controller={controller} initialCode={codeFromSearch(search)} initialName={guessDeviceName(userAgent)} />
    ) : tab === "Comments" ? (
      <CommentsScreen controller={controller} />
    ) : (
      <Frame title={tab} role={ROLES[tab]} project={snap.project} />
    );
  return (
    <div class="app">
      <header class="bar">
        <span class="brand">Movie Maker</span>
        <StatusChip state={snap.state} pending={pending.length} />
      </header>
      <main>{body}</main>
      {snap.state !== "unpaired" && (
        <nav class="tabs" role="tablist">
          {TABS.map((t) => (
            <button key={t} type="button" role="tab" aria-selected={t === tab} class={t === tab ? "on" : ""} onClick={() => setTab(t)}>
              {t}
            </button>
          ))}
        </nav>
      )}
    </div>
  );
};
