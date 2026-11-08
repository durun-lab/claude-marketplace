// ホームのダッシュボードは、名前を付けて複数の配置をサーバーに保存し、メニューから切り替える。
// どの端末でも同じ配置を選べるよう、一覧と配置はスナップショットで受け取り、ブラウザには選んでいるものだけを保存する。
import { useEffect, useState } from 'preact/hooks';
import { api, attempt } from './api.ts';
import { uniqueName } from './layout.ts';
import { type DashboardEntry, type Dashboards, FIRST_DASHBOARDS } from '../shared/types.ts';

type State = Dashboards & { current: string };
type Input = { id: string; name?: string; layout?: unknown[] };

/** ホームのウィジェットを浮かべるときの保存キー。浮かべたウィンドウは、ダッシュボードを切り替えても出したままにする。 */
export const HOME_KEY = 'dashboard-layout';
const CURRENT_KEY = 'dashboard-current';
// サーバーに保存する前に、ブラウザに一覧を保存していたキー。最初のダッシュボードの配置は HOME_KEY に、ほかは HOME_KEY:id に保存していた。
const LEGACY_LIST_KEY = 'dashboards';
const legacyLayoutKey = (id: string): string => (id ? `${HOME_KEY}:${id}` : HOME_KEY);
const NEW_NAME = '新しいダッシュボード';

const readJson = (key: string): unknown => {
  try {
    return JSON.parse(localStorage.getItem(key) ?? 'null');
  } catch {
    return null;
  }
};

const readCurrent = (): string => {
  try {
    return localStorage.getItem(CURRENT_KEY) ?? (readJson(LEGACY_LIST_KEY) as { current?: string } | null)?.current ?? '';
  } catch {
    return '';
  }
};

/** ブラウザに残っている一覧と配置。何も残っていなければ undefined。 */
const readLegacy = (): Dashboards | undefined => {
  const saved = (readJson(LEGACY_LIST_KEY) as { list?: DashboardEntry[] } | null)?.list;
  const list = saved?.length ? saved : FIRST_DASHBOARDS.list;
  const layouts = Object.fromEntries(list.flatMap((d) => {
    const layout = readJson(legacyLayoutKey(d.id));
    return Array.isArray(layout) ? [[d.id, layout]] : [];
  }));
  return saved?.length || Object.keys(layouts).length ? { list, layouts } : undefined;
};

const forgetLegacy = (d: Dashboards): void => {
  try {
    localStorage.removeItem(LEGACY_LIST_KEY);
    for (const { id } of d.list) localStorage.removeItem(legacyLayoutKey(id));
  } catch {
    // 消せなくても、サーバーに一覧があれば読まない。
  }
};

let state: State = { ...FIRST_DASHBOARDS, current: readCurrent() };
const listeners = new Set<(s: State) => void>();
// 送って返事を待っている変更の数。待っている間に届いたスナップショットは、変更より前の一覧なので使わない。
let pending = 0;
let importing = false;

const setState = (next: State): void => {
  state = next;
  listeners.forEach((fn) => fn(next));
};

/** 一覧に無いダッシュボードを選んでいたら、最初のものを選んでいるとみなす。 */
const resolved = (s: State): State => (s.list.some((d) => d.id === s.current) ? s : { ...s, current: s.list[0]?.id ?? '' });

export const useDashboards = (): State => {
  const [s, setS] = useState(state);
  useEffect(() => {
    listeners.add(setS);
    setS(state);
    return () => {
      listeners.delete(setS);
    };
  }, []);
  return resolved(s);
};

/** 変更をすぐ画面に出してからサーバーに送り、返ってきた一覧で置き換える。 */
const send = async (label: string, fn: () => Promise<Dashboards>): Promise<Dashboards | undefined> => {
  pending++;
  const saved = await attempt(label, fn);
  pending--;
  if (saved && !pending) setState({ ...state, ...saved });
  return saved;
};

/**
 * スナップショットで届いた一覧を使う。fresh はサーバーから届いたばかりのもので、前に開いていたときのデータではない。
 * サーバーにまだ一覧が無ければ、ブラウザに残っている一覧と配置を送って引き継ぐ。
 */
export const receiveDashboards = (d: Dashboards | undefined, fresh: boolean): void => {
  if (pending) return;
  if (d) {
    if (JSON.stringify(d) !== JSON.stringify({ list: state.list, layouts: state.layouts })) setState({ ...state, ...d });
    return;
  }
  const legacy = fresh && !importing ? readLegacy() : undefined;
  if (!legacy) return;
  importing = true;
  setState({ ...state, ...legacy });
  void send('ダッシュボードの引き継ぎ', () => api.importDashboards(legacy)).then((saved) => {
    if (saved) forgetLegacy(legacy);
    importing = false;
  });
};

const save = (label: string, next: State, input: Input): void => {
  setState(next);
  void send(label, () => api.saveDashboard(input));
};

const currentEntry = (): DashboardEntry => {
  const s = resolved(state);
  return s.list.find((d) => d.id === s.current) ?? FIRST_DASHBOARDS.list[0]!;
};

export const switchDashboard = (id: string): void => {
  try {
    localStorage.setItem(CURRENT_KEY, id);
  } catch {
    // 保存できなくても、開いている間は切り替えられる。
  }
  setState({ ...state, current: id });
};

/** layout の配置で、name を元に重ならない名前のダッシュボードを足して選ぶ。layout が無ければ、初期配置になる。 */
const add = (name: string, layout: unknown[] | undefined): void => {
  const id = String(Date.now());
  const entry = { id, name: uniqueName(state.list.map((d) => d.name), name) };
  save('ダッシュボードの作成', { ...state, list: [...state.list, entry], layouts: layout ? { ...state.layouts, [id]: layout } : state.layouts }, { ...entry, layout });
  switchDashboard(id);
};

/** ウィジェットを置いていないダッシュボードを作って選ぶ。 */
export const createDashboard = (): void => add(NEW_NAME, []);

/** 選んでいるダッシュボードの配置を写したものを作って選ぶ。 */
export const duplicateDashboard = (): void => {
  const src = currentEntry();
  add(`${src.name} のコピー`, state.layouts[src.id]);
};

/** 選んでいるダッシュボードの名前を変える。空の名前は受け付けない。 */
export const renameDashboard = (name: string): void => {
  const { id } = currentEntry();
  const trimmed = name.trim();
  if (trimmed) save('名前の変更', { ...state, list: state.list.map((d) => (d.id === id ? { ...d, name: trimmed } : d)) }, { id, name: trimmed });
};

/** id のダッシュボードの配置を保存する。 */
export const saveDashboardLayout = (id: string, layout: unknown[]): void => save('配置の保存', { ...state, layouts: { ...state.layouts, [id]: layout } }, { id, layout });
