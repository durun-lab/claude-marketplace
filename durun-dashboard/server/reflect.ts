// 学習のワークフローが MCP の learn_digest で読む、前回より後の人間の発言をまとめる。
// 学習の指示は plugin の agents/learn.md にあり、ワークフローの時刻のトリガーや「今すぐ実行」で動く。
// 知見のファイルとその管理は利用者の側にあり、ダッシュボードは書き込まない。
import fs from 'node:fs';
import path from 'node:path';
import type { ChatEntry } from '../shared/types.ts';
import { summarizeLog } from './claude.ts';
import { DATA_DIR, state, touch } from './store.ts';

export const LEARN_DIR = path.join(DATA_DIR, 'learn');
// 初めて集めるときに遡る長さ。
const FIRST_LOOKBACK_MS = 24 * 60 * 60_000;
// 直前の Claude の発言は、人間が何に答えたかが分かる程度に切る。人間の発言は、教えた内容を落とさないよう長めに残す。
const REPLY_CHARS = 400;
const PROMPT_CHARS = 1500;
// 1 回に読ませる量の上限。超えた分は古い発言から落とす。
const DIGEST_CHARS = 300_000;
// ダッシュボードがセッションに渡す依頼の書き出しと、plugin のスキルの呼び出し。人間が教えたことではないので拾わない。
const DASHBOARD_MARKS = ['[durun-dashboard', '/durun-dashboard:'];

export type DigestSession = { title: string; cwd: string; texts: ChatEntry[] };

const clip = (text: string, max: number): string => {
  const flat = text.trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
};

/**
 * since より後の人間の発言を、直前の Claude の発言と組にして並べる。
 * 人間が「そうではない、こうだ」と教えるのは、Claude の発言を受けたときなので、その発言を添える。
 */
export const digestOf = (sessions: DigestSession[], since: number): string => {
  const parts: string[] = [];
  for (const s of sessions) {
    const lines: string[] = [];
    let reply: string | undefined;
    for (const e of s.texts) {
      if (e.kind === 'message') reply = e.text;
      if (e.kind !== 'prompt' || (e.at ?? 0) <= since || DASHBOARD_MARKS.some((m) => e.text.startsWith(m))) continue;
      if (reply) lines.push(`Claude: ${clip(reply, REPLY_CHARS)}`);
      lines.push(`人間: ${clip(e.text, PROMPT_CHARS)}`, '');
      reply = undefined;
    }
    if (lines.length) parts.push(`## ${s.title}\n作業ディレクトリ: ${s.cwd}\n\n${lines.join('\n')}`);
  }
  const all = parts.join('\n\n');
  return all.length > DIGEST_CHARS ? all.slice(all.length - DIGEST_CHARS) : all;
};

const mtimeOf = (file: string): number => {
  try {
    return fs.statSync(file).mtimeMs;
  } catch {
    return 0;
  }
};

export type LearnDigest = { file: string; since: number; cwds: string[] };

/** 前回集めたときより後の人間の発言をファイルに書く。発言が無ければ、次も同じ時刻から集めるよう、何も残さない。 */
export const learnDigest = (): LearnDigest | undefined => {
  const now = Date.now();
  const since = state.learn?.digestedAt ?? now - FIRST_LOOKBACK_MS;
  // 会話ログは全部で 1 GB を超えるので、前回より後に書かれたものだけを読む。
  const metas = Object.values(state.sessions).filter((m) => m.transcriptPath && !m.movedTo && mtimeOf(m.transcriptPath) > since);
  const sessions = metas.map((m) => ({ title: m.title ?? m.name ?? m.id, cwd: m.cwd, texts: summarizeLog(m.transcriptPath as string).texts }));
  const digest = digestOf(sessions, since);
  if (!digest) return undefined;
  fs.mkdirSync(LEARN_DIR, { recursive: true });
  const file = path.join(LEARN_DIR, `${new Date(now).toISOString().replace(/[:.]/g, '').slice(0, 15)}.md`);
  fs.writeFileSync(file, digest);
  state.learn = { digestedAt: now };
  touch();
  const cwds = [...new Set(sessions.filter((s) => s.texts.some((e) => (e.at ?? 0) > since)).map((s) => s.cwd))];
  return { file, since, cwds };
};
