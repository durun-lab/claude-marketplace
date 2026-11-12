// スタートメニューの「コントロール パネル」で開くウィンドウ。XP のコントロール パネルのカテゴリの表示のように、分類をアイコンで並べ、選んだ分類の項目を直す。
// .env.local の項目はここで書き換え、ほかの設定はそれぞれのウィンドウを開く。
import { useState } from 'preact/hooks';
import { ENV_GROUPS, type EnvGroup, type EnvSetting, type EnvValues } from '../shared/envSettings.ts';
import { openAccounts } from './Accounts.tsx';
import { api, attempt } from './api.ts';
import { AppIcon } from './apps.tsx';
import { type Float, FloatFrame, dock, floatUp } from './Float.tsx';
import { useFetch } from './hooks.ts';
import { toast } from './toast.tsx';
import { Loading } from './ui.tsx';
import { openWallpaper } from './Wallpaper.tsx';

/** ウィンドウの保存キー。ダッシュボードのウィジェットではないので、scope は空にし、タブの id にもこのキーを入れる。 */
export const CONTROL_PANEL_KEY = 'control-panel';
const TITLE = 'コントロール パネル';
const ICON = '🖥️';
const SIZE = { w: 760, h: 480 };

export const openControlPanel = (): void => floatUp(CONTROL_PANEL_KEY, '', { id: CONTROL_PANEL_KEY }, SIZE);
const close = () => dock(CONTROL_PANEL_KEY, '', CONTROL_PANEL_KEY);

/** .env.local の項目でなく、ほかのウィンドウで設定するもの。 */
const APPLETS = [
  { title: '壁紙', icon: '🖼️', help: 'Windows XP のスキンの壁紙を、画像のファイルか URL で変える', open: openWallpaper },
  { title: 'Claude アカウント', icon: '👥', help: '推論にだけ使う追加の Claude アカウントを登録する', open: openAccounts },
];

const SAVED = '保存しました。サーバーを起動し直して反映します';

/** 1 つの項目。secret の項目は今の値を出さず、設定してあるかだけを出す。空にして保存すると、既定に戻す。 */
const SettingRow = ({ s, current, onSaved }: { s: EnvSetting; current?: { value?: string; set: boolean }; onSaved: () => void }) => {
  const [value, setValue] = useState(current?.value ?? '');
  const [busy, setBusy] = useState(false);
  const save = async (next: string) => {
    setBusy(true);
    const ok = await attempt(`${s.label}の保存`, () => api.setEnvValue(s.key, next));
    setBusy(false);
    if (!ok) return;
    toast(SAVED);
    if (s.secret) setValue('');
    onSaved();
  };
  const changed = s.secret ? value.trim() !== '' : value.trim() !== (current?.value ?? '');
  return (
    <form
      class="cpl-setting"
      onSubmit={(e) => {
        e.preventDefault();
        if (changed && !busy) void save(value.trim());
      }}
    >
      <label class="cpl-label" for={`cpl-${s.key}`}>
        {s.label} <span class="mono cpl-key">{s.key}</span>
      </label>
      <div class="actions">
        <input
          id={`cpl-${s.key}`}
          class="input mono"
          type={s.secret ? 'password' : 'text'}
          autocomplete="off"
          placeholder={s.secret && current?.set ? '設定済み（入れ直すと置き換えます）' : (s.placeholder ?? '')}
          value={value}
          onInput={(e) => setValue(e.currentTarget.value)}
        />
        <button class="btn btn-sm btn-primary" disabled={busy || !changed}>保存</button>
        {current?.set && <button type="button" class="btn btn-sm" disabled={busy} title="この行を消して、既定に戻す" onClick={() => void save('')}>消す</button>}
      </div>
      <span class="help">{s.help}</span>
    </form>
  );
};

const GroupView = ({ g, values, onSaved }: { g: EnvGroup; values?: EnvValues; onSaved: () => void }) => (
  <div class="cpl-group">
    <h2 class="cpl-heading"><AppIcon icon={g.icon} size={32} />{g.title}</h2>
    <p class="help">{g.help}</p>
    {/* 保存した後に読み直した値で入力欄を作り直すよう、今の値を key に入れる。 */}
    {values ? g.items.map((s) => <SettingRow key={`${s.key}=${values[s.key]?.value ?? ''}`} s={s} current={values[s.key]} onSaved={onSaved} />) : <Loading />}
  </div>
);

const Categories = ({ onPick }: { onPick: (g: EnvGroup) => void }) => (
  <div class="cpl-categories-wrap">
    <h2 class="cpl-pick">作業する分野を選びます</h2>
    <div class="cpl-categories">
      {ENV_GROUPS.map((g) => (
        <button key={g.id} class="cpl-category" onClick={() => onPick(g)}>
          <AppIcon icon={g.icon} size={32} />
          <span><b>{g.title}</b><small>{g.help}</small></span>
        </button>
      ))}
      {APPLETS.map((a) => (
        <button key={a.title} class="cpl-category" onClick={a.open}>
          <AppIcon icon={a.icon} size={32} />
          <span><b>{a.title}</b><small>{a.help}</small></span>
        </button>
      ))}
    </div>
  </div>
);

export const ControlPanelFloat = ({ f }: { f: Float }) => {
  const [group, setGroup] = useState<EnvGroup>();
  const values = useFetch(() => api.envValues(), []);
  return (
    <FloatFrame f={f} title={group ? `${TITLE} - ${group.title}` : TITLE} icon={ICON} controls={[{ kind: 'close', label: '閉じる', onClick: close }]}>
      <div class="recycle">
        <div class="recycle-main">
          <aside class="recycle-tasks">
            <section class="xp-task">
              <h3 class="xp-task-title">{TITLE}</h3>
              <div class="xp-task-body">
                <button class="xp-task-link" disabled={!group} onClick={() => setGroup(undefined)}><AppIcon icon={ICON} />カテゴリの表示に戻る</button>
              </div>
            </section>
            <section class="xp-task">
              <h3 class="xp-task-title">関連項目</h3>
              <div class="xp-task-body">
                {ENV_GROUPS.map((g) => (
                  <button key={g.id} class="xp-task-link" aria-pressed={g === group} onClick={() => setGroup(g)}><AppIcon icon={g.icon} />{g.title}</button>
                ))}
              </div>
            </section>
            <section class="xp-task">
              <h3 class="xp-task-title">詳細</h3>
              <div class="xp-task-body">
                <p>~/.durun-dashboard/.env.local に書きます。保存すると、サーバーが起動し直して新しい値を読みます。</p>
                <p>トークンとキーは、設定してあるかだけを出し、値は出しません。</p>
                {values.error && <p class="recycle-error">{values.error}</p>}
              </div>
            </section>
          </aside>
          <div class="recycle-list">{group ? <GroupView g={group} values={values.data} onSaved={values.reload} /> : <Categories onPick={setGroup} />}</div>
        </div>
      </div>
    </FloatFrame>
  );
};
