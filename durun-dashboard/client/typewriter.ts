// 届いたばかりの Claude の返答を、一般的な AI チャットと同じく一文字ずつ書き出していくように見せる。
// Claude Code は会話ログに返答をブロックごとまとめて書くので、届いた全文を画面の側で少しずつ明かす。
import { useEffect, useState } from 'preact/hooks';

// 短い返答が一瞬で終わらず、書いている様子が見える速さ。
const CHARS_PER_SEC = 160;
// 長い返答でも読み始めるまで待たせない長さ。これを超える返答は速く書き出す。
const MAX_MS = 2500;
// 一歩ごとに Markdown を組み直すので、毎フレームではなくこの間隔で進める。
const STEP_MS = 40;

/** 書き出し始めてから elapsedMs たったときに見せる文字数。サロゲートペアの途中では切らない。 */
export const revealLength = (text: string, elapsedMs: number): number => {
  const rate = Math.max(CHARS_PER_SEC, (text.length * 1000) / MAX_MS);
  const n = Math.min(text.length, Math.floor((elapsedMs * rate) / 1000));
  const code = text.charCodeAt(n);
  return code >= 0xdc00 && code <= 0xdfff ? n + 1 : n;
};

/** on のとき、text を先頭から少しずつ明かした文字列を返す。onStep は明かす量が増えるたびに呼ぶ。 */
export const useTypewriter = (text: string, on: boolean, onStep?: () => void): string => {
  const [shown, setShown] = useState(on ? 0 : text.length);
  useEffect(() => {
    if (!on) return;
    const startedAt = performance.now();
    const timer = setInterval(() => {
      const n = revealLength(text, performance.now() - startedAt);
      setShown(n);
      onStep?.();
      if (n >= text.length) clearInterval(timer);
    }, STEP_MS);
    return () => {
      clearInterval(timer);
      setShown(text.length);
    };
  }, [on]);
  return shown >= text.length ? text : text.slice(0, shown);
};
