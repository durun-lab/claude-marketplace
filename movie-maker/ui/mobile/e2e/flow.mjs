// 通しの確認の手順。12-mobile.md の主な流れ（コメント → Claude の propose → モバイルで承認）と、
// 登録、再生、Story、送信待ちの列を、1 つの端末の設定で順に通す。
import assert from "node:assert/strict";
import { TAILNET_LOGIN } from "./pc.mjs";

/** 区間の符号化を待つ上限。区間は見られたときに PC の GPU で描いて符号化する。 */
const MEDIA_TIMEOUT_MS = 90_000;
const UI_TIMEOUT_MS = 30_000;
/** MPEG-TS の同期バイト。区間の先頭がこれなら TS として読める。 */
const TS_SYNC_BYTE = 0x47;
/** 再生が進んだと見なす位置（秒）。 */
const PLAYED_SECONDS = 1;
const COMMENT_TEXT = "Make the caption easier to read.";
const OFFLINE_TEXT = "Written while offline.";
const PROPOSAL_NAME = "Fix the caption";

const item = (page, text) => page.locator(".list li").filter({ hasText: text });

/** 1. PC の Devices の符号（`Devices::issue_code`）を QR の URL で開き、登録する。 */
export const pair = async ({ page, context, pc, relay, deviceName }) => {
  const { code, url } = await pc.ask("code");
  assert.match(url, /\/m\/pair\?code=/);
  await page.goto(`${relay.origin}/m/pair?code=${code}`);
  await assert.doesNotReject(page.getByLabel("Pairing code").waitFor());
  assert.equal(await page.getByLabel("Pairing code").inputValue(), code);
  assert.equal(await page.getByLabel("Device name").inputValue(), deviceName);
  await page.getByRole("button", { name: "Pair" }).click();
  await page.getByRole("tablist").waitFor({ timeout: UI_TIMEOUT_MS });
  await page.getByRole("status").filter({ hasText: "Connected" }).waitFor({ timeout: UI_TIMEOUT_MS });
  const { devices } = await pc.ask("devices");
  assert.equal(devices.length, 1);
  assert.equal(devices[0].name, deviceName);
  assert.equal(devices[0].login, TAILNET_LOGIN);
  const cookie = (await context.cookies()).find((c) => c.name === "mm_device");
  assert.ok(cookie?.httpOnly && cookie.path === "/m", "端末の資格は HttpOnly でパスが /m のクッキー");
  return `paired as ${deviceName}`;
};

/**
 * プレイリストと先頭の区間を、画面と同じ資格（クッキー）で取る。親のプレイリスト（index.m3u8）があれば
 * 最初の画質の子をたどり、無ければ画質ごとのプレイリストを直接読む。
 */
const fetchStream = (page) =>
  page.evaluate(async () => {
    const base = "/m/api/stream/main/";
    const master = await globalThis.fetch(`${base}index.m3u8`);
    const masterText = master.ok ? await master.text() : "";
    const variant = masterText.includes("#EXT-X-STREAM-INF") ? masterText.split("\n").find((l) => l && !l.startsWith("#")) : null;
    const list = await globalThis.fetch(variant ? new globalThis.URL(variant, master.url) : `${base}360p.m3u8`);
    const text = await list.text();
    const uris = text.split("\n").filter((l) => l && !l.startsWith("#"));
    const seg = await globalThis.fetch(new globalThis.URL(uris[0], list.url));
    const bytes = new Uint8Array(await seg.arrayBuffer());
    return { list: list.status, type: list.headers.get("content-type"), segments: uris.length, seg: seg.status, first: bytes[0], size: bytes.length };
  });

/** 2a. Watch の映像。区間が TS で取れること。Chromium は hls.js で再生が進むことまで。 */
export const watch = async ({ page, browserName, hlsRequests }) => {
  const s = await fetchStream(page);
  assert.equal(s.list, 200);
  assert.match(s.type ?? "", /mpegurl/i);
  assert.ok(s.segments >= 2, "8 秒のコンポジションは 4 秒の区間で 2 本以上");
  assert.equal(s.seg, 200);
  assert.equal(s.first, TS_SYNC_BYTE);
  let played = "not played (WebKit on Windows cannot decode HLS)";
  if (browserName === "chromium") {
    await page.getByRole("button", { name: "Play" }).click();
    await page.waitForFunction((min) => (globalThis.document.querySelector("video")?.currentTime ?? 0) > min, PLAYED_SECONDS, { timeout: MEDIA_TIMEOUT_MS });
    await page.locator("video").evaluate((v) => v.pause());
    assert.ok(hlsRequests.length > 0, "Chromium は hls.js の塊を読む");
    played = "played with hls.js";
  }
  return `playlist ${s.segments} segments, first segment ${s.size} bytes, ${played}`;
};

/** 映像の上を指でなぞる。 */
const drawStroke = async (page) => {
  const box = await page.locator(".draw").boundingBox();
  const at = (x, y) => [box.x + box.width * x, box.y + box.height * y];
  await page.mouse.move(...at(0.15, 0.7));
  await page.mouse.down();
  for (const [x, y] of [[0.3, 0.75], [0.45, 0.8], [0.6, 0.78]]) await page.mouse.move(...at(x, y), { steps: 4 });
  await page.mouse.up();
};

/** 2b. 止めたフレームに描き込みつきのコメントを書く。 */
export const comment = async ({ page }) => {
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  await page.getByLabel("Comment text").fill(COMMENT_TEXT);
  await page.getByRole("button", { name: "Draw" }).click();
  await drawStroke(page);
  await page.getByRole("button", { name: "Send" }).click();
  await item(page, COMMENT_TEXT).locator(".state.open").waitFor({ timeout: UI_TIMEOUT_MS });
  return "comment with a drawing is open";
};

/** 3. Claude の待ち受けの MCP で読み、responds_to つきで提案を出す。 */
export const claudeProposes = async ({ mcp, state }) => {
  await mcp.initialize();
  const { comments } = await mcp.call("comments", { state: "open" });
  const c = comments.find((x) => x.text === COMMENT_TEXT);
  assert.ok(c, "MCP の comments にモバイルのコメントが出る");
  assert.equal(c.author.kind, "mobile");
  assert.ok(c.drawing.length > 0 && c.drawing[0].points.length > 1, `描き込みが届く: ${JSON.stringify(c.drawing)}`);
  const ops = [
    { op: "setParam", target: { layer: "l-caption", path: "source.text" }, value: { type: "text", value: "Caption fixed" } },
    { op: "setParam", target: { layer: "l-caption", path: "source.color" }, value: { type: "color", value: { r: 0.4, g: 1, b: 0.6, a: 1 } } },
  ];
  const proposal = await mcp.call("propose", { name: PROPOSAL_NAME, ops, responds_to: c.id });
  state.commentId = c.id;
  state.proposalId = proposal.id;
  return `proposal ${proposal.id} responds to ${c.id}`;
};

/** 前後のフレームが読み込まれ、後のフレームが提案を適用したものであること。 */
const checkFrames = async (page) => {
  const imgs = page.locator(".compare img");
  await page.waitForFunction(() => [...globalThis.document.querySelectorAll(".compare img")].every((i) => i.complete && i.naturalWidth > 0), null, { timeout: MEDIA_TIMEOUT_MS });
  assert.match((await imgs.nth(1).getAttribute("src")) ?? "", /proposal=/);
  await page.getByRole("button", { name: "Before", exact: true }).click();
  assert.match((await imgs.nth(0).getAttribute("class")) ?? "", /show/);
};

/** 4. モバイルの Proposals で前後を見て承認する。別の画面（同じ端末）に SSE でコメントの fresh が届く。 */
export const approve = async ({ page, context, relay, mcp, state }) => {
  const watcher = await context.newPage();
  await watcher.goto(`${relay.origin}/m/`);
  await item(watcher, COMMENT_TEXT).locator(".state.open").waitFor({ timeout: UI_TIMEOUT_MS });
  await page.getByRole("tab", { name: "Proposals" }).click();
  await item(page, PROPOSAL_NAME).click();
  await checkFrames(page);
  await page.getByRole("button", { name: /^Approve/ }).click();
  await page.getByRole("status").filter({ hasText: `Approved ${PROPOSAL_NAME}` }).waitFor({ timeout: UI_TIMEOUT_MS });
  await item(watcher, COMMENT_TEXT).locator(".state.fresh").waitFor({ timeout: UI_TIMEOUT_MS });
  await watcher.close();
  const c = await mcp.call("comments", { id: state.commentId });
  assert.equal(c.state, "fresh");
  assert.ok(c.responded_txn, "対応したトランザクションが付く");
  const { proposals } = await mcp.call("proposals");
  assert.equal(proposals.find((p) => p.id === state.proposalId)?.state, "approved");
  return "approved on mobile, comment turned fresh over SSE";
};

/** 単語を 2 回押して範囲を選ぶ。 */
const pickRange = async (page, from, to) => {
  const words = page.locator(".st-word");
  await words.nth(from).click();
  await words.nth(to).click();
};

/** 5. Story で範囲を選んで Keep（即時）と、「PC で決める」で提案。 */
export const story = async ({ page, mcp }) => {
  await page.getByRole("tab", { name: "Story" }).click();
  await page.locator(".st-assets button:not([disabled])").first().click();
  await page.locator(".st-word").nth(9).waitFor({ timeout: UI_TIMEOUT_MS });
  await pickRange(page, 0, 3);
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await page.getByText("Keep done.").waitFor({ timeout: UI_TIMEOUT_MS });
  const { project } = await mcp.call("project");
  assert.ok(JSON.stringify(project).includes('"favorite"'), "Keep で話題に favorite が付く");
  await page.getByLabel("Decide on PC").check();
  await pickRange(page, 6, 9);
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await page.getByText("Sent to the PC as a proposal.").waitFor({ timeout: UI_TIMEOUT_MS });
  const { proposals } = await mcp.call("proposals");
  const sent = proposals.find((p) => p.state === "pending");
  assert.ok(sent, "PC で決める提案が保留で届く");
  assert.equal(sent.actor, "mobile");
  return `kept a range now, proposal ${sent.id} waits on the PC`;
};

const queuedInIdb = (page) =>
  page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const req = globalThis.indexedDB.open("mm-mobile");
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const all = req.result.transaction("outbox", "readonly").objectStore("outbox").getAll();
          all.onsuccess = () => resolve(all.result.map((c) => c.text));
        };
      }),
  );

/** 6. 回線を切って書くと送信待ちの列（IndexedDB）に入り、つながったら送られる。 */
export const offline = async ({ page, context, mcp }) => {
  await page.getByRole("tab", { name: "Watch" }).click();
  await context.setOffline(true);
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  await page.getByLabel("Comment text").fill(OFFLINE_TEXT);
  await page.getByRole("button", { name: "Send" }).click();
  await page.locator(".item.pending").filter({ hasText: OFFLINE_TEXT }).waitFor({ timeout: UI_TIMEOUT_MS });
  assert.deepEqual(await queuedInIdb(page), [OFFLINE_TEXT]);
  const before = await mcp.call("comments", {});
  assert.ok(!before.comments.some((c) => c.text === OFFLINE_TEXT), "切れている間は PC に届かない");
  await context.setOffline(false);
  await item(page, OFFLINE_TEXT).locator(".state.open").waitFor({ timeout: UI_TIMEOUT_MS });
  assert.deepEqual(await queuedInIdb(page), []);
  const after = await mcp.call("comments", {});
  const sent = after.comments.filter((c) => c.text === OFFLINE_TEXT);
  assert.equal(sent.length, 1, "再送で 2 件にならない");
  assert.ok(sent[0].client_id, "送信待ちの列の client_id が付く");
  return "queued in IndexedDB while offline, sent once after reconnecting";
};

/** 順に通す手順。 */
export const STEPS = [
  ["1 pair", pair],
  ["2 watch", watch],
  ["2 comment", comment],
  ["3 claude proposes", claudeProposes],
  ["4 approve on mobile", approve],
  ["5 story", story],
  ["6 offline queue", offline],
];
