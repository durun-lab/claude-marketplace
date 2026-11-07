import { useEffect, useState } from "preact/hooks";
import type { Op, Rating, Topic } from "@mm/sdk";
import type { StoryTranscript, StoryWord, WordRange } from "@mm/story/src/api.ts";
import { wordsIn, type Span } from "@mm/story/src/model.ts";
import { addTopicOp, hideOp, insertTopicsOp } from "@mm/story/src/ops.ts";
import type { StoryClient } from "./api.ts";
import { RATING_LABELS, titleOf, topicsOver } from "./model.ts";

const UNDO_WINDOW_MS = 5000;
const TITLE_MAX_CHARS = 24;

/** 画面の下に出す 1 行の知らせ。`undo` があれば取り消しのボタンを添える。 */
export interface Notice {
  text: string;
  tone: "ok" | "warn";
  undo: boolean;
}

export interface Actions {
  notice: Notice | null;
  dismiss: () => void;
  /** 選んだ範囲（なければ重なる話題）の評価を変える。話題が無ければ範囲を話題にして付ける。 */
  rate: (rating: Rating) => Promise<void>;
  makeTopic: () => Promise<void>;
  /** 選んだ単語を表示と検索から消す（全部消してあれば戻す）。 */
  deleteWords: () => Promise<void>;
  /** 話題を並べた順に、開いているシーケンスの末尾へ差し込む。 */
  arrange: (topics: readonly Topic[]) => Promise<void>;
  undo: () => Promise<void>;
  /** 知らせを出す。 */
  warn: (text: string) => void;
}

export interface ActionContext {
  client: StoryClient;
  transcript: StoryTranscript | null;
  span: Span | null;
  /** 並べる先のシーケンス。無ければ並べられない。 */
  sequence: string | null;
  onPc: boolean;
  /** 送った後に画面を読み直す。 */
  settled: () => Promise<void>;
  clearPick: () => void;
}

const selectedWords = (cx: ActionContext): StoryWord[] => (cx.transcript ? wordsIn(cx.transcript.words, cx.span) : []);

/** 操作を送り、結果を知らせにする。衝突のときは操作を捨てて読み直す（勝手に再送しない）。 */
const makeSender = (cx: ActionContext, setNotice: (n: Notice | null) => void) => {
  return async (ops: Op[], name: string): Promise<void> => {
    if (ops.length === 0) return;
    const result = await cx.client.send(ops, { name, onPc: cx.onPc });
    if (result.kind === "conflict") {
      setNotice({ text: "The project changed on the PC. Reloaded, so pick again.", tone: "warn", undo: false });
      cx.clearPick();
    } else if (result.kind === "failed") {
      setNotice({ text: result.message, tone: "warn", undo: false });
    } else {
      const proposed = result.kind === "proposed";
      setNotice({ text: proposed ? "Sent to the PC as a proposal." : `${name} done.`, tone: "ok", undo: !proposed });
      cx.clearPick();
    }
    await cx.settled();
  };
};

/** 範囲の時刻を PC に聞いて操作を組む。選んでいなければ何もしない。 */
const withRange = async (cx: ActionContext, build: (asset: string, range: WordRange) => Op[]): Promise<Op[]> => {
  const asset = cx.transcript?.asset;
  const words = selectedWords(cx);
  const first = words[0];
  const last = words.at(-1);
  if (!asset || !first || !last) return [];
  return build(asset, await cx.client.range(asset, first.id, last.id));
};

const setRating = (target: string, rating: Rating): Op => ({ op: "rate", target, rating });

/** 知らせを持ち、成功の知らせは取り消せる間（5 秒）だけ出す。 */
const useNotice = (): [Notice | null, (n: Notice | null) => void] => {
  const [notice, setNotice] = useState<Notice | null>(null);
  useEffect(() => {
    if (notice?.tone !== "ok") return;
    const timer = setTimeout(() => setNotice(null), UNDO_WINDOW_MS);
    return () => clearTimeout(timer);
  }, [notice]);
  return [notice, setNotice];
};

/** 評価の操作。範囲に話題が重なればその話題の評価を、無ければ範囲を話題にして評価を付ける。 */
const rateOps = async (cx: ActionContext, rating: Rating): Promise<Op[]> => {
  const over = cx.transcript ? topicsOver(cx.transcript.words, cx.transcript.topics, cx.span) : [];
  if (over.length > 0) return over.map((t) => setRating(t.id, rating));
  return withRange(cx, (asset, range) => {
    const add = addTopicOp(asset, titleOf(selectedWords(cx), TITLE_MAX_CHARS), range);
    return add.op === "addTopic" ? [add, setRating(add.topic.id, rating)] : [];
  });
};

const topicOps = (cx: ActionContext): Promise<Op[]> =>
  withRange(cx, (asset, range) => [addTopicOp(asset, titleOf(selectedWords(cx), TITLE_MAX_CHARS), range)]);

/** Story の操作を、組み立てと送信と知らせにつなぐ。 */
export const useActions = (cx: ActionContext): Actions => {
  const [notice, setNotice] = useNotice();
  const warn = (text: string): void => setNotice({ text, tone: "warn", undo: false });
  const send = makeSender(cx, setNotice);
  const guard = async (run: () => Promise<void>): Promise<void> => {
    try {
      await run();
    } catch (e) {
      warn(e instanceof Error ? e.message : String(e));
    }
  };
  return {
    notice,
    warn,
    dismiss: () => setNotice(null),
    rate: (rating) => guard(async () => send(await rateOps(cx, rating), RATING_LABELS[rating])),
    makeTopic: () => guard(async () => send(await topicOps(cx), "Topic")),
    deleteWords: () =>
      guard(async () => {
        const asset = cx.transcript?.asset;
        const op = asset ? hideOp(asset, selectedWords(cx)) : null;
        await send(op ? [op] : [], "Delete");
      }),
    arrange: (topics) =>
      guard(async () => {
        if (!cx.sequence) return warn("No sequence is open on the PC.");
        await send(topics.length > 0 ? [insertTopicsOp(cx.sequence, topics)] : [], "Arrange");
      }),
    undo: () =>
      guard(async () => {
        const failure = await cx.client.undo();
        if (failure) warn(failure);
        else setNotice(null);
        await cx.settled();
      }),
  };
};
