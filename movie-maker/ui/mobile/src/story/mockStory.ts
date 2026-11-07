import type { Op, RationalTime, Topic } from "@mm/sdk";
import type { StoryHit, StoryOverview, StoryTranscript, StoryWord } from "@mm/story/src/api.ts";

const HTTP_OK = 200;
const HTTP_BAD_REQUEST = 400;
const HTTP_CONFLICT = 409;
const WORD_MS = 500;
const MS = 1000;
/** 偽の文字起こしの話題の範囲（単語の番号、終わりは含まない）。 */
const HARBOR_START = 0;
const HARBOR_END = 23;
const FERRY_START = 34;
const FERRY_END = 46;
const CONTEXT_WORDS = 6;

const SENTENCES = [
  "So today I want to talk about how we found the old harbor.",
  "It was raining all morning and honestly we almost stayed home.",
  "Then the clouds opened and the whole bay turned gold.",
  "Anyway, here is the part about the ferry schedule nobody asked for.",
  "The last boat leaves at six, so plan your day around it.",
];

const at = (i: number): RationalTime => ({ num: i * WORD_MS, den: MS });

const buildWords = (): StoryWord[] => {
  const words: StoryWord[] = [];
  SENTENCES.forEach((sentence, segment) => {
    for (const text of sentence.split(" ")) {
      const i = words.length;
      words.push({ id: `w${i}`, segment, start: at(i), end: at(i + 1), text, fixed: null, deleted: false });
    }
  });
  return words;
};

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

const fail = (status: number, code: string, message: string): Response => json(status, { error: { code, message } });

const wordIndex = (words: StoryWord[], id: string): number => words.findIndex((w) => w.id === id);

export interface MockRequest {
  path: string;
  method: string;
  body: unknown;
}

interface Txn {
  name: string;
  base_version: number;
  ops: Op[];
}

export interface MockStory {
  /** Story の要求なら応答を返し、そうでなければ `null`。 */
  handle: (req: MockRequest) => Response | null;
  /** 次の操作を 409 `conflict` で断る（版が古いときの画面を試す）。 */
  conflictNext: () => void;
  topics: Topic[];
  words: StoryWord[];
  /** 提案として受けた操作の列。 */
  proposals: Txn[];
  /** 適用した操作の列（古い順）。 */
  applied: Txn[];
}

const ASSETS: StoryOverview = {
  assets: [
    { id: "a1", name: "Harbor interview", duration: 24, has_transcript: true },
    { id: "a2", name: "Drone, pier", duration: 41, has_transcript: false },
  ],
  sequences: [{ id: "s1", cuts: 4, duration: 63 }],
  open_sequence: "s1",
};

const initialTopics = (): Topic[] => [
  { id: "t1", asset: "a1", title: "Finding the harbor", in: at(HARBOR_START), out: at(HARBOR_END), rating: "favorite" },
  { id: "t2", asset: "a1", title: "Ferry schedule", in: at(FERRY_START), out: at(FERRY_END), rating: "rejected" },
];

const applyOp = (s: MockStory, op: Op): void => {
  if (op.op === "rate") {
    s.topics = s.topics.map((t) => (t.id === op.target ? { ...t, rating: op.rating } : t));
  } else if (op.op === "addTopic") {
    s.topics = [...s.topics, op.topic];
  } else if (op.op === "removeTopic") {
    s.topics = s.topics.filter((t) => t.id !== op.topic);
  } else if (op.op === "deleteWords") {
    const ids = new Set(op.words);
    s.words = s.words.map((w) => (ids.has(w.id) ? { ...w, deleted: !op.restore } : w));
  }
};

const textOf = (words: StoryWord[], from: number): string => words.slice(from, from + CONTEXT_WORDS).map((w) => w.text).join(" ");

const search = (s: MockStory, q: string): StoryHit[] =>
  s.words.flatMap((w, i) =>
    w.text.toLowerCase().includes(q.toLowerCase()) && q !== ""
      ? [{ asset: "a1", word: w.id, start: w.start, end: w.end, context: textOf(s.words, i) }]
      : [],
  );

const read = (s: MockStory, url: URL): Response | null => {
  const p = url.pathname.replace("/m/api/story/", "");
  if (p === "overview") return json(HTTP_OK, ASSETS);
  if (p === "transcript") {
    const body: StoryTranscript = { asset: "a1", words: s.words, topics: s.topics, selections: [] };
    return json(HTTP_OK, body);
  }
  if (p === "range") {
    const first = wordIndex(s.words, url.searchParams.get("first") ?? "");
    const last = wordIndex(s.words, url.searchParams.get("last") ?? "");
    const a = s.words[first];
    const b = s.words[last];
    return a && b ? json(HTTP_OK, { start: a.start, end: b.end }) : fail(HTTP_BAD_REQUEST, "bad_request", "unknown word");
  }
  if (p === "search") return json(HTTP_OK, search(s, url.searchParams.get("q") ?? ""));
  return null;
};

/** 母艦の Story の応答を手元で返す偽物。操作は受けた分だけ状態に反映する。 */
export const createMockStory = (): MockStory => {
  let conflict = false;
  const s: MockStory = {
    topics: initialTopics(),
    words: buildWords(),
    proposals: [],
    applied: [],
    conflictNext: () => void (conflict = true),
    handle: (req) => {
      const url = new URL(req.path, "http://mock");
      if (req.method === "GET" && url.pathname.startsWith("/m/api/story/")) return read(s, url);
      if (req.method !== "POST") return null;
      if (url.pathname === "/m/api/undo") return json(HTTP_OK, { version: 0 });
      const txn = (req.body as { txn?: Txn } | null)?.txn;
      if (!txn) return null;
      if (url.pathname === "/m/api/proposals") {
        s.proposals.push(txn);
        return json(HTTP_OK, { id: `p${s.proposals.length}` });
      }
      if (url.pathname !== "/m/api/ops") return null;
      if (conflict) {
        conflict = false;
        return fail(HTTP_CONFLICT, "conflict", "The project changed since the base version.");
      }
      txn.ops.forEach((op) => applyOp(s, op));
      s.applied.push(txn);
      return json(HTTP_OK, { version: txn.base_version + 1 });
    },
  };
  return s;
};
