// ホームのダッシュボードの一覧と配置。どの端末で開いても同じ配置を選べるよう、サーバーに保存する。
import { type DashboardEntry, type Dashboards, FIRST_DASHBOARDS } from '../shared/types.ts';

export type DashboardInput = { id: string; name?: string; layout?: unknown[] };

/**
 * id のダッシュボードを足すか、名前と配置を差し替える。name や layout を省いたら今の値を残す。
 * 一覧に無い id は、名前があるときだけ足す。名前の無い配置だけが届いても、一覧に無いダッシュボードは作らない。
 */
export const putDashboard = (saved: Dashboards | undefined, { id, name, layout }: DashboardInput): Dashboards => {
  const base = saved ?? FIRST_DASHBOARDS;
  const exists = base.list.some((d) => d.id === id);
  if (!exists && !name) return base;
  const list = exists ? base.list.map((d) => (d.id === id && name ? { ...d, name } : d)) : [...base.list, { id, name: name! }];
  return { list, layouts: layout ? { ...base.layouts, [id]: layout } : base.layouts };
};

const isEntry = (v: unknown): v is DashboardEntry => typeof (v as DashboardEntry)?.id === 'string' && typeof (v as DashboardEntry)?.name === 'string';

/** 画面がブラウザに持っていた一覧と配置を読む。形が合わなければ undefined。 */
export const dashboardsOf = (body: unknown): Dashboards | undefined => {
  const { list, layouts } = (body ?? {}) as Record<string, unknown>;
  if (!Array.isArray(list) || !list.length || !list.every(isEntry)) return undefined;
  if (typeof layouts !== 'object' || !layouts || !Object.values(layouts).every(Array.isArray)) return undefined;
  return { list: list.map(({ id, name }) => ({ id, name })), layouts: layouts as Record<string, unknown[]> };
};
