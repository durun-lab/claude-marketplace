// Claude が PR の本文を書き換えるときに、人間が GitHub 上で足した変更を上書きさせない。
// 組み込みの Write と同じく、読んでいない本文と、読んだ後に変わった本文への書き込みを断る。
import { forgetCached } from './cache.ts';
import { gh as realGh } from './integrations.ts';

type Gh = (args: string[], input?: string, cwd?: string) => Promise<string>;
type Result = { text: string; isError?: boolean };

/** セッションが最後に読んだ、または書いた本文。サーバーを起動し直すと忘れるので、読み直しを求めることになる。 */
const seen = new Map<string, string>();
const key = (sessionId: string, url: string): string => `${sessionId}\n${url}`;

/** ref は PR の URL か番号。番号は cwd のリポジトリで探す。 */
const live = async (gh: Gh, ref: string, cwd?: string): Promise<{ url: string; body: string }> =>
  JSON.parse(await gh(['pr', 'view', ref, '--json', 'url,body'], undefined, cwd)) as { url: string; body: string };

export const readPrBody = async (sessionId: string, ref: string, cwd?: string, gh: Gh = realGh): Promise<Result> => {
  const pr = await live(gh, ref, cwd);
  seen.set(key(sessionId, pr.url), pr.body);
  return { text: `${pr.url} の本文は次のとおりです。\n\n${pr.body}` };
};

export const writePrBody = async (sessionId: string, ref: string, body: string, cwd?: string, gh: Gh = realGh): Promise<Result> => {
  const pr = await live(gh, ref, cwd);
  const k = key(sessionId, pr.url);
  const last = seen.get(k);
  if (last === undefined) return { text: `${pr.url} の本文をまだ読んでいません。read_pr_body で読んでから、いまの本文に変更を当てて書いてください。`, isError: true };
  if (last !== pr.body) return { text: `${pr.url} の本文は、読んだ後に GitHub 上で変わっています。read_pr_body で読み直し、いまの本文に変更を当て直してください。`, isError: true };
  // GitHub には条件付きの更新が無いので、ここから書き終えるまでの間に入った変更は防げない。
  await gh(['pr', 'edit', pr.url, '--body-file', '-'], body);
  forgetCached(`pr-v2:${pr.url}`);
  // GitHub が改行などを直して保存することがあるので、渡した body ではなく、保存された本文を覚える。
  seen.set(k, (await live(gh, pr.url)).body);
  return { text: `${pr.url} の本文を書き換えました。` };
};
