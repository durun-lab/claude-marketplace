import { useLayoutEffect } from "preact/hooks";

/** 列が狭くても残す高さ（px）。Graph と Inspector の数行が見える。 */
export const MIN_FIT_HEIGHT = 240;

/** 列の見えている高さ `visible` のうち、列の中身の上端から `top` の位置より下で使える高さ。 */
export const fitHeight = (visible: number, top: number, bottomPadding: number): number =>
  Math.max(MIN_FIT_HEIGHT, Math.floor(visible - top - bottomPadding));

/** いちばん近いスクロールする祖先（右の列）。 */
const scrollParent = (el: HTMLElement): HTMLElement | null => {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const overflow = getComputedStyle(p).overflowY;
    if (overflow === "auto" || overflow === "scroll") return p;
  }
  return null;
};

/**
 * 要素の高さを、スクロールする列の見えている所の下端までに合わせる。中身は要素の中でスクロールし、
 * 列ごとスクロールして下の部品が画面の外へ出ることがない。列の大きさとタブの表示が変わったら測り直す。
 */
export const useFitToColumn = (ref: { readonly current: HTMLElement | null }): void => {
  useLayoutEffect(() => {
    const el = ref.current;
    const column = el ? scrollParent(el) : null;
    if (!el || !column || typeof ResizeObserver === "undefined") return;
    const fit = () => {
      const top = el.getBoundingClientRect().top - column.getBoundingClientRect().top + column.scrollTop;
      const padding = Number.parseFloat(getComputedStyle(column).paddingBottom) || 0;
      el.style.height = `${fitHeight(column.clientHeight, top, padding)}px`;
    };
    const observer = new ResizeObserver(fit);
    observer.observe(column);
    if (el.parentElement) observer.observe(el.parentElement);
    fit();
    return () => observer.disconnect();
  }, [ref]);
};
