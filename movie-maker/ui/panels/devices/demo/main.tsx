// Devices パネルをモックの API で動かすページ。スクリーンショット用で、アプリには載せない。
// ?state=serve（tailscale serve が未設定）、?state=port（ポートが使われている）、?state=ok（既定）。
import { render } from "preact";
import type { Device, DevicesApi, MobileStatus, Notice, Pairing } from "../src/api.ts";
import { followTheme } from "../../../shell/src/followTheme.ts";
import { DevicesPanel } from "../src/index.ts";
import "../../../shell/src/shell.css";

const NOW_MS = 1793967600000;
const HOUR_MS = 3_600_000;
const SERVE_COMMAND = "tailscale serve --bg --https=8443 http://127.0.0.1:7338";
const URL_BASE = "https://desk.tail1234.ts.net:8443/m/";

/** QR に見せた市松模様（本物の QR はアプリが `qrcode` で作る）。 */
const fakeQr = (): string => {
  const cells = 21;
  const rects: string[] = [];
  for (let y = 0; y < cells; y++) for (let x = 0; x < cells; x++) if ((x * 7 + y * 13 + x * y) % 3 === 0) rects.push(`<rect x="${x}" y="${y}" width="1" height="1"/>`);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${cells} ${cells}" fill="#000">${rects.join("")}</svg>`;
};

const devices: Device[] = [
  { id: "d1", name: "Pixel 9", login: "me@example.com", paired_at_ms: NOW_MS - 26 * HOUR_MS },
  { id: "d2", name: "iPad", login: "me@example.com", paired_at_ms: NOW_MS - 3 * HOUR_MS },
];

const running = (notices: Notice[], url: string | null = URL_BASE): MobileStatus => ({
  state: "running",
  port: 7338,
  url,
  serveCommand: SERVE_COMMAND,
  notices,
});

const status = (): MobileStatus => {
  const which = new URLSearchParams(location.search).get("state");
  if (which === "port") return { state: "unavailable", message: "Port 7338 is already in use, so the phone listener could not start. Close the other movie-maker window or the program using the port, then restart movie-maker." };
  if (which === "serve") {
    return running([{ kind: "serveNotConfigured", message: "tailscale serve is not pointing at this app (port 7338) yet. Run the command below once in a terminal (not as administrator), then check again." }]);
  }
  if (which === "tailscale") {
    return running([{ kind: "noTailscale", message: "Tailscale is not running or not installed, so this PC's tailnet name is unknown. Install Tailscale and sign in, then restart movie-maker. Until then, a phone cannot reach this PC." }], null);
  }
  return running([]);
};

let list = [...devices];
const api: DevicesApi = {
  status: async () => status(),
  issueCode: async (): Promise<Pairing> => ({ code: "K7Q2M9XD", expiresAtMs: NOW_MS + 272_000, url: `${URL_BASE}pair?code=K7Q2M9XD`, qrSvg: fakeQr() }),
  list: async () => list,
  revoke: async (id) => {
    list = list.filter((d) => d.id !== id);
    return true;
  },
};

followTheme(document.documentElement, location.search, matchMedia.bind(window));
const root = document.getElementById("root");
if (root) {
  root.style.boxSizing = "border-box";
  root.style.flexDirection = "column";
  root.style.padding = "10px";
  root.style.width = "322px";
  root.style.height = "100vh";
  render(<DevicesPanel api={api} nowMs={NOW_MS} />, root);
}
