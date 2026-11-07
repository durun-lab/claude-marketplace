// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../App.tsx";
import { createController } from "../controller.ts";
import { createMockServer } from "../mock/mockServer.ts";
import { createMemoryStore } from "../outbox/store.ts";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const must = <T,>(v: T | null | undefined): T => {
  if (v == null) throw new Error("element not found");
  return v;
};
const settle = () => act(() => new Promise<void>((r) => setTimeout(r, 0)));
const click = (el: Element) => act(() => void el.dispatchEvent(new MouseEvent("click", { bubbles: true })));
const buttonByText = (text: string): HTMLButtonElement =>
  must([...host.querySelectorAll("button")].find((b) => b.textContent?.trim() === text));
const wordButton = (text: string): HTMLButtonElement =>
  must([...host.querySelectorAll<HTMLButtonElement>(".st-word")].find((b) => b.textContent === text));

/** Story のタブで 1 つ目の素材を開いた状態。 */
const openStory = async () => {
  const server = createMockServer({ paired: true, online: true });
  const controller = createController({
    ...server.platform,
    onNetworkBack: () => undefined,
    outboxStore: createMemoryStore(),
  });
  await controller.connection.start();
  act(() => render(<App controller={controller} initialTab="Story" />, host));
  await settle();
  await click(must(host.querySelector(".st-assets button:not(:disabled)")));
  await settle();
  return server;
};

/** 先頭の文の "today" から "talk" までの範囲を 2 回押しで選ぶ。 */
const pickRange = async () => {
  await click(wordButton("today"));
  await click(wordButton("talk"));
};

describe("Story", () => {
  it("単語を 2 回押すと範囲になり、下の棒に選んだ数が出る", async () => {
    await openStory();
    await click(wordButton("today"));
    expect(host.querySelector(".st-hint")?.textContent).toContain("another word");
    await click(wordButton("talk"));
    expect(host.querySelector(".st-hint")?.textContent).toBe("5 words selected");
    expect(host.querySelectorAll('.st-word[data-selected="true"]').length).toBe(5);
  });

  it("Keep は重なる話題の評価を base_version つきの 1 つのトランザクションで送る", async () => {
    const server = await openStory();
    await pickRange();
    await click(buttonByText("Keep"));
    await settle();
    const txn = must(server.story.applied.at(-1));
    expect(txn.base_version).toBe(12);
    expect(txn.ops).toEqual([{ op: "rate", target: "t1", rating: "favorite" }]);
    expect(host.querySelector(".st-notice")?.textContent).toContain("Undo");
  });

  it("話題の外の範囲に Hold すると、話題を足して評価を付ける", async () => {
    const server = await openStory();
    await click(wordButton("Then"));
    await click(wordButton("gold."));
    const before = server.story.topics.length;
    await click(buttonByText("Hold"));
    await settle();
    const ops = must(server.story.applied.at(-1)).ops;
    expect(ops.map((o) => o.op)).toEqual(["addTopic", "rate"]);
    expect(server.story.topics.length).toBe(before + 1);
  });

});

describe("Story の確定の操作", () => {
  it("Delete は deleteWords を送り、Undo は /m/api/undo を叩く", async () => {
    const server = await openStory();
    await pickRange();
    await click(buttonByText("Delete"));
    await settle();
    expect(must(server.story.applied.at(-1)).ops[0]).toMatchObject({ op: "deleteWords", restore: false });
    await click(buttonByText("Undo"));
    await settle();
    expect(server.writes.at(-1)?.path).toBe("/m/api/undo");
  });

  it("「PC で決める」を入れると適用せず提案として送る", async () => {
    const server = await openStory();
    await pickRange();
    await click(must(host.querySelector<HTMLInputElement>(".st-pc input")));
    await click(buttonByText("Keep"));
    await settle();
    expect(server.story.proposals).toHaveLength(1);
    expect(server.story.applied).toHaveLength(0);
    expect(host.querySelector(".st-notice")?.textContent).toContain("proposal");
    expect(host.querySelector(".st-notice")?.textContent).not.toContain("Undo");
  });

});

describe("Story の衝突と並べ替え", () => {
  it("バージョンが古くて 409 になったら画面に出して選択を外し、再送しない", async () => {
    const server = await openStory();
    await pickRange();
    server.story.conflictNext();
    await click(buttonByText("Keep"));
    await settle();
    expect(host.querySelector(".st-notice")?.textContent).toContain("changed on the PC");
    expect(server.story.applied).toHaveLength(0);
    expect(host.querySelector(".st-hint")?.textContent).toContain("Tap a word");
  });

  it("Topic は選んだ範囲の時刻で addTopic を送る", async () => {
    const server = await openStory();
    await pickRange();
    await click(buttonByText("Topic"));
    await settle();
    const op = must(server.story.applied.at(-1)).ops[0];
    expect(op).toMatchObject({ op: "addTopic", topic: { asset: "a1", in: { num: 500, den: 1000 }, out: { num: 3000, den: 1000 } } });
  });

  it("並べ替えのボタンで順を変えて差し込むと insertTopics を送る", async () => {
    const server = await openStory();
    await click(buttonByText("Arrange"));
    expect(host.querySelectorAll(".st-row").length).toBe(1);
    await click(buttonByText("Insert 1 topic"));
    await settle();
    expect(must(server.story.applied.at(-1)).ops[0]).toMatchObject({ op: "insertTopics", sequence: "s1", topics: ["t1"] });
  });
});
