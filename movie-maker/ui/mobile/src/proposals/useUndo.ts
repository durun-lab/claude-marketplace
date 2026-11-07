import { useCallback, useEffect, useState } from "preact/hooks";

/** 決めた後に `Undo` を出しておく時間。 */
export const UNDO_WINDOW_MS = 5000;

export interface UndoOffer {
  /** 画面に出す文言。 */
  label: string;
  run: () => Promise<void>;
}

/** 決めた直後だけ出す `Undo`。時間が過ぎるか、押すか、次の提案を決めると消える。 */
export const useUndo = () => {
  const [offer, setOffer] = useState<UndoOffer | null>(null);
  useEffect(() => {
    if (!offer) return undefined;
    const timer = setTimeout(() => setOffer(null), UNDO_WINDOW_MS);
    return () => clearTimeout(timer);
  }, [offer]);
  const dismiss = useCallback(() => setOffer(null), []);
  return { offer, show: setOffer, dismiss };
};
