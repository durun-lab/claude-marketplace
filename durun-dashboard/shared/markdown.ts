import { lexer, type Token } from 'marked';

// 先頭の YAML のフロントマター。閉じる --- が、下線で書く見出しとして読まれないよう外す。
const FRONT_MATTER = /^---\r?\n[\s\S]*?\r?\n---\r?\n/;

/** Markdown の最初の見出しの文字。コードブロックの中の # は見出しとみなさない。見出しが無ければ undefined。 */
export const firstHeading = (markdown: string): string | undefined => {
  const heading = lexer(markdown.replace(FRONT_MATTER, '')).find((t) => t.type === 'heading');
  return heading?.type === 'heading' ? heading.text.trim() || undefined : undefined;
};

/** GitHub と同じく、<details> から </details> までを折りたたむ 1 つのブロック。 */
export type DetailsBlock = { type: 'details'; summary: string; open: boolean; tokens: Token[] };

const OPEN_DETAILS = /^\s*<details[\s>]/i;
const count = (text: string, re: RegExp): number => text.match(re)?.length ?? 0;

/**
 * marked は <details> と </details> をそれぞれ HTML のブロックとして返し、間の Markdown を別のトークンにする。
 * その間のトークンを DetailsBlock にまとめる。<summary> が無ければ GitHub と同じく「詳細」と出す。lex は同じ HTML のブロックに書いた本文を読むのに使う。
 */
export const groupDetails = (tokens: Token[], lex: (markdown: string) => Token[]): (Token | DetailsBlock)[] => {
  const out: (Token | DetailsBlock)[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i] as Token;
    if (t.type !== 'html' || !OPEN_DETAILS.test(t.text)) {
      out.push(t);
      continue;
    }
    const summary = /<summary[^>]*>([\s\S]*?)<\/summary>/i.exec(t.text);
    const head = summary ? t.text.slice(summary.index + summary[0].length) : t.text.replace(/^\s*<details[^>]*>/i, '');
    const close = head.lastIndexOf('</details>');
    const block: DetailsBlock = {
      type: 'details',
      summary: summary?.[1]?.replace(/<[^>]+>/g, '').trim() || '詳細',
      open: /^\s*<details[^>]*\sopen[\s>=]/i.test(t.text),
      tokens: lex(close >= 0 ? head.slice(0, close) : head),
    };
    if (close < 0) {
      let depth = 1;
      let j = i + 1;
      for (; j < tokens.length; j++) {
        const u = tokens[j] as Token;
        if (u.type === 'html') depth += count(u.text, /<details[\s>]/gi) - count(u.text, /<\/details>/gi);
        if (depth <= 0) break;
      }
      block.tokens.push(...tokens.slice(i + 1, j));
      i = j;
    }
    out.push(block);
  }
  return out;
};
