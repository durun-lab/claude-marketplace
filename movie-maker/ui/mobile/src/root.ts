/** 画面の差し込み先。無ければ HTML が壊れているので、気づけるよう投げる。 */
export const mountPoint = (): HTMLElement => {
  const el = document.getElementById("root");
  if (!el) throw new Error("#root is missing");
  return el;
};
