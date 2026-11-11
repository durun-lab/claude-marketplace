// ワークフローの「設定」。workflows.json の 1 つの定義を、画面で直して書き込む。
import { useState } from 'preact/hooks';
import { COMPACT_WINDOW_MAX, COMPACT_WINDOW_MIN, EFFORT_LEVELS, MODELS, type WorkflowInfo, type WorkflowSettings, type WorkflowTrigger } from '../shared/types.ts';
import { api, attempt } from './api.ts';
import { Choice } from './ui.tsx';

type Schedule = Extract<WorkflowTrigger, { kind: 'schedule' }>;
type EventKind = Exclude<WorkflowTrigger['kind'], 'schedule'>;

const DAYS = ['日', '月', '火', '水', '木', '金', '土'];
const EVENTS: { kind: EventKind; label: string }[] = [
  { kind: 'github-review', label: 'GitHub のレビュー依頼' },
  { kind: 'slack-mention', label: 'Slack のメンション' },
  { kind: 'jira-mention', label: 'JIRA のメンション' },
  { kind: 'feedback', label: '改善要望' },
];
const NEW_SCHEDULE: Schedule = { kind: 'schedule', at: '09:00' };
const DEFAULT_BURST_MINUTES = 5;

const settingsOf = ({ name, cwd, theme, prompt, triggers, burstMinutes, mode, compact, compactWindow, maxSessions, model, effort }: WorkflowInfo): WorkflowSettings => ({ name, cwd, theme, prompt, triggers, burstMinutes, mode, compact, compactWindow, maxSessions, model, effort });

/** モードに関わらない項目は落として送る。workflows.json に読まれない項目を残さないため。 */
const cleaned = (s: WorkflowSettings): WorkflowSettings => ({
  ...s,
  burstMinutes: s.burstMinutes || undefined,
  compact: s.mode === 'continue' && s.compact ? true : undefined,
  // 範囲の外の値は定義に読まれず黙って消えるので、Claude Code が受け付ける範囲に収めて送る。
  compactWindow: s.mode === 'continue' && s.compactWindow ? Math.min(Math.max(s.compactWindow, COMPACT_WINDOW_MIN), COMPACT_WINDOW_MAX) : undefined,
  maxSessions: s.mode === 'new' && s.maxSessions && s.maxSessions > 1 ? s.maxSessions : undefined,
  theme: s.theme?.trim() || undefined,
  model: s.model || undefined,
  effort: s.effort || undefined,
});

const ScheduleRow = ({ t, onChange, onRemove }: { t: Schedule; onChange: (t: Schedule) => void; onRemove: () => void }) => {
  const toggle = (d: number) => {
    const days = (t.days ?? []).includes(d) ? (t.days ?? []).filter((x) => x !== d) : [...(t.days ?? []), d].sort();
    onChange({ kind: 'schedule', at: t.at, ...(days.length ? { days } : {}) });
  };
  return (
    <div class="workflow-schedule">
      <input type="time" class="input input-sm input-auto" value={t.at} onInput={(e) => onChange({ ...t, at: e.currentTarget.value })} />
      {DAYS.map((label, d) => (
        <label key={label} class="workflow-day">
          <input type="checkbox" checked={!!t.days?.includes(d)} onChange={() => toggle(d)} />
          {label}
        </label>
      ))}
      <span class="help">{t.days?.length ? '' : '毎日'}</span>
      <button class="xp-task-link" onClick={onRemove}>消す</button>
    </div>
  );
};

const BurstField = ({ minutes, onChange }: { minutes?: number; onChange: (minutes?: number) => void }) => (
  <fieldset class="workflow-fieldset">
    <legend>バースト制御</legend>
    <label class="workflow-day">
      <input type="checkbox" checked={!!minutes} onChange={(e) => onChange(e.currentTarget.checked ? DEFAULT_BURST_MINUTES : undefined)} />
      レビュー依頼とメンションが続けて届いたら、最後の 1 件から
      <input type="number" min={1} class="input input-sm workflow-number" disabled={!minutes} value={minutes ?? DEFAULT_BURST_MINUTES} onInput={(e) => onChange(Number(e.currentTarget.value) || 1)} />
      分待ってまとめる
    </label>
  </fieldset>
);

const TriggersField = ({ triggers, onChange }: { triggers: WorkflowTrigger[]; onChange: (t: WorkflowTrigger[]) => void }) => {
  const has = (kind: EventKind) => triggers.some((t) => t.kind === kind);
  const setEvent = (kind: EventKind, on: boolean) => onChange(on ? [...triggers, { kind } as WorkflowTrigger] : triggers.filter((t) => t.kind !== kind));
  const setAt = (i: number, next?: Schedule) => onChange(triggers.flatMap((t, j) => (j !== i ? [t] : next ? [next] : [])));
  return (
    <fieldset class="workflow-fieldset">
      <legend>トリガー</legend>
      <div class="workflow-events">
        {EVENTS.map(({ kind, label }) => (
          <label key={kind} class="workflow-day">
            <input type="checkbox" checked={has(kind)} onChange={(e) => setEvent(kind, e.currentTarget.checked)} />
            {label}
          </label>
        ))}
      </div>
      {triggers.map((t, i) => t.kind === 'schedule' && <ScheduleRow key={i} t={t} onChange={(n) => setAt(i, n)} onRemove={() => setAt(i)} />)}
      <button class="xp-task-link" onClick={() => onChange([...triggers, NEW_SCHEDULE])}>時刻を足す</button>
    </fieldset>
  );
};

const ActionField = ({ s, set }: { s: WorkflowSettings; set: (patch: Partial<WorkflowSettings>) => void }) => (
  <fieldset class="workflow-fieldset">
    <legend>アクション</legend>
    <label class="workflow-day">
      <input type="radio" name="workflow-mode" checked={s.mode === 'new'} onChange={() => set({ mode: 'new' })} />
      イベントごとに新しいセッション。同時に
      <input type="number" min={1} class="input input-sm workflow-number" disabled={s.mode !== 'new'} value={s.maxSessions ?? 1} onInput={(e) => set({ maxSessions: Number(e.currentTarget.value) || 1 })} />
      まで
    </label>
    <label class="workflow-day">
      <input type="radio" name="workflow-mode" checked={s.mode === 'continue'} onChange={() => set({ mode: 'continue' })} />
      1 つのセッションを引き継ぐ
    </label>
    <label class="workflow-day workflow-indent">
      <input type="checkbox" disabled={s.mode !== 'continue'} checked={!!s.compact} onChange={(e) => set({ compact: e.currentTarget.checked })} />
      イベントを送る前に、毎回前回までの会話を compact する
    </label>
    <label class="workflow-day workflow-indent" title="次にセッションを起動したときから効きます">
      会話が
      <input
        type="number"
        min={COMPACT_WINDOW_MIN / 1000}
        max={COMPACT_WINDOW_MAX / 1000}
        step={10}
        class="input input-sm workflow-number"
        disabled={s.mode !== 'continue'}
        placeholder="既定"
        value={s.compactWindow ? s.compactWindow / 1000 : ''}
        onInput={(e) => set({ compactWindow: Number(e.currentTarget.value) * 1000 || undefined })}
      />
      千トークンに達したら自動で compact する（空なら Claude Code の設定）
    </label>
    <div class="workflow-events">
      <Choice label="モデル" value={s.model} options={MODELS} onChange={(model) => set({ model })} />
      <Choice label="effort" value={s.effort} options={EFFORT_LEVELS} onChange={(effort) => set({ effort })} />
    </div>
  </fieldset>
);

/** 1 つのワークフローの設定。保存すると workflows.json に書き込み、閉じる。開いた後に定義が書き換わっても、直している値は保つ。 */
export const WorkflowForm = ({ w, themes, onClose }: { w: WorkflowInfo; themes: string[]; onClose: () => void }) => {
  const [s, setS] = useState(() => settingsOf(w));
  const set = (patch: Partial<WorkflowSettings>) => setS((prev) => ({ ...prev, ...patch }));
  const save = async () => {
    const saved = await attempt('ワークフローの保存', async () => {
      await api.workflowSettings(w.id, cleaned(s));
      return true;
    });
    if (saved) onClose();
  };
  return (
    <div class="stack workflow-form">
      <label class="inline-field">
        <span class="nowrap">名前</span>
        <input class="input" value={s.name} onInput={(e) => set({ name: e.currentTarget.value })} />
      </label>
      <label class="inline-field">
        <span class="nowrap">ディレクトリ</span>
        <input class="input mono" value={s.cwd} onInput={(e) => set({ cwd: e.currentTarget.value })} />
      </label>
      <label class="inline-field">
        <span class="nowrap">テーマ</span>
        <input class="input" list="workflow-themes" placeholder="省くとリポジトリ名" value={s.theme ?? ''} onInput={(e) => set({ theme: e.currentTarget.value })} />
        <datalist id="workflow-themes">{themes.map((t) => <option key={t} value={t} />)}</datalist>
      </label>
      <TriggersField triggers={s.triggers} onChange={(triggers) => set({ triggers })} />
      <BurstField minutes={s.burstMinutes} onChange={(burstMinutes) => set({ burstMinutes })} />
      <ActionField s={s} set={set} />
      <label class="stack">
        <span>依頼</span>
        <textarea class="input workflow-prompt" value={s.prompt} onInput={(e) => set({ prompt: e.currentTarget.value })} />
      </label>
      <div class="actions">
        <span class="help">id: {w.id}</span>
        <span class="spacer" />
        <button class="btn" onClick={onClose}>キャンセル</button>
        <button class="btn btn-primary" disabled={!s.cwd.trim() || !s.prompt.trim()} onClick={save}>保存</button>
      </div>
    </div>
  );
};
