import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { StoryHit, StoryOverview, StoryTranscript } from "@mm/story/src/api.ts";
import type { StoryClient } from "./api.ts";

const errorText = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export interface StoryData {
  overview: StoryOverview | null;
  assetId: string | null;
  transcript: StoryTranscript | null;
  error: string | null;
  /** 素材を開く。`null` は一覧へ戻る。 */
  open: (id: string | null) => void;
  reload: () => Promise<void>;
}

/**
 * 素材の一覧と、開いている素材の文字起こしを読む。`version`（PC のプロジェクトのバージョン）が変わるたびに読み直すので、
 * PC や Claude の変更も画面に届く。
 */
export const useStoryData = (client: StoryClient, version: number): StoryData => {
  const [overview, setOverview] = useState<StoryOverview | null>(null);
  const [assetId, setAssetId] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<StoryTranscript | null>(null);
  const [error, setError] = useState<string | null>(null);
  const current = useRef<string | null>(null);
  current.current = assetId;

  const reload = useCallback(async () => {
    try {
      setOverview(await client.overview());
      const id = current.current;
      setTranscript(id ? await client.transcript(id) : null);
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, [client]);
  useEffect(() => void reload(), [reload, version]);

  const open = (id: string | null): void => {
    setAssetId(id);
    setTranscript(null);
    if (id) client.transcript(id).then(setTranscript, (e: unknown) => setError(errorText(e)));
  };
  return { overview, assetId, transcript, error, open, reload };
};

export interface SearchState {
  hits: StoryHit[] | null;
  run: (query: string) => Promise<void>;
  clear: () => void;
}

/** 全文検索。結果は閉じるまで残す。 */
export const useSearch = (client: StoryClient, onError: (message: string) => void): SearchState => {
  const [hits, setHits] = useState<StoryHit[] | null>(null);
  const run = async (query: string): Promise<void> => {
    if (query.trim() === "") return setHits(null);
    try {
      setHits(await client.search(query));
    } catch (e) {
      onError(errorText(e));
    }
  };
  return { hits, run, clear: () => setHits(null) };
};
