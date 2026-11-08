import { describe, expect, it } from "vitest";
import { createApi } from "./api/http.ts";
import { createConnection } from "./connection.ts";
import { createMockServer } from "./mock/mockServer.ts";

const settle = () => new Promise<void>((r) => setTimeout(r, 0));

/** SSE を開いた数と閉じた数、プロジェクトを読んだ数を数える接続。 */
const setup = () => {
  const server = createMockServer({ paired: true, online: true });
  const counts = { opened: 0, closed: 0, reads: 0, timers: 0 };
  let toggle = (_visible: boolean): void => undefined;
  const connection = createConnection({
    api: createApi((path, init) => {
      if (path.endsWith("/project")) counts.reads += 1;
      return server.platform.fetch(path, init);
    }),
    openEvents: (url) => {
      counts.opened += 1;
      const es = server.platform.openEvents(url);
      const close = es.close;
      es.close = () => {
        counts.closed += 1;
        close();
      };
      return es;
    },
    onNetworkBack: () => undefined,
    onVisibility: (fn) => (toggle = fn),
    setTimer: () => (counts.timers += 1),
  });
  return { server, connection, counts, toggle: (v: boolean) => toggle(v) };
};

describe("画面が隠れたときの接続", () => {
  it("隠れたら SSE を閉じ、戻ったらプロジェクトを読み直して開き直す", async () => {
    const { connection, counts, toggle } = setup();
    await connection.start();
    await settle();
    expect(counts).toMatchObject({ opened: 1, closed: 0, reads: 1 });
    toggle(false);
    expect(counts.closed).toBe(1);
    toggle(true);
    await settle();
    expect(counts).toMatchObject({ opened: 2, reads: 2 });
    expect(connection.get().state).toBe("online");
  });

  it("隠れている間は圏外でも再試行を予約せず、SSE も開かない", async () => {
    const { server, connection, counts, toggle } = setup();
    await connection.start();
    await settle();
    toggle(false);
    server.setOnline(false);
    await connection.refresh();
    expect(counts.timers).toBe(0);
    server.setOnline(true);
    await connection.refresh();
    expect(counts.opened).toBe(1);
  });
});
