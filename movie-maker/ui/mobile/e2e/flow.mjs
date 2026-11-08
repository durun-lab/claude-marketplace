// 通しの確認の手順。12-mobile.md の主な流れ（コメント → Claude の propose → モバイルで承認）と、
// 登録、再生、Story、送信待ちのキューを、1 つの端末の設定で順に通す。
import assert from "node:assert/strict";
import { TAILNET_LOGIN } from "./pc.mjs";

/** 区間の符号化を待つ上限。区間は見られたときに PC の GPU で描いて符号化する。 */
const MEDIA_TIMEOUT_MS = 90_000;
const UI_TIMEOUT_MS = 30_000;
/** MPEG-TS の同期バイト。配信の区間の先頭がこれなら TS として読める。 */
const TS_SYNC_BYTE = 0x47;
/** 再生が進んだと見なす位置（秒）。 */
const PLAYED_SECONDS = 1;
const COMMENT_TEXT = "Make the caption easier to read.";
const OFFLINE_TEXT = "Written while offline.";
const PROPOSAL_NAME = "Fix the caption";

const item = (page, text) => page.locator(".list li").filter({ hasText: text });

const isPairPost = (route) => route.request().method() === "POST" && new globalThis.URL(route.request().url()).pathname === "/m/pair";

/**
 * 1a. 端末がクッキーを保存しないと、PC の登録だけが済む。画面が知らせることを確かめ、資格を使わなかった登録を残す
 * （次の手順が同じ名前で登録し直せることを確かめる）。`route.fetch` は文脈のクッキーに入れるので、応答の前に消す。
 */
export const pairWithoutCookie = async ({ page, context, pc, relay }) => {
  const { code } = await pc.ask("code");
  const dropCookie = async (route) => {
    if (!isPairPost(route)) return route.continue();
    const res = await route.fetch();
    await context.clearCookies();
    const headers = Object.fromEntries(Object.entries(res.headers()).filter(([k]) => k !== "set-cookie"));
    return route.fulfill({ response: res, headers });
  };
  await page.route("**/m/pair", dropCookie);
  await page.goto(`${relay.origin}/m/pair?code=${code}`);
  await page.getByRole("button", { name: "Pair" }).click();
  await page.getByRole("alert").filter({ hasText: "did not keep the sign-in cookie" }).waitFor({ timeout: UI_TIMEOUT_MS });
  await page.unroute("**/m/pair", dropCookie);
  const { devices } = await pc.ask("devices");
  assert.equal(devices.length, 1, "PC の登録は済んでいる");
  return "pairing without the cookie is reported on the screen";
};

/** 1b. PC の Devices の登録コード（`Devices::issue_code`）を QR の URL で開き、同じ名前で登録し直す。 */
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
 * プレイリストと先頭の配信の区間を、画面と同じ資格（クッキー）で取る。親のプレイリスト（index.m3u8）があれば
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
    return { variant, list: list.status, type: list.headers.get("content-type"), segments: uris.length, seg: seg.status, first: bytes[0], size: bytes.length };
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
  return `${s.variant ? `index.m3u8 -> ${s.variant}` : "360p.m3u8"}: ${s.segments} segments, first segment ${s.size} bytes, ${played}`;
};

/** なぞる点（映像の矩形の左上を (0, 0)、右下を (1, 1) にした座標）。 */
const STROKE = [[0.15, 0.7], [0.3, 0.75], [0.45, 0.8], [0.6, 0.78]];
/** 正規化した座標の許容の誤差。縦長の映像の矩形は幅が約 120 px なので、1 px は約 0.008。 */
const POINT_TOLERANCE = 0.02;

/**
 * 映像の矩形（`object-fit: contain` で映像の領域に収めた所）。コンポジションの大きさは `GET /m/api/project` から読む。
 * デスクトップの Program の `fit_rect` と同じ計算で、描き込みの座標はこの矩形を 0〜1 にしたもの。
 */
const videoRect = async (page) => {
  const box = await page.locator(".draw").boundingBox();
  const { width, height } = await page.evaluate(async () => (await (await globalThis.fetch("/m/api/project")).json()).compositions[0]);
  const scale = Math.min(box.width / width, box.height / height);
  const [w, h] = [width * scale, height * scale];
  return { x: box.x + (box.width - w) / 2, y: box.y + (box.height - h) / 2, width: w, height: h, comp: `${width}x${height}` };
};

/** 映像の上を指でなぞる。 */
const drawStroke = async (page, rect) => {
  const at = ([x, y]) => [rect.x + rect.width * x, rect.y + rect.height * y];
  await page.mouse.move(...at(STROKE[0]));
  await page.mouse.down();
  for (const p of STROKE.slice(1)) await page.mouse.move(...at(p), { steps: 4 });
  await page.mouse.up();
};

/** 2b. 止めたフレームに描き込みつきのコメントを書く。フィクスチャは縦長（360x640）なので、16:9 の映像の領域の左右にレターボックスの余白がある。 */
export const comment = async ({ page, pc, deviceName }) => {
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  await page.getByLabel("Comment text").fill(COMMENT_TEXT);
  await page.getByRole("button", { name: "Draw" }).click();
  const rect = await videoRect(page);
  await drawStroke(page, rect);
  // 縦長の映像の上に描いた所を、通った回も作業のフォルダーの横に残して目で確かめられるようにする。
  await page.screenshot({ path: `${pc.work}/../${deviceName}-draw.png` });
  await page.getByRole("button", { name: "Send" }).click();
  await item(page, COMMENT_TEXT).locator(".state.open").waitFor({ timeout: UI_TIMEOUT_MS });
  return `comment with a drawing is open (${rect.comp}, video ${Math.round(rect.width)}x${Math.round(rect.height)})`;
};

/** 描いた点が映像の矩形の座標で届いたこと。最初と最後の点は間引かれない。 */
const checkDrawing = (drawing) => {
  const points = drawing[0]?.points ?? [];
  assert.ok(points.length > 1, `描き込みが届く: ${JSON.stringify(drawing)}`);
  for (const [got, want] of [[points[0], STROKE[0]], [points.at(-1), STROKE.at(-1)]]) {
    const near = Math.abs(got[0] - want[0]) < POINT_TOLERANCE && Math.abs(got[1] - want[1]) < POINT_TOLERANCE;
    assert.ok(near, `映像の矩形の座標（デスクトップの Program と同じ）: ${JSON.stringify(got)} ≒ ${JSON.stringify(want)}`);
  }
};

/** 3. Claude の待ち受けの MCP で読み、responds_to つきで提案を出す。 */
export const claudeProposes = async ({ mcp, state }) => {
  await mcp.initialize();
  const { comments } = await mcp.call("comments", { state: "open" });
  const c = comments.find((x) => x.text === COMMENT_TEXT);
  assert.ok(c, "MCP の comments にモバイルのコメントが出る");
  assert.equal(c.author.kind, "mobile");
  checkDrawing(c.drawing);
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
  await page.locator(".compare img.show[alt=Before]").waitFor({ timeout: UI_TIMEOUT_MS });
};

/**
 * 短い動画の先頭を画面と同じ資格で取る。URL は提案の応答の `clip_url`（キーつき）。MP4 の先頭のボックス（ftyp）と、Range への 206。
 * 作っていないキーの URL は 404（古いキーは今のキーへリダイレクトしない）。
 */
const fetchClip = (page, id) =>
  page.evaluate(async (pid) => {
    const detail = await (await globalThis.fetch(`/m/api/proposals/${pid}`)).json();
    const res = await globalThis.fetch(detail.clip_url, { headers: { Range: "bytes=0-7" } });
    const bytes = new Uint8Array(await res.arrayBuffer());
    const stale = await globalThis.fetch(`/m/api/proposals/${pid}/clip/0123456789abcdef.mp4`);
    return { url: detail.clip_url, status: res.status, type: res.headers.get("content-type"), box: String.fromCharCode(...bytes.slice(4, 8)), stale: stale.status };
  }, id);

/**
 * 提案の区間の短い動画が MP4 で取れること。Chromium は Play で再生が進むことまで。
 * WebKit の Windows 版は H.264 を再生できないので、押すと作っている間の表示が出て、失敗の表示で終わることまで。
 */
const checkClip = async (page, browserName, id) => {
  const c = await fetchClip(page, id);
  assert.equal(c.status, 206);
  assert.equal(c.type, "video/mp4");
  assert.equal(c.box, "ftyp");
  assert.match(c.url, /\/clip\/[0-9a-f]{16}\.mp4\?comp=main$/);
  assert.equal(c.stale, 404, "作っていないキーは 404");
  await page.getByRole("button", { name: "Play", exact: true }).click();
  assert.equal(await page.locator(".compare video").getAttribute("src"), c.url, "Play は提案の応答の URL を読む");
  if (browserName !== "chromium") {
    await page.locator(".clip-note").first().waitFor({ timeout: MEDIA_TIMEOUT_MS });
    await page.getByRole("button", { name: "After", exact: true }).click();
    return "keyed clip answered 206, unknown key 404 (WebKit on Windows cannot decode H.264)";
  }
  await page.waitForFunction((min) => (globalThis.document.querySelector(".compare video.on")?.currentTime ?? 0) > min, PLAYED_SECONDS, { timeout: MEDIA_TIMEOUT_MS });
  await page.getByRole("button", { name: "After", exact: true }).click();
  return "keyed clip played, unknown key 404";
};

/** 4. モバイルの Proposals で前後を見て、短い動画を再生して承認する。別の画面（同じ端末）に SSE でコメントの fresh が届く。 */
export const approve = async ({ page, context, relay, mcp, state, browserName }) => {
  const watcher = await context.newPage();
  await watcher.goto(`${relay.origin}/m/`);
  await item(watcher, COMMENT_TEXT).locator(".state.open").waitFor({ timeout: UI_TIMEOUT_MS });
  await page.getByRole("tab", { name: "Proposals" }).click();
  await item(page, PROPOSAL_NAME).click();
  await checkFrames(page);
  const clip = await checkClip(page, browserName, state.proposalId);
  await page.getByRole("button", { name: /^Approve/ }).click();
  await page.getByRole("status").filter({ hasText: `Approved ${PROPOSAL_NAME}` }).waitFor({ timeout: UI_TIMEOUT_MS });
  await item(watcher, COMMENT_TEXT).locator(".state.fresh").waitFor({ timeout: UI_TIMEOUT_MS });
  await watcher.close();
  const c = await mcp.call("comments", { id: state.commentId });
  assert.equal(c.state, "fresh");
  assert.ok(c.responded_txn, "対応したトランザクションが付く");
  const { proposals } = await mcp.call("proposals");
  assert.equal(proposals.find((p) => p.id === state.proposalId)?.state, "approved");
  return `${clip}, approved on mobile, comment turned fresh over SSE`;
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

/** IndexedDB の送信待ちのキューの本文が `texts` になるまで待つ。画面を更新してから保存するので、画面の後に揃う。 */
const waitForIdbQueue = (page, texts) =>
  page.waitForFunction(
    (want) =>
      new Promise((resolve, reject) => {
        const req = globalThis.indexedDB.open("mm-mobile");
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const all = req.result.transaction("outbox", "readonly").objectStore("outbox").getAll();
          all.onsuccess = () => {
            req.result.close();
            resolve(JSON.stringify(all.result.map((c) => c.text)) === want);
          };
        };
      }),
    JSON.stringify(texts),
    { timeout: UI_TIMEOUT_MS },
  );

/** 6. 回線を切って書くと送信待ちのキュー（IndexedDB）に入り、つながったら送られる。 */
export const offline = async ({ page, context, mcp }) => {
  await page.getByRole("tab", { name: "Watch" }).click();
  await context.setOffline(true);
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  await page.getByLabel("Comment text").fill(OFFLINE_TEXT);
  await page.getByRole("button", { name: "Send" }).click();
  await page.locator(".item.pending").filter({ hasText: OFFLINE_TEXT }).waitFor({ timeout: UI_TIMEOUT_MS });
  await waitForIdbQueue(page, [OFFLINE_TEXT]);
  const before = await mcp.call("comments", {});
  assert.ok(!before.comments.some((c) => c.text === OFFLINE_TEXT), "切れている間は PC に届かない");
  await context.setOffline(false);
  await item(page, OFFLINE_TEXT).locator(".state.open").waitFor({ timeout: UI_TIMEOUT_MS });
  await waitForIdbQueue(page, []);
  const after = await mcp.call("comments", {});
  const sent = after.comments.filter((c) => c.text === OFFLINE_TEXT);
  assert.equal(sent.length, 1, "再送で 2 件にならない");
  assert.ok(sent[0].client_id, "送信待ちのキューの client_id が付く");
  return "queued in IndexedDB while offline, sent once after reconnecting";
};

/** 順に通す手順。 */
export const STEPS = [
  ["1 pair without cookie", pairWithoutCookie],
  ["1 pair", pair],
  ["2 watch", watch],
  ["2 comment", comment],
  ["3 claude proposes", claudeProposes],
  ["4 approve on mobile", approve],
  ["5 story", story],
  ["6 offline queue", offline],
];
