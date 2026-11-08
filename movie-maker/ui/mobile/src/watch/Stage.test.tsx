// @vitest-environment jsdom
import { createRef, render } from "preact";
import { describe, expect, it } from "vitest";
import type { Editing } from "./DrawLayer.tsx";
import { Stage } from "./Stage.tsx";

describe("Stage", () => {
  it("映像を読めない知らせは描き込みの面より下に重なり、描く指と線を覆わない", () => {
    const root = document.createElement("div");
    const editing: Editing = { tool: "stroke", onShape: () => undefined };
    render(<Stage videoRef={createRef()} playing={false} failed shapes={[]} frame={{ width: 16, height: 9 }} editing={editing} onToggle={() => undefined} />, root);
    const order = [...root.querySelectorAll(".stage > *")].map((el) => el.classList.item(0));
    expect(order.indexOf("overlay")).toBeGreaterThan(-1);
    expect(order.indexOf("overlay")).toBeLessThan(order.indexOf("draw"));
  });
});
