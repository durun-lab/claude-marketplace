import { useEffect, useMemo, useState } from "preact/hooks";
import type { StoryHit, StoryOverview } from "@mm/story/src/api.ts";
import { formatClock } from "@mm/story/src/model.ts";
import type { Controller } from "../controller.ts";
import { useStore } from "../hooks.ts";
import { createStoryClient } from "./api.ts";
import { Reader } from "./Reader.tsx";
import { SearchBar } from "./SearchBar.tsx";
import "./story.css";
import { useSearch, useStoryData } from "./useStory.ts";

const AssetList = ({ overview, onOpen }: { overview: StoryOverview | null; onOpen: (id: string) => void }) => {
  if (!overview) return <p class="empty">Loading.</p>;
  if (overview.assets.length === 0) return <p class="empty">No clips yet.</p>;
  return (
    <ul class="st-assets">
      {overview.assets.map((a) => (
        <li key={a.id}>
          <button type="button" disabled={!a.has_transcript} onClick={() => onOpen(a.id)}>
            <span class="st-title">{a.name}</span>
            <span class="st-time">{a.has_transcript ? formatClock(a.duration) : "No transcript"}</span>
          </button>
        </li>
      ))}
    </ul>
  );
};

/** Story。素材の一覧から文字起こしを開き、範囲を選んで下の操作バーで決める。上に検索。 */
export const StoryScreen = ({ controller }: { controller: Controller }) => {
  const { connection } = controller;
  const snap = useStore(connection.subscribe, connection.get);
  const client = useMemo(() => createStoryClient(controller.api, connection.version), [controller, connection]);
  const data = useStoryData(client, snap.project?.version ?? 0);
  const [hit, setHit] = useState<StoryHit | null>(null);
  const [searchError, setSearchError] = useState<string | null>(null);
  const search = useSearch(client, setSearchError);
  useEffect(() => {
    if (hit && hit.asset !== data.assetId) data.open(hit.asset);
  }, [hit]);
  const open = (id: string | null): void => {
    setHit(null);
    data.open(id);
  };
  const openHit = (h: StoryHit): void => {
    search.clear();
    setHit(h);
  };
  return (
    <section class="screen story">
      <SearchBar search={search} onOpen={openHit} />
      {(data.error ?? searchError) && <p class="error">{data.error ?? searchError}</p>}
      {data.assetId && data.transcript ? (
        <Reader
          client={client}
          transcript={data.transcript}
          overview={data.overview}
          focusWord={hit?.asset === data.assetId ? hit.word : null}
          onBack={() => open(null)}
          settled={async () => {
            await connection.refresh();
            await data.reload();
          }}
        />
      ) : (
        <AssetList overview={data.overview} onOpen={open} />
      )}
    </section>
  );
};
