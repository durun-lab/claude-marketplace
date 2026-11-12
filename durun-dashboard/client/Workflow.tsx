// トリガーでセッションを動かす「ワークフロー」アプリ。一覧で ON と OFF を切り替え、「設定」で項目を直す。作るときと大きく変えるときはエージェントと相談する。
// 定義はワークフローごとのフォルダーにあり、エクスプローラーやメモ帳でも直せる。
import { useState } from 'preact/hooks';
import type { Snapshot, WorkflowInfo, WorkflowState, WorkflowTrigger } from '../shared/types.ts';
import { api, attempt } from './api.ts';
import { AppIcon, DesktopIcon } from './apps.tsx';
import { type Float, FloatFrame, dock, floatUp } from './Float.tsx';
import { shortDateTime } from './format.ts';
import { openExplorer } from './Explorer.tsx';
import { openNotepadFile } from './Notepad.tsx';
import { href } from './router.ts';
import { WorkflowForm } from './WorkflowForm.tsx';

/** ウィンドウの保存キーで、ダッシュボードに置くアプリの種類の名前でもある。ウィンドウは 1 つだけ開く。 */
export const WORKFLOW_KEY = 'workflow';
export const WORKFLOW_TITLE = 'ワークフロー';
export const WORKFLOW_ICON = '/workflow.svg';
const SIZE = { w: 900, h: 460 };
const EMPTY: WorkflowState = { dir: '', workflows: [] };
const DAYS = ['日', '月', '火', '水', '木', '金', '土'];

export const openWorkflow = (): void => floatUp(WORKFLOW_KEY, '', { id: WORKFLOW_KEY }, SIZE);
const close = () => dock(WORKFLOW_KEY, '', WORKFLOW_KEY);

export const WorkflowIcon = () => <DesktopIcon icon={WORKFLOW_ICON} title={WORKFLOW_TITLE} hint="時刻やレビュー依頼、改善要望をきっかけに、セッションを動かします。ダブルクリックで開きます" onLaunch={openWorkflow} />;

const triggerText = (t: WorkflowTrigger): string =>
  t.kind === 'github-review' ? 'レビュー依頼' : t.kind === 'slack-mention' ? 'Slack のメンション' : t.kind === 'jira-mention' ? 'JIRA のメンション' : t.kind === 'feedback' ? '改善要望' : `${t.days ? `${t.days.map((d) => DAYS[d]).join('')} ` : '毎日 '}${t.at}`;

const actionText = (w: WorkflowInfo): string => (w.mode === 'continue' ? `1 つのセッションを引き継ぐ${w.compact ? '（毎回 compact）' : ''}${w.compactWindow ? `（${w.compactWindow / 1000} 千トークンで自動 compact）` : ''}` : `新しいセッション（同時 ${w.maxSessions ?? 1} まで）`);

/** 相談のセッションを起動し、そのチャットへ移る。 */
const consult = (id?: string) =>
  void attempt('ワークフローの相談', async () => {
    const { sessionId } = await api.workflowConsult(id);
    location.hash = href.session(sessionId);
  });

const Row = ({ w, now, onEdit }: { w: WorkflowInfo; now: number; onEdit: () => void }) => (
  <tr class={w.enabled ? undefined : 'is-off'} title={w.prompt}>
    <td>
      <input type="checkbox" role="switch" class="learn-switch" checked={w.enabled} aria-label={`${w.name}を動かす`} onChange={(e) => void attempt('ワークフローの切り替え', () => api.workflowEnabled(w.id, e.currentTarget.checked))} />
      {w.name}
    </td>
    <td>{w.triggers.map(triggerText).join('、') || 'なし'}{w.burstMinutes ? `（${w.burstMinutes} 分でまとめる）` : ''}</td>
    <td>{actionText(w)}</td>
    <td>
      {w.lastRunAt ? shortDateTime(w.lastRunAt, now) : '—'}
      {w.running > 0 && `（${w.running} 件動作中）`}
      {w.queued > 0 && `（${w.queued} 件待ち）`}
      {w.error && <span class="recycle-error" title={w.error}> {w.error}</span>}
    </td>
    <td class="workflow-actions">
      <button class="xp-task-link" onClick={() => void attempt('ワークフローの実行', () => api.workflowRun(w.id))}>実行</button>
      <button class="xp-task-link" onClick={onEdit}>設定</button>
      <button class="xp-task-link" title={`${w.dir}/${w.id}.md`} onClick={() => openNotepadFile(`${w.dir}/${w.id}.md`)}>指示</button>
      <button class="xp-task-link" onClick={() => consult(w.id)}>相談</button>
      {w.sessionId && <a href={href.session(w.sessionId)}>セッション</a>}
    </td>
  </tr>
);

export const WorkflowFloat = ({ f, snapshot }: { f: Float; snapshot: Snapshot }) => {
  const state = snapshot.workflows ?? EMPTY;
  const [editing, setEditing] = useState<string>();
  const target = state.workflows.find((w) => w.id === editing);
  return (
    <FloatFrame f={f} title={WORKFLOW_TITLE} icon={WORKFLOW_ICON} controls={[{ kind: 'close', label: '閉じる', onClick: close }]}>
      <div class="recycle">
        <div class="recycle-main">
          <aside class="recycle-tasks">
            <section class="xp-task">
              <h3 class="xp-task-title">ワークフローのタスク</h3>
              <div class="xp-task-body">
                <button class="xp-task-link" onClick={() => consult()}><AppIcon icon={WORKFLOW_ICON} />相談して新しく作る</button>
                {state.dir && <button class="xp-task-link" onClick={() => openExplorer(state.dir)}>フォルダーを開く</button>}
              </div>
            </section>
            <section class="xp-task">
              <h3 class="xp-task-title">詳細</h3>
              <div class="xp-task-body">
                {state.error && <p class="recycle-error">{state.error}</p>}
                <p>時刻や、GitHub で自分へのレビュー依頼、Slack と JIRA で自分へのメンション、ダッシュボードへの改善要望が届いたことをきっかけに、セッションを動かします。</p>
                <p>「改善要望」のトリガーは、前のウェーブを終えてから、たまった要望を 1 つのウェーブとして渡します。</p>
                <p>「新しいセッション」はイベントごとに起動し、同時に動かす数を抑えます。「引き継ぐ」は 1 つのセッションのチャットにイベントを送り、毎回前回までの会話を compact することもできます。</p>
                <p>「設定」で項目を直せます。作るときと、依頼の中身を練り直すときは、エージェントと相談します。</p>
              </div>
            </section>
          </aside>
          <div class="recycle-list">
            {target ? <WorkflowForm key={target.id} w={target} themes={snapshot.themes} onClose={() => setEditing(undefined)} /> : (
              <table class="recycle-table workflow-table">
                <thead>
                  <tr><th>名前</th><th>トリガー</th><th>アクション</th><th>最後の実行</th><th>操作</th></tr>
                </thead>
                <tbody>
                  {state.workflows.map((w) => <Row key={w.id} w={w} now={snapshot.now} onEdit={() => setEditing(w.id)} />)}
                </tbody>
              </table>
            )}
          </div>
        </div>
      </div>
    </FloatFrame>
  );
};
