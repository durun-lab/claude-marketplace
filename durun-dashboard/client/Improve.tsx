import type { ImproveState } from '../shared/types.ts';
import { api, attempt } from './api.ts';
import { href } from './router.ts';
import { openWorkflow } from './Workflow.tsx';

type Flow = NonNullable<ImproveState['workflow']>;

const statusText = (pending: number, w: Flow): string | undefined => {
  const rest = pending ? `（残り ${pending} 件）` : '';
  if (w.running && !w.enabled) return `対応中のウェーブを終えたら止まります: ${w.current ?? ''}`;
  if (w.running) return `対応中: ${w.current ?? ''}${rest}`;
  if (w.enabled && w.error) return undefined;
  if (w.enabled) return pending ? 'セッションを起動しています…' : '要望を送ると、自動で実装を始めます。';
  return pending ? `未対応の要望が ${pending} 件あります。` : undefined;
};

/** 改善要望のトリガーを持つワークフローの切り替えと、いまの進み具合。モデルなどの設定はワークフローのアプリで変える。 */
export const ImproveStatus = ({ improve }: { improve: ImproveState }) => {
  const w = improve.workflow;
  if (!w) {
    return (
      <div class="fb-improve">
        <div class="actions">
          <span class="help">要望に自動で対応するには、ワークフローに「改善要望」のトリガーを足してください。</span>
          <span class="spacer" />
          <button class="btn btn-sm" onClick={openWorkflow}>ワークフロー</button>
        </div>
      </div>
    );
  }
  const status = statusText(improve.pending, w);
  return (
    <div class="fb-improve">
      <div class="actions">
        <label class="check">
          <input type="checkbox" role="switch" checked={w.enabled} onChange={(e) => void attempt('ワークフローの切り替え', () => api.workflowEnabled(w.id, e.currentTarget.checked))} />
          ワークフロー「{w.name}」で自動で実装する
        </label>
        <span class="spacer" />
        {w.running && w.sessionId && <a class="fb-link" href={href.session(w.sessionId)}>進み具合を見る</a>}
        <button class="btn btn-sm" onClick={openWorkflow}>設定</button>
      </div>
      {status && <p class="help fb-help">{status}</p>}
      {w.error && <p class="help fb-help error-text">{w.error}</p>}
    </div>
  );
};
