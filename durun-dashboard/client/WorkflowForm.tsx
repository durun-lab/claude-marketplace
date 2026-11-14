// ワークフローの「設定」。1 つのワークフローのフォルダーの workflow.json と agent を、画面で直して書き込む。
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

const settingsOf = ({ name, cwd, theme, prompt, triggers, burstMinutes, mode, compact, compactWindow, maxSessions, model, effort, account }: WorkflowInfo): WorkflowSettings => ({ name, cwd, theme, prompt, triggers, burstMinutes, mode, compact, compactWindow, maxSessions, model, effort, account });

/** モードに関わらない項目は落として送る。定義に読まれない項目を残さないため。 */
const cleaned = (s: WorkflowSettings): WorkflowSettings => ({
  ...s,
  burstMinutes: s.burstMinutes || undefined,
  compact: s.mode === 'continue' && s.compact ? true : undefined,
  // 範囲の外の値は定義に読まれず黙って消えるので、Claude Code が受け付ける範囲に収めて送る。
  compactWindow: s.mode === 'continue' && s.compactWindow ? Math.min(Math.max(s.compactWindow, COMPACT_WINDOW_MIN), COMPACT_WINDOW_MAX) : undefined,
  maxSessions: s.mode === 'new' && s.maxSessions && s.maxSessions > 1 ? s.maxSessions : undefined,
  account: s.account || undefined,
  theme: s.theme?.trim() || undefined,
  model: s.model || undefined,
  effort: s.effort || undefined,
});

type MonthKey = 'dates' | 'businessDays';
const MONTH: { key: MonthKey | ''; label: string }[] = [
  { key: '', label: '毎月の指定なし' },
  { key: 'dates', label: '毎月の日付' },
  { key: 'businessDays', label: '毎月の営業日' },
];

/** 「1, 15, -1」のような打った文字を、日の番号の並びにする。0 と数でないものは捨てる。 */
const parseDayList = (text: string): number[] => text.split(/[\s,、]+/).map(Number).filter((n) => Number.isInteger(n) && n !== 0);

/** 曜日は何も選ばなければ毎日。毎月の日付と営業日は、どちらか 1 つを合わせて絞れる。曜日を切り替えても、日付と営業日の指定は残す。 */
const ScheduleFields = ({ t, onChange }: { t: Schedule; onChange: (t: Schedule) => void }) => {
  const month: MonthKey | '' = t.businessDays ? 'businessDays' : t.dates ? 'dates' : '';
  // 「-」だけ打った途中の文字も消さないよう、打った文字は手元に持つ。
  const [text, setText] = useState((month ? t[month] : undefined)?.join(', ') ?? '');
  const withDays = (days: number[]): Schedule => {
    const { days: _days, ...rest } = t;
    return days.length ? { ...rest, days } : rest;
  };
  const withMonth = (key: MonthKey | '', list: number[]): Schedule => {
    const { dates: _dates, businessDays: _business, ...rest } = t;
    return key && list.length ? { ...rest, [key]: list } : rest;
  };
  const toggle = (d: number) => onChange(withDays((t.days ?? []).includes(d) ? (t.days ?? []).filter((x) => x !== d) : [...(t.days ?? []), d].sort()));
  return (
    <>
      <input type="time" class="input input-sm input-auto" aria-label="時刻" value={t.at} onInput={(e) => onChange({ ...t, at: e.currentTarget.value })} />
      {DAYS.map((label, d) => (
        <label key={label} class="workflow-day">
          <input type="checkbox" checked={!!t.days?.includes(d)} onChange={() => toggle(d)} />
          {label}
        </label>
      ))}
      <select class="input input-sm input-auto" aria-label="毎月の指定" value={month} onChange={(e) => onChange(withMonth(e.currentTarget.value as MonthKey | '', parseDayList(text).length ? parseDayList(text) : [1]))}>
        {MONTH.map((m) => <option key={m.key} value={m.key}>{m.label}</option>)}
      </select>
      {month && (
        <input
          class="input input-sm workflow-days-input"
          aria-label={month === 'dates' ? '日付' : '何営業日目'}
          placeholder="1, 15, -1"
          title={month === 'dates' ? '月の日付をカンマで区切って書きます。負の数は月末から数え、-1 が末日です' : '月の何営業日目かをカンマで区切って書きます。営業日は土日と日本の祝日を除いた日で、負の数は月末から数え、-1 が最終営業日です'}
          value={text}
          onInput={(e) => {
            setText(e.currentTarget.value);
            const list = parseDayList(e.currentTarget.value);
            if (list.length) onChange(withMonth(month, list));
          }}
        />
      )}
      <span class="help">{month === 'businessDays' ? '営業日目（-1 は最終営業日）' : month === 'dates' ? '日（-1 は末日）' : t.days?.length ? '' : '毎日'}</span>
    </>
  );
};

const KIND_LABEL: Record<WorkflowTrigger['kind'], string> = { schedule: '時刻', ...Object.fromEntries(EVENTS.map((e) => [e.kind, e.label])) } as Record<WorkflowTrigger['kind'], string>;

/** 1 つのトリガー。種類を選び、時刻なら日の指定を続ける。同じイベントのトリガーは 1 つまでなので、ほかの行で使っている種類は選べない。 */
const TriggerRow = ({ t, used, onChange, onRemove }: { t: WorkflowTrigger; used: Set<string>; onChange: (t: WorkflowTrigger) => void; onRemove: () => void }) => (
  <div class="workflow-schedule">
    <select class="input input-sm input-auto" aria-label="トリガーの種類" value={t.kind} onChange={(e) => onChange(e.currentTarget.value === 'schedule' ? NEW_SCHEDULE : ({ kind: e.currentTarget.value } as WorkflowTrigger))}>
      {(Object.keys(KIND_LABEL) as WorkflowTrigger['kind'][]).map((k) => <option key={k} value={k} disabled={k !== 'schedule' && k !== t.kind && used.has(k)}>{KIND_LABEL[k]}</option>)}
    </select>
    {t.kind === 'schedule' && <ScheduleFields t={t} onChange={onChange} />}
    <button type="button" class="btn btn-sm" title="このトリガーを消す" aria-label="このトリガーを消す" onClick={onRemove}>×</button>
  </div>
);

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
  const used = new Set(triggers.map((t) => t.kind));
  const setAt = (i: number, next?: WorkflowTrigger) => onChange(triggers.flatMap((t, j) => (j !== i ? [t] : next ? [next] : [])));
  return (
    <fieldset class="workflow-fieldset">
      <legend>トリガー</legend>
      {/* 種類を変えると欄の中身が変わるので、種類も key に入れて作り直す。 */}
      {triggers.map((t, i) => <TriggerRow key={`${i}-${t.kind}`} t={t} used={used} onChange={(n) => setAt(i, n)} onRemove={() => setAt(i)} />)}
      {!triggers.length && <span class="help">トリガーがありません。「今すぐ実行」でだけ動きます。</span>}
      <div><button type="button" class="btn btn-sm" title="トリガーを足す" aria-label="トリガーを足す" onClick={() => onChange([...triggers, NEW_SCHEDULE])}>+</button></div>
    </fieldset>
  );
};

const ActionField = ({ s, set, accounts }: { s: WorkflowSettings; set: (patch: Partial<WorkflowSettings>) => void; accounts: string[] }) => (
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
      {(accounts.length > 0 || s.account) && <Choice label="アカウント" value={s.account} options={s.account && !accounts.includes(s.account) ? [...accounts, s.account] : accounts} empty="プライマリ" title="追加アカウントには claude.ai のコネクタがありません" onChange={(account) => set({ account })} />}
    </div>
  </fieldset>
);

/** 1 つのワークフローの設定。保存するとワークフローのフォルダーに書き込み、閉じる。開いた後に定義が書き換わっても、直している値は保つ。 */
export const WorkflowForm = ({ w, themes, accounts, onClose }: { w: WorkflowInfo; themes: string[]; accounts: string[]; onClose: () => void }) => {
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
      <ActionField s={s} set={set} accounts={accounts} />
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
