// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import { EDIT_MESSAGE_EVENT } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeListen } from "./fakeListen.ts";
import { Shell } from "./Shell.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const settle = () => act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
const message = () => host.querySelector<HTMLElement>(".mm-shell-status > .mm-shell-message");

/** Shell を出し、アプリの編集の結果の文言のイベントを届ける関数を返す。 */
const mount = async () => {
  const { listen, fire } = fakeListen();
  act(() => render(<Shell sendOp={vi.fn()} listen={listen} />, host));
  await settle();
  return (text: string, failed: boolean, serial: number) => act(() => fire(EDIT_MESSAGE_EVENT, { text, failed, serial }));
};

describe("Shell のステータスの行", () => {
  it("編集の結果の文言を列の下に出し、拒否の理由を見分け、次の編集が成功して空になると消す", async () => {
    const send = await mount();
    expect(host.querySelector(".mm-shell-status")?.getAttribute("role")).toBe("status");
    send("No clip at the playhead", true, 1);
    expect(message()?.textContent).toBe("No clip at the playhead");
    expect(message()?.dataset["failed"]).toBe("true");
    expect(host.querySelector(".mm-shell-right [role=status]")).toBeNull();
    send("Undo: Split Clip (webview)", false, 2);
    expect(message()?.dataset["failed"]).toBe("false");
    send("", false, 3);
    expect(message()).toBeNull();
  });

  it("同じ文言を置き直すと文言の要素を作り直し、押したことが分かるようにする", async () => {
    const send = await mount();
    send("No clip open in Source", true, 1);
    const first = message();
    send("No clip open in Source", true, 2);
    expect(message()?.textContent).toBe("No clip open in Source");
    expect(message()).not.toBe(first);
  });
});
