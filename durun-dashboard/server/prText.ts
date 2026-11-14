// Claude が PR のタイトルと本文を書き換えるときに、人間が GitHub 上で足した変更を上書きさせない。
// 組み込みの Write と同じく、読んでいない PR と、読んだ後に変わった PR への書き込みを断る。
import { forgetCached } from './cache.ts';
import { gh as realGh } from './integrations.ts';

type Gh = (args: string[], input?: string, cwd?: string) => Promise<string>;
type Result = { text: string; isError?: boolean };
type PrText = { url: string; title: string; body: string };

/** セッションが最後に読んだ、または書いたタイトルと本文。サーバーを起動し直すと忘れるので、読み直しを求めることになる。 */
const seen = new Map<string, PrText>();
const key = (sessionId: string, url: string): string => `${sessionId}\n${url}`;

/** ref は PR の URL か番号。番号は cwd のリポジトリで探す。 */
const live = async (gh: Gh, ref: string, cwd?: string): Promise<PrText> => JSON.parse(await gh(['pr', 'view', ref, '--json', 'url,title,body'], undefined, cwd)) as PrText;

export const readPr = async (sessionId: string, ref: string, cwd?: string, gh: Gh = realGh): Promise<Result> => {
  const pr = await live(gh, ref, cwd);
  seen.set(key(sessionId, pr.url), pr);
  return { text: `${pr.url}\nタイトル: ${pr.title}\n\n${pr.body}` };
};

/** title と body は、省いたほうを変えない。 */
export const writePr = async (sessionId: string, ref: string, next: { title?: string; body?: string }, cwd?: string, gh: Gh = realGh): Promise<Result> => {
  const pr = await live(gh, ref, cwd);
  const k = key(sessionId, pr.url);
  const last = seen.get(k);
  if (!last) return { text: `${pr.url} をまだ読んでいません。read_pr で読んでから、いまのタイトルと本文に変更を当てて書いてください。`, isError: true };
  if (last.title !== pr.title || last.body !== pr.body) return { text: `${pr.url} は、読んだ後に GitHub 上で変わっています。read_pr で読み直し、いまのタイトルと本文に変更を当て直してください。`, isError: true };
  // GitHub には条件付きの更新が無いので、ここから書き終えるまでの間に入った変更は防げない。
  await gh(['pr', 'edit', pr.url, ...(next.title === undefined ? [] : ['--title', next.title]), ...(next.body === undefined ? [] : ['--body-file', '-'])], next.body);
  forgetCached(`pr-v2:${pr.url}`);
  // GitHub が改行などを直して保存することがあるので、渡した値ではなく、保存された値を覚える。
  seen.set(k, await live(gh, pr.url));
  return { text: `${pr.url} を書き換えました。` };
};
