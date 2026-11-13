// 改善要望を保存し、対応を終えた要望を済みにする。要望への対応は、「改善要望が届いたとき」のトリガーを持つワークフローが動かす。
// ウェーブの区切りは、セッションの最後の発言の「対応した要望:」で読む。
import fs from 'node:fs';
import path from 'node:path';
import { listDir, readJson } from './claude.ts';
import { hostOs } from './platform.ts';
import { DATA_DIR, type LegacyImprove, newId, state, touch } from './store.ts';

const ROOT = path.join(import.meta.dirname, '..');
const FEEDBACK_DIR = path.join(DATA_DIR, 'feedback');
const DONE_DIR = path.join(FEEDBACK_DIR, 'done');
const SUMMARY_CHARS = 120;
export const DONE_MARK = '対応した要望:';
// main はマージキューを通さないと変えられないので、手元ではこのブランチにマージしてダッシュボードにすぐ反映する。
const LOCAL_BRANCH = 'local/dashboard';
// 手元のチェックアウトには並行するセッションの書きかけがあるので、実装はリポジトリの外から切り離した worktree で進める。
const WORKTREES = path.join(ROOT, '..', '.worktrees');

/** 未対応の要望。ファイル名が時刻から始まるので、名前順が古い順になる。 */
export const pendingItems = (): string[] => listDir(FEEDBACK_DIR).filter((f) => f.endsWith('.json')).sort();

const summaryOf = (item: string): string =>
  String(readJson<{ text: string }>(path.join(FEEDBACK_DIR, item))?.text ?? '').replace(/\s+/g, ' ').slice(0, SUMMARY_CHARS);

/** ワークフローのイベントとして渡す、たまった要望の一覧。読み方とウェーブを区切る印の書き方は、ワークフローの指示に書く。 */
export const feedbackEvent = (items: string[]): string => `改善要望が ${items.length} 件たまっています（古い順）。
${items.map((item) => `- ${path.join(FEEDBACK_DIR, item)}: ${summaryOf(item)}`).join('\n')}`;

/** 自動実装の設定から、同じ動きをするワークフローの定義を作る。指示と model と effort は、plugin に同梱した improve の agent を使う。 */
export const improveWorkflow = (id: string, legacy: LegacyImprove) => ({
  id,
  name: '改善要望の対応',
  enabled: legacy.enabled === true,
  cwd: ROOT,
  triggers: [{ kind: 'feedback' }],
  // 1 つのセッションで前のウェーブの文脈を引き継ぎ、毎回 compact して会話を短く保つ。
  mode: 'continue',
  compact: true,
});

// 済んだ要望は、何をしたか（セッションの最後の発言）を先頭に置いて done/ へ移す。
// その要望を起票したエラーや遅い処理の記録は、回数と最長を数え直す。再発して起票し直す要望に、直す前の原因で長かった回を混ぜないため。
const finishItem = (item: string, sessionId: string, result: string): void => {
  const file = path.join(FEEDBACK_DIR, item);
  fs.mkdirSync(DONE_DIR, { recursive: true });
  fs.writeFileSync(path.join(DONE_DIR, item), JSON.stringify({ result, sessionId, doneAt: new Date().toISOString(), ...readJson(file) }, null, 2));
  fs.rmSync(file, { force: true });
  for (const r of Object.values(state.errors)) if (r.item === item) Object.assign(r, { count: 0, slowestMs: undefined, slowestLoad: undefined });
};

/**
 * 最後の発言の「対応した要望:」より後に名前がある要望を済みにし、その数を返す。拡張子を省いて書かれても拾う。
 * 印が無ければ、ウェーブはまだ終わっていないので undefined を返す。
 */
export const finishItems = (sessionId: string, message: string | undefined): number | undefined => {
  const at = message?.lastIndexOf(DONE_MARK) ?? -1;
  if (!message || at < 0) return undefined;
  const done = pendingItems().filter((item) => message.includes(item.replace(/\.json$/, ''), at));
  for (const item of done) finishItem(item, sessionId, message);
  touch();
  return done.length;
};

const listeners = new Set<() => void>();

/** 要望が届いたときに呼ぶ処理を足す。 */
export const onFeedback = (fn: () => void): void => void listeners.add(fn);

export const saveFeedback = (text: string, targets: unknown[], context: object): string => {
  const createdAt = new Date().toISOString();
  // 同じ秒に続けて送っても順番が崩れないよう、ミリ秒まで名前に入れる。
  const file = path.join(FEEDBACK_DIR, `${createdAt.replace(/[:.]/g, '').slice(0, 20)}-${newId()}.json`);
  fs.mkdirSync(FEEDBACK_DIR, { recursive: true });
  // 画面の状態は大きいので最後に置き、ファイルの先頭で要望と指した要素を読めるようにする。
  // OS はサーバーで足すので、エラーから起票した要望にも載る。
  fs.writeFileSync(file, JSON.stringify({ createdAt, text, targets, context: { os: hostOs(), ...context } }, null, 2));
  // 未対応の数が変わるので、ワークフローが動かないときも画面を更新させる。
  touch();
  for (const fn of listeners) fn();
  return path.relative(ROOT, file);
};
