// 改善要望を保存し、対応を終えた要望を済みにする。要望への対応は、「改善要望が届いたとき」のトリガーを持つワークフローが動かす。
// ウェーブの区切りは、セッションの最後の発言の「対応した要望:」で読む。読み方の約束はここに置き、ワークフローの prompt は人間が直せるようにする。
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

/** ワークフローのイベントとして渡す、たまった要望の一覧と、ウェーブを区切る印の書き方。 */
export const feedbackEvent = (items: string[]): string => `人間がダッシュボードの画面から送った改善要望が ${items.length} 件たまっています。
- 要望は次のファイルです（古い順）。text が要望で、targets は人間が画面上で指した要素です。selector のクラス名から、描画しているコンポーネントを探せます。context は送ったときの画面の状態です。context.snapshot は大きいので、必要なところだけ読んでください。
${items.map((item) => `  - ${path.join(FEEDBACK_DIR, item)}: ${summaryOf(item)}`).join('\n')}
- 終えたら、最後に「${DONE_MARK}」の行を置き、続けて対応を終えた要望のファイル名を 1 行に 1 つずつ書いて止まってください。残りの要望は、次のウェーブで計画し直します。`;

const IMPROVE_PROMPT = `全体を見て作業計画を立て、1 つのウェーブとして実装してください。
- まず全体を読み、似た作業や同じ場所を触る要望をまとめて、このウェーブで対応する要望と順番を決めてください。一度に仕上げられる量にとどめ、残りは次のウェーブに回してください。
- ダッシュボードは起動済みで、${LOCAL_BRANCH} のコードが変わると画面とサーバーに自動で反映されます。pnpm start は実行しないでください。
- ダッシュボードは手元専用のブランチ ${LOCAL_BRANCH} のチェックアウト（${ROOT}）で動いています。ここには並行するセッションの書きかけがあるので、このチェックアウトでは実装せず、書きかけにも触らないでください。
- 実装を始める前に git fetch origin main を実行し、git worktree add ${WORKTREES}/improve-<日時> -b fix/dashboard-improve-<日時> origin/main で worktree を作ってください。worktree の durun-dashboard で pnpm install を実行してから、そこで実装します。Bash の作業ディレクトリはコールをまたいで保たれないので、毎回 cd <絶対パス> && を付けるか git -C を使ってください。
- まとまりごとに worktree で pnpm check と pnpm test を通し、1 つのコミットにしてください。コードレビューは依頼せず、そのまま次のまとまりへ進んでください。
- main は直接 push できません。すべてのまとまりを終えたら、ブランチを push -u し、PR を作って gh pr merge --auto でマージキューに載せてください。
- 続けて、${ROOT} で ${LOCAL_BRANCH} にいることを確かめ、git merge --no-edit fix/dashboard-improve-<日時> でこのウェーブを取り込んでください。衝突したら両方の意図を残して解消してください。${LOCAL_BRANCH} は push しないでください。取り込んだら git worktree remove で worktree を片付けてください。
- 判断に迷う点は ask_human で人間に聞いてください。
- 終えたら、まとまりごとに何をどう変えたかを 2、3 行で書いてください。`;

/** 自動実装の設定から、同じ動きをするワークフローの定義を作る。workflows.json にそのまま書く。 */
export const improveWorkflow = (id: string, legacy: LegacyImprove) => ({
  id,
  name: '改善要望の対応',
  enabled: legacy.enabled === true,
  cwd: ROOT,
  prompt: IMPROVE_PROMPT,
  triggers: [{ kind: 'feedback' }],
  mode: 'new',
  ...(legacy.model ? { model: legacy.model } : {}),
  ...(legacy.effort ? { effort: legacy.effort } : {}),
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
