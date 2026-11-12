// コントロール パネルの「Claude アカウント」で開くウィンドウ。推論にだけ使う追加アカウントを足し、外す。
import { useState } from 'preact/hooks';
import type { Snapshot } from '../shared/types.ts';
import { api, attempt } from './api.ts';
import { type Float, FloatFrame, dock, floatUp } from './Float.tsx';
import { toast } from './toast.tsx';

/** ウィンドウの保存キー。ダッシュボードのウィジェットではないので、scope は空にし、タブの id にもこのキーを入れる。 */
export const ACCOUNTS_KEY = 'accounts';
// 一覧と入力欄と説明がちょうど収まる大きさ（px）。
const SIZE = { w: 480, h: 420 };

export const openAccounts = (): void => floatUp(ACCOUNTS_KEY, '', { id: ACCOUNTS_KEY }, SIZE);
const close = () => dock(ACCOUNTS_KEY, '', ACCOUNTS_KEY);

const Accounts = ({ accounts }: { accounts: string[] }) => {
  const [name, setName] = useState('');
  const [token, setToken] = useState('');
  const [busy, setBusy] = useState(false);
  const run = async (label: string, done: string, fn: () => Promise<unknown>): Promise<boolean> => {
    setBusy(true);
    const ok = (await attempt(label, async () => (await fn(), true))) ?? false;
    setBusy(false);
    if (ok) toast(done);
    return ok;
  };
  const add = async (e: Event) => {
    e.preventDefault();
    if (busy || !name.trim() || !token.trim()) return;
    if (await run('追加アカウントの登録', `${name.trim()} を登録しました`, () => api.addAccount(name.trim(), token.trim()))) {
      setName('');
      setToken('');
    }
  };
  return (
    <div class="stack">
      <p class="help">プライマリアカウントは claude にログインしているアカウントで、コネクタ（claude.ai の MCP サーバー、Artifact、Claude in Chrome）はこちらにだけ付きます。追加アカウントは推論にだけ使います。</p>
      {accounts.length === 0 ? (
        <p class="empty">追加アカウントはありません。</p>
      ) : (
        <ul class="stack">
          {accounts.map((a) => (
            <li key={a} class="actions">
              <span class="mono">{a}</span>
              <span class="spacer" />
              <button class="btn btn-sm" disabled={busy} onClick={() => void run('追加アカウントの削除', `${a} を外しました`, () => api.removeAccount(a))}>削除</button>
            </li>
          ))}
        </ul>
      )}
      <form class="stack" onSubmit={add}>
        <label class="inline-field">
          <span class="nowrap">名前</span>
          <input class="input mono" placeholder="sub" pattern="[A-Za-z0-9_\-]{1,40}" value={name} onInput={(e) => setName(e.currentTarget.value)} />
        </label>
        <label class="inline-field">
          <span class="nowrap">トークン</span>
          <input class="input mono" type="password" autocomplete="off" placeholder="sk-ant-oat01-..." value={token} onInput={(e) => setToken(e.currentTarget.value)} />
        </label>
        <div class="actions">
          <span class="help">トークンは、追加アカウントで claude setup-token を実行して得ます。同じ名前で登録すると置き換えます。</span>
          <span class="spacer" />
          <button class="btn btn-primary" disabled={busy || !name.trim() || !token.trim()}>登録</button>
        </div>
      </form>
    </div>
  );
};

export const AccountsFloat = ({ f, snapshot }: { f: Float; snapshot: Snapshot }) => (
  <FloatFrame f={f} title="Claude アカウント" icon="👥" controls={[{ kind: 'close', label: '閉じる', onClick: close }]}>
    <Accounts accounts={snapshot.accounts ?? []} />
  </FloatFrame>
);
