// コントロール パネルの分類「Claude Code の設定」。~/.claude/settings.json の項目を、スキーマの型に合った入力欄で直す。
import { useState } from 'preact/hooks';
import type { ClaudeSettingField } from '../shared/claudeSettings.ts';
import { api, attempt } from './api.ts';
import { useFetch } from './hooks.ts';
import { toast } from './toast.tsx';
import { Loading } from './ui.tsx';

const UNSET = '';

const textOf = (f: ClaudeSettingField, v: unknown): string => (v === undefined ? '' : f.kind === 'json' ? JSON.stringify(v, null, 2) : String(v));

/** 入力欄の文字を settings.json に書く値にする。JSON として読めなければ投げる。 */
const valueOf = (f: ClaudeSettingField, text: string): unknown => {
  if (f.kind === 'boolean') return text === 'true';
  if (f.kind === 'number') return Number(text);
  if (f.kind === 'json') return JSON.parse(text);
  return text;
};

/** 1 つの項目。選ぶ欄は選んだらすぐ書き、打つ欄は保存を押したら書く。空にして書くと、項目を消して既定に戻す。 */
const SettingRow = ({ f, current, onSaved }: { f: ClaudeSettingField; current: unknown; onSaved: () => void }) => {
  const [text, setText] = useState(textOf(f, current));
  const [busy, setBusy] = useState(false);
  const save = async (next: string) => {
    let value: unknown;
    try {
      value = next.trim() === UNSET ? undefined : valueOf(f, next.trim());
    } catch (e) {
      toast(`JSON として読めません（${(e as Error).message}）`);
      return;
    }
    if (f.kind === 'number' && Number.isNaN(value)) return toast('数で入れてください');
    setBusy(true);
    const ok = await attempt(`${f.key} の保存`, () => api.setClaudeSetting(f.key, value));
    setBusy(false);
    if (ok === undefined) return;
    toast('保存しました。Claude Code は settings.json の変更を読み直します');
    onSaved();
  };
  const set = current !== undefined;
  const choices = f.kind === 'boolean' ? ['true', 'false'] : f.options;
  return (
    <form
      class="cpl-setting"
      onSubmit={(e) => {
        e.preventDefault();
        if (!busy) void save(text);
      }}
    >
      <label class="cpl-label mono" for={`cpl-claude-${f.key}`}>{f.key}</label>
      <div class="actions">
        {choices ? (
          <select id={`cpl-claude-${f.key}`} class="input" disabled={busy} value={text} onChange={(e) => void save(e.currentTarget.value)}>
            <option value={UNSET}>（既定）</option>
            {choices.map((c) => <option key={c} value={c}>{c}</option>)}
          </select>
        ) : f.kind === 'json' ? (
          <textarea id={`cpl-claude-${f.key}`} class="input mono" rows={Math.min(Math.max(text.split('\n').length, 2), 12)} value={text} onInput={(e) => setText(e.currentTarget.value)} />
        ) : (
          <input id={`cpl-claude-${f.key}`} class="input mono" type={f.kind === 'number' ? 'number' : 'text'} value={text} onInput={(e) => setText(e.currentTarget.value)} />
        )}
        {!choices && <button class="btn btn-sm btn-primary" disabled={busy || text === textOf(f, current)}>保存</button>}
        {set && !choices && <button type="button" class="btn btn-sm" disabled={busy} title="この項目を消して、既定に戻す" onClick={() => void save(UNSET)}>消す</button>}
      </div>
      <span class="help">{f.description}</span>
    </form>
  );
};

export const ClaudeSettingsView = () => {
  const settings = useFetch((fresh) => api.claudeSettings(fresh), []);
  const [query, setQuery] = useState('');
  const [onlySet, setOnlySet] = useState(false);
  if (settings.error) return <p class="recycle-error">{settings.error}</p>;
  if (!settings.data) return <Loading />;
  const { fields, values } = settings.data;
  const q = query.trim().toLowerCase();
  const shown = fields.filter((f) => (!onlySet || f.key in values) && (!q || f.key.toLowerCase().includes(q) || f.description.toLowerCase().includes(q)));
  return (
    <>
      <div class="cpl-setting">
        <div class="actions">
          <input class="input" type="search" placeholder="項目の名前か説明で絞り込む" aria-label="項目を絞り込む" value={query} onInput={(e) => setQuery(e.currentTarget.value)} />
          <label><input type="checkbox" checked={onlySet} onChange={() => setOnlySet(!onlySet)} /> 設定済みだけ</label>
        </div>
      </div>
      {/* 保存した後に読み直した値で入力欄を作り直すよう、今の値を key に入れる。 */}
      {shown.map((f) => <SettingRow key={`${f.key}=${JSON.stringify(values[f.key])}`} f={f} current={values[f.key]} onSaved={settings.reload} />)}
    </>
  );
};
