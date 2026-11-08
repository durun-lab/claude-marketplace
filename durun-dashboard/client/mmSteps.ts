// ムービーメーカーのウィンドウに出す「次にすること」と、ログの文。サーバーが送った生の値から決めるので、テストで確かめる。
import { MOBILE_PORT, SERVE_COMMAND } from '../shared/mmMobile.ts';
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

const TAILSCALE_DOWNLOAD = 'https://tailscale.com/download';
const TAILSCALE_DNS_ADMIN = 'https://login.tailscale.com/admin/dns';

/** 人の待ち受けが応えない理由ごとの段。起動と作り直しのボタンは上の「次にすること」にあるので、ここではそこへ導く。 */
const listenerStep = (v: MovieMakerView): MmStep => {
  const i = v.install;
  const key = 'mobile-listener';
  if (i.mode === 'build' && i.commit && i.sourceCommit && i.commit !== i.sourceCommit) {
    return { key, tone: 'warn', title: 'movie-maker が古く、モバイル版の待ち受けがありません', text: '上の「作り直す」で作り直して、起動し直してください。' };
  }
  if (v.link.status === 'connected') {
    return { key, tone: 'warn', title: `モバイル版の待ち受け（127.0.0.1:${MOBILE_PORT}）が開いていません`, text: 'movie-maker の Devices パネルで理由を確かめてください。古いビルドなら、作り直して起動し直します。' };
  }
  const text = v.project ? '上の「起動する」で起動すると、モバイル版の待ち受けも始まります。' : 'プロジェクトのフォルダーを選んで起動すると、モバイル版の待ち受けも始まります。';
  return { key, tone: 'warn', title: 'movie-maker が起動していません', text };
};

const serveStep = (m: MmMobileView): MmStep => ({
  key: 'mobile-serve',
  tone: 'warn',
  title: 'tailscale serve にモバイル版の中継がありません',
  text: 'PC で 1 回だけ、次の 2 つをします。',
  guide: [
    { text: 'Serve と HTTPS 証明書を有効にする', link: { label: '管理画面を開く', href: m.enableUrl ?? TAILSCALE_DNS_ADMIN } },
    // コマンドは Serve が有効になるまで返らず、Tailscale の設定を変えるので、ダッシュボードから実行しない。
    { text: '管理者でないターミナルで打つ（1 の前に打つと、有効になるまで待ち続けます）', command: SERVE_COMMAND },
  ],
});

/** モバイル版のリンクを出せない理由ごとの手順。3 つの条件がそろっていれば空で、リンクを出す。 */
export const mobileSteps = (v: MovieMakerView, m: MmMobileView): MmStep[] => {
  const steps: MmStep[] = [];
  if (!m.tailscale) {
    steps.push({ key: 'mobile-tailscale', tone: 'warn', title: 'Tailscale が動いていません', text: 'モバイル版は tailnet の中だけで開けます。', guide: [{ text: 'PC に Tailscale を入れて、ログインする', link: { label: 'ダウンロード', href: TAILSCALE_DOWNLOAD } }] });
  } else if (!m.serve) {
    steps.push(serveStep(m));
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
