import type { WebError } from "./api.ts";

/** 1 回の起動で送る画面の例外の上限。同じものはアプリがまとめるが、送るたびにファイルを書くので数を絞る。 */
const MAX_REPORTS = 20;

const describe = (reason: unknown): WebError => {
  if (reason instanceof Error) return { message: `${reason.name}: ${reason.message}`, stack: reason.stack };
  return { message: String(reason) };
};

/**
 * 画面の例外（`error` と `unhandledrejection`）を自動の起票に送る。Shell が API を作るときに 1 回だけ呼ぶので、
 * どのパネルが開いていても届く。送るのに失敗しても、例外をもう一度出さない。戻り値で外す。
 */
export const reportWindowErrors = (win: Window, report: (e: WebError) => Promise<void>): (() => void) => {
  const seen = new Set<string>();
  const send = (e: WebError) => {
    if (seen.size >= MAX_REPORTS || seen.has(e.message)) return;
    seen.add(e.message);
    report(e).catch(() => undefined);
  };
  const onError = (e: ErrorEvent) => {
    const base = e.error === undefined || e.error === null ? { message: e.message } : describe(e.error);
    send({ ...base, where: e.filename ? `${e.filename}:${e.lineno}:${e.colno}` : undefined });
  };
  const onRejection = (e: PromiseRejectionEvent) => send({ ...describe(e.reason), where: "unhandledrejection" });
  win.addEventListener("error", onError);
  win.addEventListener("unhandledrejection", onRejection);
  return () => {
    win.removeEventListener("error", onError);
    win.removeEventListener("unhandledrejection", onRejection);
  };
};
