// スタートメニューの「コントロール パネル」で開くウィンドウ。XP のコントロール パネルのカテゴリの表示のように、分類をアイコンで並べ、選んだ分類の項目を直す。
// 画面とエージェントの設定、.env.local の項目はここで変え、壁紙とアカウントはそれぞれのウィンドウを開く。
import { useState } from 'preact/hooks';
import { ENV_GROUPS, type EnvGroup, type EnvSetting, type EnvValues } from '../shared/envSettings.ts';
import { AGENT_PRIORITIES, type AgentPriority, type Snapshot } from '../shared/types.ts';
import { openAccounts } from './Accounts.tsx';
import { api, attempt } from './api.ts';
import { AppIcon } from './apps.tsx';
import { type Float, FloatFrame, dock, floatUp, latestFloat, setFloatSettings } from './Float.tsx';
import { useFetch } from './hooks.ts';
import type { WidgetSettings } from './layout.ts';
import { setLightMode, useLightMode } from './lightMode.ts';
import { SKINS, setSkin, useSkin, type Skin } from './skin.ts';
import { toast } from './toast.tsx';
import { Loading } from './ui.tsx';
import { openWallpaper } from './Wallpaper.tsx';

/** ウィンドウの保存キー。ダッシュボードのウィジェットではないので、scope は空にし、タブの id にもこのキーを入れる。 */
export const CONTROL_PANEL_KEY = 'control-panel';
const TITLE = 'コントロール パネル';
const ICON = '🖥️';
const SIZE = { w: 760, h: 480 };

const close = () => dock(CONTROL_PANEL_KEY, '', CONTROL_PANEL_KEY);

type Category = { id: string; title: string; icon: string; help: string; open?: () => void };

/** XP のカテゴリの表示のように、見た目とパフォーマンスを先に、アカウントを最後に並べる。open のある分類は、ほかのウィンドウで設定する。 */
export const CATEGORIES: Category[] = [
  { id: 'display', title: 'デスクトップの表示', icon: '🎨', help: 'スキン、壁紙、軽量モードを変える' },
  { id: 'performance', title: 'パフォーマンス', icon: '⚙️', help: 'エージェントの CPU の優先度と、止まったセッションの自動継続を変える' },
  ...ENV_GROUPS,
  { id: 'accounts', title: 'Claude アカウント', icon: '👥', help: '推論にだけ使う追加の Claude アカウントを登録する', open: openAccounts },
];

/** 分類を指定すると、その分類を開いた状態で出す。既に開いていれば、手前に出して分類を切り替える。 */
export const openControlPanel = (category?: string): void => {
  const c = CATEGORIES.find((x) => x.id === category);
  if (c?.open) return c.open();
  const settings: WidgetSettings = c ? { category: c.id } : {};
  floatUp(CONTROL_PANEL_KEY, '', { id: CONTROL_PANEL_KEY, settings }, SIZE);
  const f = latestFloat({ key: CONTROL_PANEL_KEY, scope: '', tab: { id: CONTROL_PANEL_KEY } } as Float);
  if (f) setFloatSettings(f, settings);
};

const PRIORITY_LABEL: Record<AgentPriority, string> = { normal: '通常', low: '低め', idle: 'アイドル' };

const DisplayView = () => {
  const skin = useSkin();
  const light = useLightMode();
  return (
    <>
      <div class="cpl-setting" role="radiogroup" aria-label="スキン">
        <span class="cpl-label">スキン</span>
        {Object.entries(SKINS).map(([id, label]) => (
          <label key={id}><input type="radio" name="cpl-skin" checked={skin === id} onChange={() => setSkin(id as Skin)} /> {label}</label>
        ))}
      </div>
      <div class="cpl-setting">
        <span class="cpl-label">壁紙</span>
        <div class="actions"><button class="btn btn-sm" onClick={openWallpaper}>壁紙を変える...</button></div>
        <span class="help">Windows XP のスキンの壁紙を、画像のファイルか URL で変える。</span>
      </div>
      <div class="cpl-setting">
        <label class="cpl-label"><input type="checkbox" checked={light} onChange={() => setLightMode(!light)} /> 軽量モード</label>
        <span class="help">通信と描画を減らす。更新は 3 秒おきになり、アニメーションとイルカが止まる。タブが裏にある間は更新も通知も止まる。</span>
      </div>
    </>
  );
};

const PerformanceView = ({ snapshot }: { snapshot: Snapshot }) => {
  const priority = snapshot.agentPriority ?? 'normal';
  const autoContinue = !!snapshot.autoContinue;
  return (
    <>
      <div class="cpl-setting" role="radiogroup" aria-label="エージェントの優先度">
        <span class="cpl-label">エージェントの優先度</span>
        {AGENT_PRIORITIES.map((p) => (
          <label key={p}><input type="radio" name="cpl-priority" checked={priority === p} onChange={() => void attempt('エージェントの優先度の切り替え', () => api.agentPriority(p))} /> {PRIORITY_LABEL[p]}</label>
        ))}
        <span class="help">低めやアイドルにすると、ゲームやほかの作業に CPU を先に回す。</span>
      </div>
      <div class="cpl-setting">
        <label class="cpl-label"><input type="checkbox" checked={autoContinue} onChange={() => void attempt('自動継続モードの切り替え', () => api.autoContinue(!autoContinue))} /> 自動継続モード</label>
        <span class="help">スリープや通信の途切れで止まったセッションを、確認のダイアログを出さずに、API に届くようになったら続けさせる。Claude のログインが切れて止まったセッションも、ログインが戻ったら続けさせる。</span>
      </div>
    </>
  );
};

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

const EnvItems = ({ g, values, onSaved }: { g: EnvGroup; values?: EnvValues; onSaved: () => void }) =>
  // 保存した後に読み直した値で入力欄を作り直すよう、今の値を key に入れる。
  values ? <>{g.items.map((s) => <SettingRow key={`${s.key}=${values[s.key]?.value ?? ''}`} s={s} current={values[s.key]} onSaved={onSaved} />)}</> : <Loading />;

const Categories = ({ onPick }: { onPick: (id: string) => void }) => (
  <div class="cpl-categories-wrap">
    <h2 class="cpl-pick">作業する分野を選びます</h2>
    <div class="cpl-categories">
      {CATEGORIES.map((c) => (
        <button key={c.id} class="cpl-category" onClick={() => (c.open ? c.open() : onPick(c.id))}>
          <AppIcon icon={c.icon} size={32} />
          <span><b>{c.title}</b><small>{c.help}</small></span>
        </button>
      ))}
    </div>
  </div>
);

export const ControlPanelFloat = ({ f, snapshot }: { f: Float; snapshot: Snapshot }) => {
  const group = CATEGORIES.find((c) => c.id === f.tab.settings?.category);
  const setGroup = (id?: string) => setFloatSettings(f, id ? { category: id } : {});
  const env = ENV_GROUPS.find((g) => g === group);
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
                {CATEGORIES.map((c) => (
                  <button key={c.id} class="xp-task-link" aria-pressed={c === group} onClick={() => (c.open ? c.open() : setGroup(c.id))}><AppIcon icon={c.icon} />{c.title}</button>
                ))}
              </div>
            </section>
            <section class="xp-task">
              <h3 class="xp-task-title">詳細</h3>
              <div class="xp-task-body">
                <p>サービスとアプリの分類は ~/.durun-dashboard/.env.local に書きます。保存すると、サーバーが起動し直して新しい値を読みます。</p>
                <p>トークンとキーは、設定してあるかだけを出し、値は出しません。</p>
                {values.error && <p class="recycle-error">{values.error}</p>}
              </div>
            </section>
          </aside>
          <div class="recycle-list">
            {group ? (
              <div class="cpl-group">
                <h2 class="cpl-heading"><AppIcon icon={group.icon} size={32} />{group.title}</h2>
                <p class="help">{group.help}</p>
                {group.id === 'display' && <DisplayView />}
                {group.id === 'performance' && <PerformanceView snapshot={snapshot} />}
                {env && <EnvItems g={env} values={values.data} onSaved={values.reload} />}
              </div>
            ) : (
              <Categories onPick={setGroup} />
            )}
          </div>
        </div>
      </div>
    </FloatFrame>
  );
};
