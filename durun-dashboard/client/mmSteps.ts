// ムービーメーカーのウィンドウに出す「次にすること」と、ログの文。サーバーが送った生の値から決めるので、テストで確かめる。
import type { MmChange, MmLogEntry, MmMobileView, MovieMakerView } from '../shared/types.ts';

export type MmAction = 'install' | 'launch';
/** 手順の 1 段。link は開く先、command はコピーして人が打つコマンド。 */
export type MmGuideItem = { text: string; link?: { label: string; href: string }; command?: string };
export type MmStep = { key: string; tone: 'warn' | 'info'; title: string; text?: string; guide?: MmGuideItem[]; action?: { kind: MmAction; label: string; disabled?: string } };

const installStep = (v: MovieMakerView): MmStep | undefined => {
  const i = v.install;
  if (i.busy) return { key: 'install', tone: 'info', title: 'movie-maker を入れています', text: '数分から数十分かかります。進み具合は下に出ます。' };
  if (i.error) return { key: 'install', tone: 'warn', title: '入れられませんでした', text: i.error, action: i.mode === 'build' ? { kind: 'install', label: 'やり直す' } : undefined };
  if (!i.path) {
    return i.mode === 'build'
      ? { key: 'install', tone: 'warn', title: 'movie-maker が入っていません', text: 'ソースからビルドして入れます（UI と Rust のビルド）。', action: { kind: 'install', label: 'ビルドして入れる' } }
      : { key: 'install', tone: 'warn', title: 'movie-maker が入っていません', action: { kind: 'install', label: '入れる' } };
  }
  if (i.mode === 'build' && i.commit && i.sourceCommit && i.commit !== i.sourceCommit) {
    return { key: 'rebuild', tone: 'info', title: 'ソースが新しくなっています', text: `入れたもの ${i.commit}、ソース ${i.sourceCommit}。`, action: { kind: 'install', label: '作り直す' } };
  }
  return undefined;
};

/** 次にすること。上から順に片づける。済んでいれば空。 */
export const mmSteps = (v: MovieMakerView): MmStep[] => {
  const steps: MmStep[] = [];
  const install = installStep(v);
  if (install) steps.push(install);
  if (!v.project) {
    steps.push({ key: 'project', tone: 'warn', title: 'プロジェクトのフォルダーを選んでください', text: 'project.mmdb のあるフォルダーか、新しく作るフォルダー。' });
  } else if (v.link.status === 'waiting') {
    const disabled = v.install.path ? undefined : '先に movie-maker を入れてください';
    steps.push({ key: 'launch', tone: 'info', title: 'movie-maker でこのプロジェクトを開いてください', text: v.link.message, action: { kind: 'launch', label: v.hasStore ? '起動する' : 'プロジェクトを作って起動する', disabled } });
  }
  return steps;
};

/** ダッシュボードから実行せず、人が打つ設定のコマンド。Serve が有効になるまで返らず、ユーザーの Tailscale の設定を変えるため。 */
export const SERVE_COMMAND = 'tailscale serve --bg --https=8443 http://127.0.0.1:7338';
const TAILSCALE_DOWNLOAD = 'https://tailscale.com/download';
const TAILSCALE_DNS_ADMIN = 'https://login.tailscale.com/admin/dns';

const listenerStep = (v: MovieMakerView): MmStep => {
  const i = v.install;
  if (i.mode === 'build' && i.commit && i.sourceCommit && i.commit !== i.sourceCommit) {
    return { key: 'mobile-listener', tone: 'warn', title: 'movie-maker が古く、モバイル版の待ち受けがありません', text: 'ソースが新しくなっています。作り直して、起動し直してください。', action: { kind: 'install', label: '作り直す' } };
  }
  if (v.link.status === 'connected') {
    return { key: 'mobile-listener', tone: 'warn', title: 'movie-maker がモバイル版の待ち受け（127.0.0.1:7338）を開いていません', text: '古いビルドの可能性があります。作り直して起動し直してから、確かめ直してください。' };
  }
  const text = v.project ? '上の「起動する」で movie-maker を起動すると、モバイル版の待ち受けも一緒に始まります。起動したら、確かめ直してください。' : 'プロジェクトのフォルダーを選んで movie-maker を起動すると、モバイル版の待ち受けも一緒に始まります。';
  return { key: 'mobile-listener', tone: 'warn', title: 'movie-maker が起動していません', text };
};

/** モバイル版のリンクを出せない理由ごとの手順。3 つの条件がそろっていれば空で、リンクを出す。 */
export const mobileSteps = (v: MovieMakerView, m: MmMobileView): MmStep[] => {
  const steps: MmStep[] = [];
  if (!m.tailscale) {
    steps.push({ key: 'mobile-tailscale', tone: 'warn', title: 'Tailscale が動いていません', text: 'モバイル版は tailnet の中だけで開けます。Tailscale を入れてログインしてください。tailnet での名前が読めると、リンクが出ます。', guide: [{ text: 'Tailscale を入れて、ログインする', link: { label: 'ダウンロード', href: TAILSCALE_DOWNLOAD } }] });
  } else if (!m.serve) {
    steps.push({
      key: 'mobile-serve',
      tone: 'warn',
      title: 'tailscale serve に、モバイル版の中継がありません',
      text: 'PC で 1 回だけ、次の 2 段を行います。',
      guide: [
        { text: 'tailnet で Serve と HTTPS 証明書を有効にする（初めてだけ。有効になるまで次のコマンドは待ち続けます）', link: { label: '管理画面を開く', href: m.enableUrl ?? TAILSCALE_DNS_ADMIN } },
        { text: 'ターミナルで、設定のコマンドを打つ', command: SERVE_COMMAND },
      ],
    });
  }
  return m.listening ? steps : [...steps, listenerStep(v)];
};

/** 変更の通知を文にする。取り消しとやり直しは、戻した元のトランザクションと、それをした人の種類を添える。 */
export const describeMmChange = (c: MmChange): string => {
  const touched = c.touched.length ? `（${c.touched.length} か所）` : '';
  const mobile = c.actor === 'mobile' ? '（モバイル）' : '';
  if (c.kind === 'apply') return `${c.name}${touched}${mobile}`;
  const verb = c.kind === 'undo' ? '取り消し' : 'やり直し';
  const who = c.of?.actor === 'mobile' ? 'モバイルの' : c.of?.actor === 'gui' ? '人間の' : 'エージェントの';
  return c.of ? `${verb}: ${c.of.name}（${who}操作）` : verb;
};

export const describeMmEntry = (e: MmLogEntry): string => (e.who === 'you' ? describeMmChange(e.change) : `${e.tool}${e.ok ? '' : '（失敗）'}`);
