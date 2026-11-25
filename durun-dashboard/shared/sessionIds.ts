// 発言に出たセッションの id を見分ける。画面ではリンクに、サーバーでは関連セッションにする。
// セッションの id は UUID。省略形は、bg のジョブの名前などに出る先頭の 8 文字。
export const SESSION_ID = /((?<![\w-])[0-9a-f]{8}(?:-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})?(?![\w-]))/;

/** 文 1 つがまるごとセッションの id なら、その id。省略形は、先頭が合うセッションが 1 つだけのときに限る。コミットのハッシュなどを取り違えないため。 */
export const sessionIdOf = (ids: string[], text: string): string | undefined => {
  if (text.length > 8) return ids.includes(text) ? text : undefined;
  const hits = ids.filter((id) => id.startsWith(text));
  return hits.length === 1 ? hits[0] : undefined;
};

/** 文に出た、ids にあるセッションの id。省略形はフルの id に直し、出た順に重ねずに並べる。 */
export const sessionIdsIn = (ids: string[], texts: string[]): string[] => {
  const re = new RegExp(SESSION_ID.source, 'g');
  return [...new Set(texts.flatMap((t) => [...t.matchAll(re)].flatMap((m) => sessionIdOf(ids, m[1] ?? '') ?? [])))];
};
