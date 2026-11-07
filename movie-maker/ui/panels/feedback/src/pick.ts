import type { Target } from "./api.ts";

/** セレクターにたどる祖先の数。長すぎると読めず、短すぎると同じ形の部品を見分けられない。 */
const SELECTOR_DEPTH = 4;
/** セレクターに載せるクラスの数。 */
const CLASSES_PER_STEP = 2;
/** 部品の名前に使う文字の上限。 */
const LABEL_CHARS = 80;

/** 1 段の名前。`tag.class` と、同じ形の兄弟がいれば `:nth-of-type`。 */
const step = (el: Element): string => {
  const tag = el.tagName.toLowerCase();
  if (el.id) return `${tag}#${el.id}`;
  const classes = [...el.classList].slice(0, CLASSES_PER_STEP).map((c) => `.${c}`).join("");
  const parent = el.parentElement;
  const same = parent ? [...parent.children].filter((c) => c.tagName === el.tagName) : [];
  const nth = same.length > 1 ? `:nth-of-type(${same.indexOf(el) + 1})` : "";
  return `${tag}${classes}${nth}`;
};

/** 部品を指す短い CSS セレクター。id を持つ祖先か `SELECTOR_DEPTH` 段で止める。 */
export const selectorOf = (el: Element): string => {
  const parts: string[] = [];
  let cur: Element | null = el;
  while (cur && cur !== document.body && parts.length < SELECTOR_DEPTH) {
    parts.unshift(step(cur));
    if (cur.id) break;
    cur = cur.parentElement;
  }
  return parts.join(" > ");
};

/** 部品の属するパネル。パネルの根は `aria-label` を持つ `section`。 */
const panelOf = (el: Element): string | null => el.closest("section[aria-label]")?.getAttribute("aria-label") ?? null;

/** 人が読む部品の名前。`aria-label`、`title`、中の文字の順。 */
const labelOf = (el: Element): string => {
  const named = el.getAttribute("aria-label") ?? el.getAttribute("title");
  const text = named ?? el.textContent ?? "";
  return text.replace(/\s+/g, " ").trim().slice(0, LABEL_CHARS);
};

/** 指した部品を要望に添える形にする。 */
export const targetOf = (el: Element, dpr: number): Target => {
  const r = el.getBoundingClientRect();
  return {
    selector: selectorOf(el),
    panel: panelOf(el),
    label: labelOf(el),
    rect: { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) },
    dpr,
  };
};

/** 指すモードの間の見た目。部品の上に枠を重ねる。 */
export interface PickHandlers {
  onHover: (el: Element | null) => void;
  onPick: (el: Element) => void;
  onCancel: () => void;
}

/**
 * 画面の部品を指すモードを始める。クリックは捕まえて部品に届けず、Esc で抜ける。戻り値で止める。
 * `ignore` に当たる要素（指すのをやめるボタンなど）は指さず、クリックもそのまま届ける。
 */
export const startPicking = (doc: Document, handlers: PickHandlers, ignore: (el: Element) => boolean): (() => void) => {
  const hit = (e: Event): Element | null => {
    const el = e.target instanceof Element ? e.target : null;
    return el && !ignore(el) ? el : null;
  };
  const move = (e: MouseEvent) => handlers.onHover(hit(e));
  const click = (e: MouseEvent) => {
    const el = hit(e);
    if (!el) return;
    e.preventDefault();
    e.stopPropagation();
    handlers.onPick(el);
  };
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") handlers.onCancel();
  };
  doc.addEventListener("mousemove", move, true);
  doc.addEventListener("click", click, true);
  doc.addEventListener("keydown", key, true);
  return () => {
    doc.removeEventListener("mousemove", move, true);
    doc.removeEventListener("click", click, true);
    doc.removeEventListener("keydown", key, true);
  };
};
