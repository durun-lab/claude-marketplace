// 通しの確認の PC の側。本物の mm-serve（crates/mm-testkit/examples/mobile_rig.rs）を起動し、
// tailscale serve の代わりの中継と、Claude の待ち受けの MCP を呼ぶ口を作る。
import { spawn } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import { request as httpRequest } from "node:http";
import { createServer as createHttpsServer } from "node:https";
import { join } from "node:path";
import process from "node:process";
import { createInterface } from "node:readline";
import { setTimeout } from "node:timers";

/** 試験の tailnet での名前と本人。mobile_rig の既定と合わせる。 */
export const TAILNET_HOST = "desk.tail1234.ts.net";
export const TAILNET_LOGIN = "me@example.com";
/** tailscale serve の HTTPS のポート。ブラウザーのオリジンはこれだと見なして Origin を書き換える。 */
const SERVE_HTTPS_PORT = 8443;
/** 人の待ち受けの決まったポート。設計（12-mobile.md）の既定。 */
export const MOBILE_PORT = 7338;
const READY_TIMEOUT_MS = 120_000;

/** 1 行の JSON を順に受ける読み手。 */
const lineReader = (stream) => {
  const queue = [];
  const waiters = [];
  createInterface({ input: stream }).on("line", (line) => {
    let value;
    try {
      value = JSON.parse(line);
    } catch {
      return;
    }
    const w = waiters.shift();
    if (w) w(value);
    else queue.push(value);
  });
  return () => (queue.length > 0 ? Promise.resolve(queue.shift()) : new Promise((r) => waiters.push(r)));
};

const withTimeout = (p, ms, what) =>
  Promise.race([p, new Promise((_, reject) => setTimeout(() => reject(new Error(`${what}: ${ms} ms で応答が無い`)), ms))]);

/**
 * mobile_rig を起動し、待ち受けが整うまで待つ。`LOCALAPPDATA` は `appData` に差し替え、
 * 利用者の本物のトークンと devices.json に触れない。
 */
export const startPc = async ({ exe, project, transcript, assets, workRoot, appData, port = MOBILE_PORT }) => {
  const work = await mkdtemp(join(workRoot, "rig-"));
  const args = ["--project", project, "--transcript", transcript, "--assets", assets, "--work", work, "--port", String(port)];
  const child = spawn(exe, args, { env: { ...process.env, LOCALAPPDATA: appData }, stdio: ["pipe", "pipe", "pipe"] });
  const logs = [];
  child.stderr.on("data", (d) => logs.push(String(d)));
  const next = lineReader(child.stdout);
  const done = new Promise((resolve) => child.on("exit", resolve));
  const exited = done.then((code) => Promise.reject(new Error(`mobile_rig が止まった（${code}）: ${logs.join("")}`)));
  // 起動の後に止まるのは stop の正常な終わりなので、拒否を拾わずに捨てる
  exited.catch(() => undefined);
  const ready = await withTimeout(Promise.race([next(), exited]), READY_TIMEOUT_MS, "mobile_rig の起動");
  const ask = async (command) => {
    child.stdin.write(`${command}\n`);
    return withTimeout(next(), READY_TIMEOUT_MS, `mobile_rig の ${command}`);
  };
  // 標準入力を閉じると mobile_rig は止まる。次の回が同じポートで待てるよう、止まるまで待つ
  const stop = async () => {
    child.stdin.end();
    await done;
  };
  return { ...ready, work, logs, ask, stop };
};

/** 中継が付け替える見出し。外から来た本人の見出しは tailscale serve と同じく捨てる。 */
const relayHeaders = (headers, selfOrigin) => {
  const out = Object.fromEntries(Object.entries(headers).filter(([k]) => !k.startsWith("tailscale-")));
  out.host = `${TAILNET_HOST}:${SERVE_HTTPS_PORT}`;
  out["tailscale-user-login"] = TAILNET_LOGIN;
  // ブラウザーから見たオリジンは https://<tailnet での名前>:8443 なので、同じオリジンの Origin だけを置き換える
  if (out.origin === selfOrigin) out.origin = `https://${out.host}`;
  return out;
};

/**
 * 中継の TLS の自己署名の証明書を openssl（Git for Windows に付く）で作る。WebKit は http の localhost に
 * `Secure` のクッキーを置かないので、tailscale serve と同じく HTTPS で受ける。
 */
export const makeCert = async (dir) => {
  const key = join(dir, "relay-key.pem");
  const cert = join(dir, "relay-cert.pem");
  const args = ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert, "-days", "1", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost"];
  await new Promise((resolve, reject) => {
    const p = spawn("openssl", args, { stdio: "ignore" });
    p.on("error", reject);
    p.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`openssl が失敗した（${code}）`))));
  });
  return { key: await readFile(key), cert: await readFile(cert) };
};

/** tailscale serve の代わりの中継。127.0.0.1 の空いているポートで HTTPS を受け、人の待ち受けへ http で流す。 */
export const startRelay = (mobilePort, tls) =>
  new Promise((resolve) => {
    let selfOrigin = "";
    const server = createHttpsServer(tls, (req, res) => {
      const upstream = httpRequest(
        { host: "127.0.0.1", port: mobilePort, method: req.method, path: req.url, headers: relayHeaders(req.headers, selfOrigin) },
        (up) => {
          res.writeHead(up.statusCode ?? 502, up.headers);
          up.pipe(res);
        },
      );
      upstream.on("error", () => res.destroy());
      req.pipe(upstream);
    });
    server.listen(0, "127.0.0.1", () => {
      selfOrigin = `https://localhost:${server.address().port}`;
      resolve({ origin: selfOrigin, close: () => server.close() });
    });
  });

/** Claude の待ち受けの MCP（Streamable HTTP、JSON 応答）を呼ぶ。 */
export const mcpClient = ({ claude_port: port, token }) => {
  let id = 0;
  const post = async (body) => {
    const res = await globalThis.fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json", Accept: "application/json, text/event-stream" },
      body: JSON.stringify(body),
    });
    return res.status === 202 ? null : res.json();
  };
  const call = async (name, args = {}) => {
    const reply = await post({ jsonrpc: "2.0", id: ++id, method: "tools/call", params: { name, arguments: args } });
    if (reply.error) throw new Error(`MCP ${name}: ${reply.error.message}`);
    const text = reply.result.content?.[0]?.text ?? "null";
    if (reply.result.isError) throw new Error(`MCP ${name}: ${text}`);
    return JSON.parse(text);
  };
  const initialize = async () => {
    const reply = await post({ jsonrpc: "2.0", id: ++id, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "mobile-e2e", version: "0" } } });
    await post({ jsonrpc: "2.0", method: "notifications/initialized" });
    return reply.result;
  };
  return { initialize, call };
};
