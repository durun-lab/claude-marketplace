// Claude Code が読む指示を、いつ効くかとともに並べ、メモ帳で開いて直せる「学習」アプリ。
// 人間が会話で教えたことから差分を提案させる学習は、ワークフロー「人間の指示から学習」が受け持つ。
import { useState } from 'preact/hooks';
import type { HarnessItem, HarnessToggle, Snapshot } from '../shared/types.ts';
import { api, attempt } from './api.ts';
import { AppIcon, DesktopIcon } from './apps.tsx';
import { type Float, FloatFrame, dock, floatUp } from './Float.tsx';
import { useFetch } from './hooks.ts';
import { NOTEPAD_ICON, openNotepadFile } from './Notepad.tsx';
import { WORKFLOW_ICON, openWorkflow } from './Workflow.tsx';

/** ウィンドウの保存キーで、ダッシュボードに置くアプリの種類の名前でもある。ウィンドウは 1 つだけ開くので、タブの id にもこのキーを入れる。 */
export const LEARN_KEY = 'learn';
export const LEARN_TITLE = '学習';
export const LEARN_ICON = '/learn.svg';
const SIZE = { w: 860, h: 480 };
const KINDS: Record<HarnessItem['kind'], string> = { instructions: 'CLAUDE.md', skill: 'スキル', memory: '記憶', hook: 'フック' };

type View = HarnessItem['kind'];

export const openLearn = (): void => floatUp(LEARN_KEY, '', { id: LEARN_KEY }, SIZE);
const close = () => dock(LEARN_KEY, '', LEARN_KEY);

export const LearnIcon = () => <DesktopIcon icon={LEARN_ICON} title={LEARN_TITLE} hint="Claude Code が読む指示を一覧して直します。ダブルクリックで開きます" onLaunch={openLearn} />;

type Toggle = (t: HarnessToggle, on: boolean) => void;

/** settings.json に書くスイッチ。行のクリック（枝の開閉）に伝えない。 */
const Switch = ({ t, label, onToggle }: { t: HarnessToggle; label: string; onToggle: Toggle }) => (
  <input type="checkbox" role="switch" class="learn-switch" checked={t.enabled} aria-label={`${label}を使う`} title={t.enabled ? '使う（クリックで止める）' : '止めている（クリックで使う）'} onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => onToggle(t, e.currentTarget.checked)} />
);

/** 置き場所ごとの枝に分けて並べる。枝の行をクリックすると開閉する。止めているプラグインの枝は、開くまで畳んでおく。行をダブルクリックするか Enter で、その指示のファイルをメモ帳で開く。 */
const Harness = ({ items, opened, flip, onToggle }: { items: HarnessItem[]; opened: Map<string, boolean>; flip: (group: string, open: boolean) => void; onToggle: Toggle }) => {
  const groups = Map.groupBy(items, (h) => h.group);
  return (
    <table class="recycle-table learn-harness">
      <thead>
        <tr><th>名前</th><th>いつ効くか</th><th>置き場所</th><th>文字数</th></tr>
      </thead>
      <tbody>
        {[...groups].map(([group, list]) => {
          const toggle = list[0]?.groupToggle;
          const open = opened.get(group) ?? toggle?.enabled !== false;
          return [
            <tr key={`group\n${group}`} class="learn-group" tabIndex={0} aria-expanded={open} onClick={() => flip(group, !open)} onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && flip(group, !open)}>
              <td colSpan={4}>
                <span class="learn-twisty">{open ? '−' : '+'}</span>
                {toggle && <Switch t={toggle} label={group} onToggle={onToggle} />}
                {group}（{list.length}）
              </td>
            </tr>,
            ...(open
              ? list.map((h) => (
                  <tr key={`${h.path}\n${h.trigger}\n${h.name}`} class={h.toggle?.enabled === false || toggle?.enabled === false ? 'is-off' : undefined} tabIndex={0} title={h.path} onDblClick={() => openNotepadFile(h.path)} onKeyDown={(e) => e.key === 'Enter' && openNotepadFile(h.path)}>
                    <td class="learn-leaf">{h.toggle ? <Switch t={h.toggle} label={h.name} onToggle={onToggle} /> : <AppIcon icon={NOTEPAD_ICON} />}{h.name}</td>
                    <td title={h.trigger}>{h.trigger}</td>
                    <td>{h.source}</td>
                    <td>{h.chars.toLocaleString()}</td>
                  </tr>
                ))
              : []),
          ];
        })}
      </tbody>
    </table>
  );
};

export const LearnFloat = ({ f }: { f: Float; snapshot: Snapshot }) => {
  const [view, setView] = useState<View>('instructions');
  // 人間が開閉した枝だけを覚える。覚えていない枝は、有効なら開き、無効なら畳む。
  const [opened, setOpened] = useState(new Map<string, boolean>());
  const harness = useFetch(() => api.harness(), []);
  const all = harness.data?.items ?? [];
  const items = all.filter((h) => h.kind === view);
  const count = (kind: HarnessItem['kind']) => all.filter((h) => h.kind === kind).length;
  const flip = (group: string, open: boolean) => setOpened((m) => new Map(m).set(group, open));
  const onToggle: Toggle = (t, on) => void attempt('有効・無効の切り替え', () => api.harnessToggle(t, on)).then(harness.reload);
  const kindToggle = harness.data?.kindToggles[view];
  return (
    <FloatFrame f={f} title={LEARN_TITLE} icon={LEARN_ICON} controls={[{ kind: 'close', label: '閉じる', onClick: close }]}>
      <div class="recycle">
        <div class="recycle-main">
          <aside class="recycle-tasks">
            <section class="xp-task">
              <h3 class="xp-task-title">学習のタスク</h3>
              <div class="xp-task-body">
                <button class="xp-task-link" title="ワークフロー「人間の指示から学習」で、動かす時刻を決めたり今すぐ動かしたりします" onClick={openWorkflow}><AppIcon icon={WORKFLOW_ICON} />学習のワークフローを開く</button>
              </div>
            </section>
            <section class="xp-task">
              <h3 class="xp-task-title">知見の一覧</h3>
              <div class="xp-task-body">
                {(Object.keys(KINDS) as HarnessItem['kind'][]).map((k) => (
                  <button key={k} class="xp-task-link" aria-pressed={view === k} onClick={() => setView(k)}>{KINDS[k]}（{count(k)}）</button>
                ))}
                <button class="xp-task-link" onClick={harness.reload}>読み直す</button>
                {harness.error && <p class="recycle-error">{harness.error}</p>}
              </div>
            </section>
            <section class="xp-task">
              <h3 class="xp-task-title">詳細</h3>
              <div class="xp-task-body">
                {kindToggle && (
                  <label class="check">
                    <input type="checkbox" role="switch" checked={kindToggle.enabled} onChange={(e) => onToggle(kindToggle, e.currentTarget.checked)} />
                    {KINDS[view]}を使う
                  </label>
                )}
                <p>Claude Code が読む指示です。行をダブルクリックすると、メモ帳で開いて直せます。スキルの「いつ効くか」は SKILL.md の description です。</p>
                <p>スイッチは ~/.claude/settings.json に書き、次に始めるセッションから効きます。{view === 'hook' && 'フックは 1 つずつは止められず、まとめて止めます。'}</p>
              </div>
            </section>
          </aside>
          <div class="recycle-list"><Harness items={items} opened={opened} flip={flip} onToggle={onToggle} /></div>
        </div>
      </div>
    </FloatFrame>
  );
};
