import { vi } from "vitest";

/** 試験で Tauri の listen の代わりに渡す。イベント名ごとに最後の購読を持ち、`fire` で届ける。 */
export const fakeListen = () => {
  const handlers = new Map<string, (e: { payload: unknown }) => void>();
  const off = vi.fn();
  const listen = vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    handlers.set(event, handler);
    return off;
  });
  const fire = (event: string, payload: unknown) => handlers.get(event)?.({ payload });
  return { listen, off, fire };
};
