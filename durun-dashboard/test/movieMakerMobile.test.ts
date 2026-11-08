import assert from 'node:assert/strict';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { test } from 'node:test';
import { mmSteps, mobileSteps } from '../client/mmSteps.ts';
import { probeListening, readMobile, serveEnableUrl, serveProxies, waitListening } from '../server/movieMakerMobile.ts';
import { SERVE_COMMAND } from '../shared/mmMobile.ts';
import type { MmMobileView, MovieMakerView } from '../shared/types.ts';

const STATUS = JSON.stringify({ BackendState: 'Running', Self: { ID: 'n4Xzt4BkWT11CNTRL', DNSName: 'core.tailf03e44.ts.net.', TailscaleIPs: ['100.1.2.3'] } });

/** tailscale 1.104 で `serve --bg --https=8443 http://127.0.0.1:7338` を足した後の `serve status --json`。 */
const SERVE_OK = JSON.stringify({
  TCP: { '443': { HTTPS: true }, '8443': { HTTPS: true } },
  Web: {
    'core.tailf03e44.ts.net:443': { Handlers: { '/': { Proxy: 'http://127.0.0.1:7337' } } },
    'core.tailf03e44.ts.net:8443': { Handlers: { '/': { Proxy: 'http://127.0.0.1:7338' } } },
  },
});
/** 今日のこの PC の状態。7337 と 8765 の http だけで、8443 が無い。 */
const SERVE_WITHOUT = JSON.stringify({
  TCP: { '7337': { HTTP: true }, '8765': { HTTP: true } },
  Web: {
    'core.tailf03e44.ts.net:7337': { Handlers: { '/': { Proxy: 'http://127.0.0.1:7337' } } },
    'core.tailf03e44.ts.net:8765': { Handlers: { '/': { Proxy: 'http://127.0.0.1:8765' } } },
  },
});

test('serve の判定は、8443 の HTTPS が 127.0.0.1:7338 へ向くときだけ真で、未設定、別の中継先、http、Foreground の設定も正しく扱う', () => {
  assert.equal(serveProxies(SERVE_OK), true);
  assert.equal(serveProxies(SERVE_WITHOUT), false);
  assert.equal(serveProxies('{}'), false);
  assert.equal(serveProxies('not json'), false);
  assert.equal(serveProxies('null'), false);
  assert.equal(serveProxies(SERVE_OK.replace('7338', '9999')), false);
  assert.equal(serveProxies(SERVE_OK.replace('"8443":{"HTTPS":true}', '"8443":{"HTTP":true}')), false);
  assert.equal(serveProxies(SERVE_OK.replace('http://127.0.0.1:7338', 'localhost:7338/')), true);
  assert.equal(serveProxies(JSON.stringify({ Foreground: { s1: JSON.parse(SERVE_OK) } })), true);
});

test('Serve が有効でない tailnet の案内は、tailscale status の Self.ID から f/serve のリンクを作り、読めなければ作らない', () => {
  assert.equal(serveEnableUrl(STATUS), 'https://login.tailscale.com/f/serve?node=n4Xzt4BkWT11CNTRL');
  assert.equal(serveEnableUrl('{}'), undefined);
  assert.equal(serveEnableUrl('not json'), undefined);
});

test('人の待ち受けは、HTTP に応えれば中身を問わず真で、つながらなければ偽', async () => {
  const server = http.createServer((_, res) => res.writeHead(404).end());
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const { port } = server.address() as AddressInfo;
  assert.equal(await probeListening(port, 1000), true);
  await new Promise((r) => server.close(r));
  assert.equal(await probeListening(port, 1000), false);
});

const deps = (outputs: Record<string, string | undefined>, listening: boolean) => ({
  tailscale: async (args: string[]) => {
    const out = outputs[args[0] ?? ''];
    if (out === undefined) throw new Error('tailscale が動いていません');
    return out;
  },
  probe: async () => listening,
});

test('3 つの条件をまとめて読み、Tailscale が読めなければ serve も偽にする', async () => {
  assert.deepEqual(await readMobile(deps({ status: STATUS, serve: SERVE_OK }, true)), { tailscale: true, url: 'https://core.tailf03e44.ts.net:8443/m/', serve: true, enableUrl: 'https://login.tailscale.com/f/serve?node=n4Xzt4BkWT11CNTRL', listening: true });
  const noServe = await readMobile(deps({ status: STATUS, serve: SERVE_WITHOUT }, false));
  assert.deepEqual([noServe.tailscale, noServe.serve, noServe.listening], [true, false, false]);
  const noTailscale = await readMobile(deps({}, true));
  assert.deepEqual(noTailscale, { tailscale: false, url: undefined, serve: false, enableUrl: undefined, listening: true });
  const garbled = await readMobile(deps({ status: 'Warning: not json', serve: SERVE_OK }, true));
  assert.deepEqual([garbled.tailscale, garbled.serve], [false, false]);
});

test('movie-maker とつながった直後は、人の待ち受けが開くまで待ち、時間を過ぎたら偽にする', async () => {
  let calls = 0;
  const opensOnThird = async () => ++calls >= 3;
  assert.equal(await waitListening(opensOnThird, 1000, 1), true);
  assert.equal(calls, 3);
  assert.equal(await waitListening(async () => false, 20, 5), false);
  const waited = await readMobile({ ...deps({ status: STATUS, serve: SERVE_OK }, false), probe: async () => ++calls >= 5 }, true);
  assert.equal(waited.listening, true);
});

const view = (over: Partial<MovieMakerView> = {}, mobile?: MmMobileView): MovieMakerView => ({
  install: { path: 'C:/mm/movie-maker.exe', mode: 'build', commit: 'abc', sourceCommit: 'abc', busy: false, output: [] },
  project: 'D:/videos/ep04',
  recent: [],
  hasStore: true,
  mobile,
  link: { status: 'connected' },
  log: [],
  ...over,
});
const OK: MmMobileView = { tailscale: true, url: 'https://core.tailf03e44.ts.net:8443/m/', serve: true, enableUrl: 'https://login.tailscale.com/f/serve?node=n1', listening: true };

test('モバイル版のガイドは、3 つとも満たすと空で、満たさない項目だけを手順にする', () => {
  assert.deepEqual(mobileSteps(view({}, OK), OK), []);

  const noTailscale = { ...OK, tailscale: false, url: undefined, serve: false, enableUrl: undefined };
  assert.deepEqual(mobileSteps(view({}, noTailscale), noTailscale).map((s) => s.key), ['mobile-tailscale']);

  const noServe = mobileSteps(view({}, { ...OK, serve: false }), { ...OK, serve: false });
  assert.deepEqual(noServe.map((s) => s.key), ['mobile-serve']);
  assert.equal(noServe[0]?.guide?.[0]?.link?.href, OK.enableUrl);
  assert.equal(noServe[0]?.guide?.[1]?.command, SERVE_COMMAND);
  assert.equal(noServe[0]?.action, undefined);

  const noLink = { ...OK, serve: false, enableUrl: undefined };
  assert.equal(mobileSteps(view({}, noLink), noLink)[0]?.guide?.[0]?.link?.href, 'https://login.tailscale.com/admin/dns');
});

test('人の待ち受けが応えないときは、起動していなければ起動を、古ければ作り直しを案内する', () => {
  const down = { ...OK, listening: false };
  const stopped = view({ link: { status: 'waiting' } }, down);
  const [start] = mobileSteps(stopped, down);
  assert.equal(start?.title, 'movie-maker が起動していません');
  assert.match(start?.text ?? '', /起動する/);
  assert.equal(start?.action, undefined);
  assert.deepEqual(mmSteps(stopped).map((s) => s.key), ['launch']);

  const [old] = mobileSteps(view({}, down), down);
  assert.match(old?.title ?? '', /待ち受け/);
  assert.match(old?.text ?? '', /作り直して/);

  const stale = view({ install: { ...view().install, sourceCommit: 'def' } }, down);
  const [rebuild] = mobileSteps(stale, down);
  assert.match(rebuild?.text ?? '', /「作り直す」/);
  assert.equal(rebuild?.action, undefined);
  assert.deepEqual(mmSteps(stale).map((s) => s.key), ['rebuild']);
});
