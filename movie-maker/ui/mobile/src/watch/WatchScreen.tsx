import { useState } from "preact/hooks";
import type { Controller } from "../controller.ts";
import { useStore } from "../hooks.ts";
import { CommentsPanel } from "./CommentsPanel.tsx";
import { ComposeSheet } from "./ComposeSheet.tsx";
import { DEFAULT_FILTERS, type Filter } from "./commentsView.ts";
import { streamUrl } from "./commentsApi.ts";
import { Scrubber } from "./Scrubber.tsx";
import { Stage } from "./Stage.tsx";
import { ThreadSheet } from "./ThreadSheet.tsx";
import { commentStart } from "./time.ts";
import { useComments } from "./useComments.ts";
import { useComposer } from "./useComposer.ts";
import { usePlayback, type Attach } from "./usePlayback.ts";
import "./watch.css";

export interface WatchScreenProps {
  controller: Controller;
  /** 映像のつなぎ方。テストが差し替える。 */
  attach?: Attach;
}

const toggleFilter = (now: readonly Filter[], f: Filter): Filter[] =>
  now.includes(f) ? now.filter((x) => x !== f) : [...now, f];

/** Watch のタブ。上に映像とスクラブバー、下にコメントの一覧、入力欄、スレッド。 */
export const WatchScreen = ({ controller, attach }: WatchScreenProps) => {
  const project = useStore(controller.connection.subscribe, controller.connection.get).project;
  const comp = project?.open_composition ?? project?.compositions[0]?.id ?? "";
  const play = usePlayback(comp ? streamUrl(comp) : "", attach);
  const comments = useComments(controller);
  const composer = useComposer(controller, { comp, time: play.time });
  const [filters, setFilters] = useState<readonly Filter[]>(DEFAULT_FILTERS);
  const [openId, setOpenId] = useState<string | null>(null);
  const opened = comments.items.find((c) => c.id === openId) ?? null;
  const open = (id: string, at: number): void => {
    play.pause();
    play.seek(at);
    setOpenId(id);
  };
  const shapes = composer.composing ? composer.state.shapes : (opened?.drawing ?? []);
  return (
    <section class="screen watch">
      <Stage videoRef={play.ref} playing={play.playing} failed={play.failed} shapes={shapes} editing={composer.editing} onToggle={play.toggle} />
      <Scrubber time={play.time} duration={play.duration} cuts={project?.cuts ?? []} comments={comments.items.map(commentStart)} onSeek={play.seek} />
      {composer.composing && (
        <ComposeSheet state={composer.state} time={play.time} canSend={composer.draft !== null} dispatch={composer.dispatch} onCancel={composer.close} onSend={composer.send} />
      )}
      {!composer.composing && opened && (
        <ThreadSheet
          comment={opened}
          error={comments.error}
          onClose={() => setOpenId(null)}
          onReply={(text) => comments.reply(opened.id, text)}
          onState={(to) => comments.setState(opened.id, to)}
        />
      )}
      {!composer.composing && !opened && (
        <CommentsPanel
          comments={comments}
          controller={controller}
          filters={filters}
          openId={openId}
          onToggle={(f) => setFilters((now) => toggleFilter(now, f))}
          onOpen={open}
          canCompose={comp !== ""}
          onCompose={() => {
            play.pause();
            composer.open();
          }}
        />
      )}
    </section>
  );
};
