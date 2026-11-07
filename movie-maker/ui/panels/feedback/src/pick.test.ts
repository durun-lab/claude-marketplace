// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { reportWindowErrors } from "./reportErrors.ts";
import { selectorOf, startPicking, targetOf } from "./pick.ts";

afterEach(() => {
  document.body.innerHTML = "";
});

const mount = () => {
  document.body.innerHTML = `
    <section aria-label="Review">
      <ul class="mm-review-list">
        <li class="mm-review-card"><button class="mm-review-button primary extra">Approve</button></li>
        <li class="mm-review-card"><button class="mm-review-button" aria-label="Reject proposal">x</button></li>
      </ul>
    </section>`;
  const [approve, reject] = document.querySelectorAll("button");
  if (!approve || !reject) throw new Error("ボタンが無い");
  return [approve, reject] as const;
};

describe("指した部品の識別子", () => {
  it("短いセレクター、パネル、名前、位置を作る", () => {
    const [approve, reject] = mount();
    expect(selectorOf(approve)).toBe(
      "section > ul.mm-review-list > li.mm-review-card:nth-of-type(1) > button.mm-review-button.primary",
    );
    const t = targetOf(reject, 2);
    expect(t.panel).toBe("Review");
    expect(t.label).toBe("Reject proposal");
    expect(t.dpr).toBe(2);
    expect(t.rect).toEqual({ x: 0, y: 0, width: 0, height: 0 });
  });

  it("id を持つ祖先で止める", () => {
    document.body.innerHTML = `<div id="root"><div><span class="a b c">t</span></div></div>`;
    expect(selectorOf(document.querySelector("span") ?? document.body)).toBe("div#root > div > span.a.b");
  });
});

describe("指すモード", () => {
  it("クリックを部品に届けずに拾い、Esc で抜け、止めると何も拾わない", () => {
    const [approve] = mount();
    const clicked = vi.fn();
    approve.addEventListener("click", clicked);
    const onPick = vi.fn();
    const onCancel = vi.fn();
    const stop = startPicking(document, { onHover: vi.fn(), onPick, onCancel }, () => false);
    approve.click();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(onPick).toHaveBeenCalledWith(approve);
    expect(clicked).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalled();
    stop();
    approve.click();
    expect(onPick).toHaveBeenCalledTimes(1);
    expect(clicked).toHaveBeenCalledTimes(1);
  });
});

describe("画面の例外の見張り", () => {
  it("error と unhandledrejection を送り、同じものは 1 回だけ", async () => {
    const report = vi.fn().mockResolvedValue(undefined);
    const off = reportWindowErrors(window, report);
    const err = new TypeError("x is undefined");
    for (let i = 0; i < 2; i++) {
      window.dispatchEvent(new ErrorEvent("error", { error: err, message: err.message, filename: "story.js", lineno: 3, colno: 7 }));
    }
    const rejection = new Event("unhandledrejection") as PromiseRejectionEvent;
    Object.defineProperty(rejection, "reason", { value: "ipc failed" });
    window.dispatchEvent(rejection);
    off();
    window.dispatchEvent(new ErrorEvent("error", { message: "after off" }));
    expect(report.mock.calls.map((c) => c[0])).toEqual([
      { message: "TypeError: x is undefined", stack: err.stack, where: "story.js:3:7" },
      { message: "ipc failed", where: "unhandledrejection" },
    ]);
  });
});
