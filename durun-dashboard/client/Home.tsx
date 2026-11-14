import { useEffect, useRef, useState } from 'preact/hooks';
import { EFFORT_LEVELS, MODELS, type Session, type Snapshot } from '../shared/types.ts';
import { api, attempt } from './api.ts';
import { HOME_KEY, saveDashboardLayout, useDashboards } from './dashboards.ts';
import { useKeptScroll } from './keptScroll.ts';
import { Dashboard, type EditProps, type Widget, type WidgetProps } from './Dashboard.tsx';
import { DesktopIcon } from './apps.tsx';
import { BOOKMARKS_ICON, BOOKMARKS_KEY, BOOKMARKS_TITLE, BookmarksIcon, openBookmarks } from './Bookmarks.tsx';
import { jiraSpaces } from './completion.ts';
import { SessionTimeline, TIMELINE_TITLE } from './SessionTimeline.tsx';
import { NEW_SESSION_ALT_SHORTCUT, NEW_SESSION_SHORTCUT, shortcutLabel } from './shortcut.ts';
import { EXPLORER_ICON, EXPLORER_KEY, EXPLORER_TITLE, ExplorerIcon, openExplorer } from './Explorer.tsx';
import { IE_ICON, IE_KEY, IE_TITLE, InternetExplorerIcon, openInternetExplorer } from './InternetExplorer.tsx';
import { NOTEPAD_ICON, NOTEPAD_KEY, NOTEPAD_TITLE, NotepadIcon, openNotepad } from './Notepad.tsx';
import { PROOFREAD_ICON, PROOFREAD_KEY, PROOFREAD_TITLE, ProofreadIcon, openProofread } from './Proofread.tsx';
import { LEARN_ICON, LEARN_KEY, LEARN_TITLE, LearnIcon, openLearn } from './Learn.tsx';
import { WORKFLOW_ICON, WORKFLOW_KEY, WORKFLOW_TITLE, WorkflowIcon, openWorkflow } from './Workflow.tsx';
import { RECYCLE_BIN_ICON, RECYCLE_BIN_KEY, RECYCLE_BIN_TITLE, RecycleBinIcon, openRecycleBin } from './RecycleBin.tsx';
import { TERMINAL_ICON, TERMINAL_KEY, TERMINAL_TITLE, TerminalIcon, openTerminal } from './Terminal.tsx';
import { FL_ICON, FL_KEY, FL_TITLE, FlConnectIcon, openFlConnect } from './FlConnect.tsx';
import { MM_ICON, MM_KEY, MM_TITLE, MovieMakerIcon, openMovieMaker } from './MovieMaker.tsx';
import { CDRIP_ICON, CDRIP_KEY, CDRIP_TITLE, CdRipIcon, openCdRip } from './CdRip.tsx';
import { TRANSLATE_ICON, TRANSLATE_KEY, TRANSLATE_TITLE, TranslateIcon, openTranslate } from './Translate.tsx';
import { type Float, FloatFrame, GITHUB_ICON, JIRA_ICON, FloatWindow, dock, floatUp, isFloating, useFloats } from './Float.tsx';
import { GITHUB_INBOX_TITLE, GithubInbox, githubInboxControls } from './GithubInbox.tsx';
import { Inbox } from './Inbox.tsx';
import { JIRA_TITLE, JiraIssues, jiraControls } from './JiraIssues.tsx';
import { SLACK_THREAD_TITLE, SlackThread, slackThreadControls } from './SlackThread.tsx';
import { ROWS, type Layout } from './layout.ts';
import { MessageInput } from './Messaging.tsx';
import { currentSessionId } from './router.ts';
import { ThemeGroups, type Selection } from './SessionCards.tsx';
import { toast } from './toast.tsx';
import { Choice } from './ui.tsx';

const SelectionBar = ({ ids, onClear }: { ids: string[]; onClear: () => void }) => {
  const [busy, setBusy] = useState(false);
  const archive = async () => {
    setBusy(true);
    const deferred = await attempt('アーカイブ', () => Promise.all(ids.map((id) => api.archive(id, true))));
    setBusy(false);
    if (!deferred) return;
    const later = deferred.filter(Boolean).length;
    const now = ids.length - later;
    toast([now > 0 && `${now} 件をアーカイブしました`, later > 0 && `作業中の ${later} 件は、作業が終わったらアーカイブします`].filter(Boolean).join('。'));
    onClear();
  };
  return (
    <div class="selection-bar" role="toolbar" aria-label="選んだセッションへの操作">
      <span><b>{ids.length}</b> 件を選択中</span>
      <button class="btn btn-sm btn-primary" disabled={busy} onClick={archive}>アーカイブする</button>
      <button class="btn btn-sm" onClick={onClear}>選択を解除</button>
    </div>
  );
};

/** ディレクトリの候補は、これまでのセッションの作業ディレクトリを最近使った順に並べる。 */
const recentCwds = (sessions: Session[]): string[] =>
  [...new Set(sessions.toSorted((a, b) => b.updatedAt - a.updatedAt).map((s) => s.cwd))];

/**
 * 新規セッションの入力欄の値を、開き直しても前回の選択から始められるようブラウザに保存する。
 * options を渡すと、選択肢から消えた値は既定に戻す。
 */
const useSaved = (key: string, options?: readonly string[]) => {
  const [value, setValue] = useState(() => {
    const v = localStorage.getItem(key) ?? '';
    return !options || options.includes(v) ? v : '';
  });
  const save = (next: string) => {
    setValue(next);
    localStorage.setItem(key, next);
  };
  return [value, save] as const;
};

// 新規セッションのウィンドウを開くときに、ディレクトリとテーマの欄へ入れる値。ウィンドウが開いていれば、入力欄へ直に渡す。
// prompt は、起動できなかった最初の依頼を書き戻すときだけ渡す。parent は、セッションの画面から開いたときの、そのセッションの id。
type Preset = { cwd: string; theme: string; prompt?: string; parent?: string };
let presetOnOpen: Preset | undefined;
const presetListeners = new Set<(preset: Preset) => void>();

const NewSession = ({ cwds, themes, sessions, accounts, onClose }: { cwds: string[]; themes: string[]; sessions: Session[]; accounts: string[]; onClose: () => void }) => {
  const dir = useRef<HTMLInputElement>(null);
  const ask = useRef<HTMLTextAreaElement>(null);
  const [cwd, setCwd] = useSaved('dashboard-new-session-cwd');
  // 前回のテーマを残すと、無関係なセッションまで同じテーマに入るので、保存しない。
  const [theme, setTheme] = useState('');
  const [prompt, setPrompt] = useState('');
  const [parent, setParent] = useState<string>();
  const [asChild, setAsChild] = useState(false);
  const [model, setModel] = useSaved('dashboard-new-session-model', MODELS);
  const [effort, setEffort] = useSaved('dashboard-new-session-effort', EFFORT_LEVELS);
  // 追加アカウントにはコネクタが無いので、前回の選択を残さず、毎回プライマリアカウントから始める。
  const [account, setAccount] = useState('');
  // セッションの画面から開いたときは、そのセッションの作業ディレクトリとテーマを入れる。開いている間にもう一度開かれたら、入れ直す。
  // ディレクトリが埋まっていれば、次に書く最初の依頼から入力できるようにする。
  useEffect(() => {
    const preset = presetOnOpen;
    presetOnOpen = undefined;
    const fill = (p: Preset) => {
      setCwd(p.cwd);
      setTheme(p.theme);
      setParent(p.parent);
      if (p.prompt) setPrompt(p.prompt);
    };
    if (preset) fill(preset);
    (preset || cwd ? ask : dir).current?.focus();
    presetListeners.add(fill);
    return () => {
      presetListeners.delete(fill);
    };
  }, []);
  const ready = !!cwd.trim() && !!prompt.trim();
  // claude --bg は、まれに数十秒かかる。人間を待たせないよう、起動を待たずにウィンドウを閉じる。
  // 起動できなかったら、書いた依頼を失わないよう、ウィンドウを開き直して書き戻す。次の依頼を書いているウィンドウは書き換えない。
  const start = async () => {
    if (!ready) return;
    onClose();
    const child = asChild ? parent : undefined;
    if (await attempt('セッションの起動', () => api.startSession(cwd.trim(), prompt, model, effort, theme, child, account))) toast('セッションを起動しました');
    else if (!presetListeners.size) openNewSession(cwd.trim(), theme, prompt, parent);
  };
  return (
    <div class="stack new-session">
      <label class="inline-field">
        <span class="nowrap">ディレクトリ</span>
        <input ref={dir} class="input mono" list="cwd-options" placeholder="/path/to/repo" value={cwd} onInput={(e) => setCwd(e.currentTarget.value)} />
        <datalist id="cwd-options">
          {cwds.map((c) => <option key={c} value={c} />)}
        </datalist>
      </label>
      <label class="inline-field">
        <span class="nowrap">テーマ</span>
        <input class="input" list="new-session-theme-options" placeholder="空のときは、リポジトリ名になります" value={theme} onInput={(e) => setTheme(e.currentTarget.value)} />
        <datalist id="new-session-theme-options">
          {themes.map((t) => <option key={t} value={t} />)}
        </datalist>
      </label>
      {/* ウィンドウは画面を移っただけでは描き直さないので、開いている画面は、打つたびに描き直すこの欄で読む。 */}
      <MessageInput cwd={cwd.trim()} spaces={jiraSpaces(sessions, currentSessionId())} value={prompt} setValue={setPrompt} placeholder="最初の依頼（/ で skills、# で PR と Issue、@ でファイル、PROJ- か - で JIRA の課題を補完、Cmd/Ctrl + Enter で起動）" onSubmit={start} inputRef={ask} />
      <div class="actions">
        <Choice label="モデル" value={model} options={MODELS} onChange={setModel} />
        <Choice label="effort" value={effort} options={EFFORT_LEVELS} onChange={setEffort} />
        {accounts.length > 0 && <Choice label="アカウント" value={account} options={accounts} empty="プライマリ" onChange={setAccount} />}
        {parent && (
          <label class="inline-field">
            <input type="checkbox" checked={asChild} onChange={(e) => setAsChild(e.currentTarget.checked)} />
            子セッションとして開始
          </label>
        )}
      </div>
      <div class="actions">
        <span class="help">バックグラウンドで Claude Code を起動し、最初の依頼を渡します。</span>
        <span class="spacer" />
        <button class="btn" onClick={onClose}>キャンセル</button>
        <button class="btn btn-primary" disabled={!ready} onClick={start}>起動</button>
      </div>
    </div>
  );
};

/** 新規セッションのウィンドウの保存キー。ホームのウィジェットではないので、scope は空にし、タブの id にもこのキーを入れる。 */
export const NEW_SESSION_KEY = 'new-session';
const NEW_SESSION_TAB = { id: NEW_SESSION_KEY };
const closeNewSession = () => dock(NEW_SESSION_KEY, '', NEW_SESSION_KEY);
// 入力欄がちょうど収まる大きさ（px）で、画面の上寄りの中ほどに出す。
const NEW_SESSION_SIZE = { w: 640, h: 333 };
export const NEW_SESSION_TITLE = '新規セッション';
export const NEW_SESSION_ICON = '/claude-new.svg';

/**
 * 新規セッションのウィンドウを開く。開いていれば手前に出す。cwd と theme と prompt を渡すと、ディレクトリとテーマと最初の依頼の欄にそれぞれ入れる。
 * parent を渡すと、そのセッションの子として起動するチェックボックスを出す。
 */
export const openNewSession = (cwd?: string, theme = '', prompt?: string, parent?: string): void => {
  const preset = cwd ? { cwd, theme, prompt, parent } : undefined;
  if (preset && presetListeners.size > 0) presetListeners.forEach((fn) => fn(preset));
  else presetOnOpen = preset;
  floatUp(NEW_SESSION_KEY, '', NEW_SESSION_TAB, NEW_SESSION_SIZE);
};

/** アプリ「新規セッション」のアイコン。cwd と theme は、セッションの画面に置いたときの、そのセッションの作業ディレクトリとテーマ。 */
export const NewSessionIcon = ({ cwd, theme, parent }: { cwd?: string; theme?: string; parent?: string }) => (
  <DesktopIcon
    icon={NEW_SESSION_ICON}
    title={NEW_SESSION_TITLE}
    hint={cwd ? `ダブルクリックで、${cwd} で始めるセッションの入力欄を開きます` : 'ダブルクリックで、新しいセッションの入力欄を開きます'}
    onLaunch={() => openNewSession(cwd, theme, undefined, parent)}
  />
);

export const NewSessionFloat = ({ f, snapshot }: HomeData & { f: Float }) => (
  <FloatFrame f={f} title={NEW_SESSION_TITLE} icon={NEW_SESSION_ICON} controls={[{ kind: 'close', label: '閉じる', onClick: closeNewSession }]}>
    <NewSession cwds={recentCwds(snapshot.sessions)} themes={snapshot.themes} sessions={snapshot.sessions} accounts={snapshot.accounts ?? []} onClose={closeNewSession} />
  </FloatFrame>
);

/** ホームのウィジェットに渡すデータ。 */
type HomeData = { snapshot: Snapshot };
export type HomeWidgetProps = WidgetProps<HomeData>;

const SessionsWidget = ({ snapshot }: HomeWidgetProps) => {
  const creating = useFloats((list) => isFloating(list, NEW_SESSION_KEY, '', NEW_SESSION_KEY));
  const [selected, setSelected] = useState<Set<string>>(new Set());
  // 一覧から消えたセッションは、選んだ数にも操作の対象にも入れない。
  const ids = snapshot.sessions.filter((s) => selected.has(s.id)).map((s) => s.id);
  const sel: Selection = {
    selected: new Set(ids),
    toggle: (ids, on) =>
      setSelected((prev) => {
        const next = new Set(prev);
        for (const id of ids) on ? next.add(id) : next.delete(id);
        return next;
      }),
  };
  return (
    <>
      <div class="section-head">
        <h2 class="section-title">セッション</h2>
        <div class="actions">
          <button class="btn btn-sm btn-primary" data-icon="add" aria-expanded={creating} title={`新しいセッションを開く（${shortcutLabel(NEW_SESSION_ALT_SHORTCUT)}。全画面では ${shortcutLabel(NEW_SESSION_SHORTCUT)}）`} onClick={() => openNewSession()}>新規セッション</button>
        </div>
      </div>
      <div class={`widget-body${ids.length ? ' is-selecting' : ''}`}>
        <ThemeGroups snapshot={snapshot} sel={sel} />
        {ids.length > 0 && <SelectionBar ids={ids} onClear={() => setSelected(new Set())} />}
      </div>
    </>
  );
};

const WIDGETS: Record<string, Widget<HomeData>> = {
  inbox: { title: '要対応', icon: '📥', Body: Inbox, size: { w: 10, h: ROWS }, sketch: 'list' },
  sessions: { title: 'セッション', icon: '🗂️', Body: SessionsWidget, size: { w: 14, h: ROWS }, sketch: 'cards' },
  jira: { title: JIRA_TITLE, icon: JIRA_ICON, Body: JiraIssues, size: { w: 8, h: ROWS / 2 }, sketch: 'list', controls: jiraControls },
  slack: { title: SLACK_THREAD_TITLE, icon: '💬', Body: SlackThread, size: { w: 8, h: ROWS / 2 }, sketch: 'chat', controls: slackThreadControls },
  timeline: { title: TIMELINE_TITLE, icon: '📊', Body: SessionTimeline, size: { w: 12, h: ROWS / 2 }, sketch: 'chart' },
  github: { title: GITHUB_INBOX_TITLE, icon: GITHUB_ICON, Body: GithubInbox, size: { w: 8, h: ROWS / 2 }, sketch: 'list', controls: githubInboxControls },
  [EXPLORER_KEY]: { title: EXPLORER_TITLE, icon: EXPLORER_ICON, Body: () => <ExplorerIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openExplorer() },
  [IE_KEY]: { title: IE_TITLE, icon: IE_ICON, Body: InternetExplorerIcon, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openInternetExplorer() },
  [NOTEPAD_KEY]: { title: NOTEPAD_TITLE, icon: NOTEPAD_ICON, Body: () => <NotepadIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openNotepad() },
  [PROOFREAD_KEY]: { title: PROOFREAD_TITLE, icon: PROOFREAD_ICON, Body: () => <ProofreadIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openProofread() },
  [TERMINAL_KEY]: { title: TERMINAL_TITLE, icon: TERMINAL_ICON, Body: () => <TerminalIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openTerminal() },
  // ホームから開いたときは、ウィンドウでセッションを選ぶまで、どのセッションにもつながない。
  [FL_KEY]: { title: FL_TITLE, icon: FL_ICON, Body: () => <FlConnectIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openFlConnect() },
  [MM_KEY]: { title: MM_TITLE, icon: MM_ICON, Body: () => <MovieMakerIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openMovieMaker() },
  [CDRIP_KEY]: { title: CDRIP_TITLE, icon: CDRIP_ICON, Body: () => <CdRipIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openCdRip() },
  [TRANSLATE_KEY]: { title: TRANSLATE_TITLE, icon: TRANSLATE_ICON, Body: () => <TranslateIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openTranslate() },
  [BOOKMARKS_KEY]: { title: BOOKMARKS_TITLE, icon: BOOKMARKS_ICON, Body: BookmarksIcon, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openBookmarks() },
  [RECYCLE_BIN_KEY]: { title: RECYCLE_BIN_TITLE, icon: RECYCLE_BIN_ICON, Body: ({ snapshot }) => <RecycleBinIcon full={!!snapshot.trash?.length} />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openRecycleBin() },
  [LEARN_KEY]: { title: LEARN_TITLE, icon: LEARN_ICON, Body: () => <LearnIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openLearn() },
  [WORKFLOW_KEY]: { title: WORKFLOW_TITLE, icon: WORKFLOW_ICON, Body: () => <WorkflowIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openWorkflow() },
  newSession: { title: NEW_SESSION_TITLE, icon: NEW_SESSION_ICON, Body: () => <NewSessionIcon />, size: { w: 2, h: 3 }, sketch: 'cards', launch: () => openNewSession() },
};

// 初期配置には、JIRA の設定が無くても使える種類を 1 つずつ入れる。
const DEFAULT_LAYOUT: Layout = [
  { id: 'inbox', x: 0, y: 0, w: 10, h: ROWS, tabs: [{ id: 'inbox' }] },
  { id: 'sessions', x: 10, y: 0, w: 14, h: ROWS, tabs: [{ id: 'sessions' }] },
];

export const Home = ({ snapshot, editing, onEditDone }: HomeData & EditProps) => {
  const { current, layouts } = useDashboards();
  const store = { saved: layouts[current], save: (layout: Layout) => saveDashboardLayout(current, layout) };
  const root = useRef<HTMLDivElement>(null);
  useKeptScroll(root, current);
  // 切り替えたダッシュボードの配置を読み直すよう、key で作り直す。
  return (
    <div ref={root} class="dashboard-scope">
      <Dashboard key={current} widgets={WIDGETS} defaults={DEFAULT_LAYOUT} storageKey={HOME_KEY} store={store} data={{ snapshot }} editing={editing} onEditDone={onEditDone} />
    </div>
  );
};

export const HomeFloat = ({ f, snapshot }: HomeData & { f: Float }) => <FloatWindow f={f} widgets={WIDGETS} data={{ snapshot }} />;
