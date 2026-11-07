import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { FeedbackApi, FeedbackItem, ImproveStatus, ListFilter, NewFeedback, SettingsPatch } from "./api.ts";

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** 状態と一覧を読み、`refreshKey` の変化とアプリの `mm:feedback-changed` で読み直す。最後に頼んだ読み直しだけを使う。 */
const useLoad = (api: FeedbackApi, filter: ListFilter, refreshKey: number, setError: (m: string | null) => void) => {
  const [status, setStatus] = useState<ImproveStatus | null>(null);
  const [items, setItems] = useState<FeedbackItem[] | null>(null);
  const loads = useRef(0);
  const reload = useCallback(async () => {
    const id = ++loads.current;
    try {
      const [s, list] = await Promise.all([api.status(), api.list(filter)]);
      if (id !== loads.current) return;
      setStatus(s);
      setItems(list);
      setError(null);
    } catch (e) {
      if (id === loads.current) setError(message(e));
    }
  }, [api, filter, setError]);
  useEffect(() => {
    void reload();
  }, [reload, refreshKey]);
  useEffect(() => {
    let off: (() => void) | null = null;
    let alive = true;
    api.onChanged(() => void reload()).then(
      (f) => {
        if (alive) off = f;
        else f();
      },
      () => undefined,
    );
    return () => {
      alive = false;
      off?.();
    };
  }, [api, reload]);
  return { status, setStatus, items, reload };
};

/** Feedback パネルの状態と操作。 */
export const useFeedback = (api: FeedbackApi, refreshKey: number) => {
  const [filter, setFilter] = useState<ListFilter>("all");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useLoad(api, filter, refreshKey, setError);
  const act = useCallback(
    async <T>(f: () => Promise<T>): Promise<T | null> => {
      setBusy(true);
      try {
        return await f();
      } catch (e) {
        setError(message(e));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [],
  );
  const submit = useCallback(
    async (feedback: NewFeedback): Promise<boolean> => {
      const item = await act(() => api.submit(feedback));
      await load.reload();
      return item !== null;
    },
    [act, api, load.reload],
  );
  const applyStatus = useCallback(
    async (f: () => Promise<ImproveStatus>) => {
      const s = await act(f);
      if (s) load.setStatus(s);
    },
    [act, load.setStatus],
  );
  const settings = useCallback((patch: SettingsPatch) => applyStatus(() => api.settings(patch)), [api, applyStatus]);
  const stop = useCallback(() => applyStatus(() => api.stop()), [api, applyStatus]);
  return { ...load, filter, setFilter, error, busy, submit, settings, stop };
};

export type FeedbackModel = ReturnType<typeof useFeedback>;
