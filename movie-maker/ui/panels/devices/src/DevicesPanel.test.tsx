// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Device, DevicesApi, MobileStatus, Pairing } from "./api.ts";
import { createIpcDevicesApi } from "./api.ts";
import { DevicesPanel } from "./DevicesPanel.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const NOW = 1_793_967_600_000;
const RUNNING: MobileStatus = {
  state: "running",
  port: 7338,
  url: "https://desk.tail1234.ts.net:8443/m/",
  serveCommand: "tailscale serve --bg --https=8443 http://127.0.0.1:7338",
  notices: [],
};
const PHONE: Device = { id: "d1", name: "Pixel", login: "me@example.com", paired_at_ms: NOW - 3_600_000 };
const PAIRING: Pairing = { code: "ABCD2345", expiresAtMs: NOW + 272_000, url: "https://x/m/pair?code=ABCD2345", qrSvg: "<svg id='qr'></svg>" };

const fakeApi = (status: MobileStatus, devices: Device[] = []): DevicesApi => {
  let list = [...devices];
  return {
    status: vi.fn(async () => status),
    issueCode: vi.fn(async () => PAIRING),
    list: vi.fn(async () => [...list]),
    revoke: vi.fn(async (id: string) => {
      list = list.filter((d) => d.id !== id);
      return true;
    }),
  };
};

/** 状態と一覧を続けて読むので、タイマーを 1 周待って描き直しを終わらせる。 */
const flush = () => act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
const mount = async (api: DevicesApi) => {
  act(() => render(<DevicesPanel api={api} nowMs={NOW} />, host));
  await flush();
};
const click = async (el: Element | null | undefined) => {
  act(() => (el as HTMLElement).click());
  await flush();
};
const button = (label: string) => [...host.querySelectorAll("button")].find((b) => b.textContent === label);

describe("DevicesPanel", () => {
  it("端末の名前と登録した日時を一覧に出す", async () => {
    await mount(fakeApi(RUNNING, [PHONE]));
    const row = host.querySelector(".mm-devices-row");
    expect(row?.textContent).toContain("Pixel");
    expect(row?.textContent).toContain("Paired 2026-");
  });

  it("端末を足すと QR と登録コードと期限の残りを出す", async () => {
    await mount(fakeApi(RUNNING));
    await click(button("Add device"));
    expect(host.querySelector("#qr")).not.toBeNull();
    expect(host.querySelector(".mm-devices-code")?.textContent).toBe("ABCD 2345");
    expect(host.textContent).toContain("Expires in 4:32");
  });

  it("登録を外すは確かめてから外し、外した端末は一覧から消える", async () => {
    const api = fakeApi(RUNNING, [PHONE]);
    await mount(api);
    await click(button("Unpair"));
    expect(api.revoke).not.toHaveBeenCalled();
    await click(button("Unpair Pixel"));
    expect(api.revoke).toHaveBeenCalledWith("d1");
    expect(host.querySelector(".mm-devices-row")).toBeNull();
    expect(host.textContent).toContain("No devices yet.");
  });

  it("ポートが使われているときは理由を出し、端末を足せない", async () => {
    await mount(fakeApi({ state: "unavailable", message: "Port 7338 is already in use" }));
    expect(host.querySelector("[role=alert]")?.textContent).toContain("Port 7338 is already in use");
    expect(button("Add device")).toBeUndefined();
  });

  it("tailscale serve が未設定なら文言とコマンドを出す", async () => {
    const status: MobileStatus = {
      ...RUNNING,
      notices: [{ kind: "serveNotConfigured", message: "tailscale serve is not pointing at this app" }],
    };
    await mount(fakeApi(status));
    expect(host.textContent).toContain("tailscale serve is not pointing at this app");
    expect(host.querySelector(".mm-devices-command")?.textContent).toBe(RUNNING.serveCommand);
  });
});

describe("DevicesPanel の登録を外す確かめ", () => {
  it("確かめている間は Cancel にフォーカスを置き、Escape でやめて Unpair に戻す", async () => {
    const api = fakeApi(RUNNING, [PHONE]);
    await mount(api);
    await click(button("Unpair"));
    expect(document.activeElement).toBe(button("Cancel"));
    act(() => {
      button("Cancel")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    await flush();
    expect(button("Unpair Pixel")).toBeUndefined();
    expect(document.activeElement).toBe(button("Unpair"));
    expect(api.revoke).not.toHaveBeenCalled();
  });
});

describe("createIpcDevicesApi", () => {
  it("コマンドの名前と引数で呼び、失敗は Error にする", async () => {
    const call = vi.fn(async (command: string) => {
      if (command === "devices_revoke") throw "nope";
      return [];
    });
    const api = createIpcDevicesApi(call);
    await api.list();
    expect(call).toHaveBeenCalledWith("devices_list", undefined);
    await expect(api.revoke("d1")).rejects.toThrow("nope");
    expect(call).toHaveBeenLastCalledWith("devices_revoke", { id: "d1" });
  });
});
