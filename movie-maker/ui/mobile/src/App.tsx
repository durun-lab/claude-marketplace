import { useState } from "preact/hooks";
import type { Controller } from "./controller.ts";
import { useStore } from "./hooks.ts";
import { PairScreen, codeFromSearch, guessDeviceName } from "./pairing/PairScreen.tsx";
import { ProposalsScreen } from "./proposals/ProposalsScreen.tsx";
import { Frame } from "./screens/Frame.tsx";
import { StatusChip } from "./StatusChip.tsx";
import { WatchScreen } from "./watch/WatchScreen.tsx";
import type { Attach } from "./watch/usePlayback.ts";

export const TABS = ["Watch", "Story", "Proposals"] as const;
export type Tab = (typeof TABS)[number];

const ROLES: Record<Exclude<Tab, "Proposals" | "Watch">, string> = {
  Story: "Pick, reject and reorder transcript ranges.",
};

export interface AppProps {
  /** 映像のつなぎ方。テストとデモが差し替える。 */
  attach?: Attach;
  controller: Controller;
  search?: string;
  userAgent?: string;
  initialTab?: Tab;
}

/** 下のタブ 4 つ、上の接続の表示、未登録なら登録の画面。 */
export const App = ({ controller, search = "", userAgent = "", initialTab = "Watch", attach }: AppProps) => {
  const { connection } = controller;
  const snap = useStore(connection.subscribe, connection.get);
  const pending = useStore(controller.outbox.subscribe, controller.outbox.list);
  const [tab, setTab] = useState<Tab>(initialTab);
  const body =
    snap.state === "unpaired" ? (
      <PairScreen controller={controller} initialCode={codeFromSearch(search)} initialName={guessDeviceName(userAgent)} />
    ) : tab === "Watch" ? (
      <WatchScreen controller={controller} attach={attach} />
    ) : tab === "Proposals" ? (
      <ProposalsScreen controller={controller} />
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
