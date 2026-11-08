// モバイル版につながる条件を確かめる部品。tailnet の名前、tailscale serve の中継、人の待ち受けの 3 つを読む。
// serve の判定は movie-maker の mm-serve/src/mobile/tailnet.rs の `serve_proxies` と同じにする。
import type { MmMobileView } from '../shared/types.ts';
import { MOBILE_HTTPS_PORT, mobileUrl } from './movieMakerLink.ts';
import { hostsInTailscaleStatus, platform, type Run } from './platform.ts';

/** 人の待ち受け（127.0.0.1）のポート。movie-maker の MOBILE_PORT と同じ。 */
export const MOBILE_PORT = 7338;
const PROBE_TIMEOUT_MS = 1500;
const ENABLE_URL = 'https://login.tailscale.com/f/serve';

type ServeConfig = {
  TCP?: Record<string, { HTTPS?: boolean }>;
  Web?: Record<string, { Handlers?: Record<string, { Proxy?: string }> }>;
  Foreground?: Record<string, ServeConfig>;
};

/** 中継先がこの PC の port か（`http://127.0.0.1:7338`、`localhost:7338/`、`http://[::1]:7338` など）。 */
const isLocalPort = (proxy: string, port: number): boolean => {
  const rest = proxy.replace(/^http:\/\//, '').replace(/\/+$/, '');
  const at = rest.lastIndexOf(':');
  return at >= 0 && ['127.0.0.1', 'localhost', '[::1]'].includes(rest.slice(0, at)) && Number(rest.slice(at + 1)) === port;
};

const proxies = (c: ServeConfig, httpsPort: number, port: number): boolean => {
  // `--http=8443` の設定では Secure のクッキーと Service Worker が使えないので、HTTPS のときだけ数える。
  const https = c.TCP?.[String(httpsPort)]?.HTTPS === true;
  const here =
    https &&
    Object.entries(c.Web ?? {})
      .filter(([site]) => site.endsWith(`:${httpsPort}`))
      .flatMap(([, web]) => Object.values(web.Handlers ?? {}))
      .some((h) => typeof h.Proxy === 'string' && isLocalPort(h.Proxy, port));
  // `--bg` なしで動かしている間の設定は Foreground の下に入る。
  return here || Object.values(c.Foreground ?? {}).some((f) => proxies(f, httpsPort, port));
};

/** `tailscale serve status --json` の出力が、httpsPort の HTTPS を port へ中継しているか。何も設定していないと `{}`。読めなければ偽。 */
export const serveProxies = (json: string, httpsPort: number = MOBILE_HTTPS_PORT, port: number = MOBILE_PORT): boolean => {
  try {
    const config = JSON.parse(json) as ServeConfig | null;
    return !!config && typeof config === 'object' && proxies(config, httpsPort, port);
  } catch {
    return false;
  }
};

/** Serve と HTTPS 証明書を有効にする管理画面のリンク。`tailscale serve` が出すものと同じ形で、Self.ID（端末の安定した ID）から作る。 */
export const serveEnableUrl = (statusJson: string): string | undefined => {
  try {
    const id = (JSON.parse(statusJson) as { Self?: { ID?: unknown } }).Self?.ID;
    return typeof id === 'string' && id ? `${ENABLE_URL}?node=${encodeURIComponent(id)}` : undefined;
  } catch {
    return undefined;
  }
};

/** 人の待ち受けが HTTP で応えるか。応答の中身は問わず、つながれば真。 */
export const probeListening = async (port: number = MOBILE_PORT, timeoutMs: number = PROBE_TIMEOUT_MS): Promise<boolean> => {
  try {
    await fetch(`http://127.0.0.1:${port}/m/`, { signal: AbortSignal.timeout(timeoutMs), redirect: 'manual' });
    return true;
  } catch {
    return false;
  }
};

export type MobileDeps = { tailscale: (args: string[], run?: Run) => Promise<string>; probe: () => Promise<boolean> };
const realDeps: MobileDeps = { tailscale: (args, run) => platform.tailscale(args, run), probe: () => probeListening() };

/** モバイル版の 3 つの条件を読む。Tailscale が読めないときは serve の CLI も答えないので、serve は偽にする。 */
export const readMobile = async (deps: MobileDeps = realDeps): Promise<MmMobileView> => {
  const [status, serve, listening] = await Promise.all([
    deps.tailscale(['status', '--json', '--peers=false']).catch(() => undefined),
    deps.tailscale(['serve', 'status', '--json']).catch(() => undefined),
    deps.probe(),
  ]);
  const url = status ? mobileUrl(hostsInTailscaleStatus(status)) : undefined;
  return {
    tailscale: !!url,
    url,
    serve: !!url && !!serve && serveProxies(serve),
    enableUrl: status ? serveEnableUrl(status) : undefined,
    listening,
  };
};
