// ホームのウィジェットの中のスクロール位置。画面を移ってホームが作り直されても戻せるよう、モジュールに覚える。
// ページを読み込み直すと消えるので、再読み込みでは先頭から始まる。
import { type RefObject } from 'preact';
import { useEffect } from 'preact/hooks';

const kept = new Map<string, number>();

// 中身が読み込み終わるまでは戻す先の高さが足りないので、この間は描き直しのたびに戻し直す。
const RESTORE_MS = 3000;

/** 何番目のウィジェットの、最初のクラスが同じ要素のうち何番目か。ウィジェットの並びとクラスは、画面を移っても変わらない。 */
const keyOf = (root: HTMLElement, el: Element): string | undefined => {
  const widget = el.closest('.widget');
  const cls = el.classList[0];
  if (!widget || !cls || !root.contains(widget)) return undefined;
  const w = [...root.querySelectorAll('.widget')].indexOf(widget);
  const i = [...widget.getElementsByClassName(cls)].indexOf(el);
  return `${w}:${cls}:${i}`;
};

const elementOf = (root: HTMLElement, key: string): HTMLElement | undefined => {
  const [w = '', cls = '', i = ''] = key.split(':');
  return root.querySelectorAll('.widget')[Number(w)]?.getElementsByClassName(cls)[Number(i)] as HTMLElement | undefined;
};

/**
 * root の中のウィジェットのスクロール位置を覚え、作り直したときに戻す。人間がスクロールし始めたら、戻すのをやめる。
 * scope は、ウィジェットの並びが違う配置を見分ける値。ダッシュボードを切り替えたら、別の位置として覚える。
 */
export const useKeptScroll = (root: RefObject<HTMLElement>, scope: string): void => {
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const prefix = `${scope}/`;
    const pending = new Map([...kept].filter(([k]) => k.startsWith(prefix)).map(([k, v]) => [k.slice(prefix.length), v]));
    const restore = () => {
      for (const [key, top] of pending) {
        const target = elementOf(el, key);
        if (!target || target.scrollHeight - target.clientHeight < top) continue;
        target.scrollTop = top;
        pending.delete(key);
      }
    };
    const observer = new MutationObserver(restore);
    const stop = () => {
      observer.disconnect();
      pending.clear();
    };
    const onScroll = (e: Event) => {
      const key = e.target instanceof Element ? keyOf(el, e.target) : undefined;
      if (key) kept.set(prefix + key, (e.target as Element).scrollTop);
    };
    restore();
    if (pending.size) observer.observe(el, { childList: true, subtree: true });
    const timer = setTimeout(stop, RESTORE_MS);
    el.addEventListener('scroll', onScroll, { capture: true, passive: true });
    for (const type of ['wheel', 'pointerdown', 'keydown', 'touchstart']) el.addEventListener(type, stop, { capture: true, passive: true });
    return () => {
      stop();
      clearTimeout(timer);
      el.removeEventListener('scroll', onScroll, { capture: true });
      for (const type of ['wheel', 'pointerdown', 'keydown', 'touchstart']) el.removeEventListener(type, stop, { capture: true });
    };
  }, [scope]);
};
