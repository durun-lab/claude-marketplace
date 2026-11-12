// コントロール パネルで設定できる、~/.durun-dashboard/.env.local の項目。XP のコントロール パネルのように、分類ごとにアイコンを付けてまとめる。
// DURUN_DASHBOARD_PORT と DURUN_DASHBOARD_DATA_DIR は、.env.local の場所そのものを決めるので、ここでは扱わない。

/** secret は、値を画面に返さず、設定してあるかだけを返す項目。 */
export type EnvSetting = { key: string; label: string; help: string; secret?: boolean; placeholder?: string };
export type EnvGroup = { id: string; title: string; icon: string; help: string; items: EnvSetting[] };

/** 項目ごとの今の値。secret の項目は value を返さない。 */
export type EnvValues = Record<string, { value?: string; set: boolean }>;

export const ENV_GROUPS: EnvGroup[] = [
  {
    id: 'jira',
    title: 'JIRA',
    icon: '🎫',
    help: '課題の題名と状態の表示、JIRA Issues のウィジェット、ブランチ名や依頼からの課題キーの拾い出し',
    items: [
      { key: 'JIRA_BASE_URL', label: 'サイト', help: 'JIRA のサイトの URL', placeholder: 'https://example.atlassian.net' },
      { key: 'JIRA_PROJECT_KEYS', label: 'プロジェクト', help: 'ブランチ名、PR タイトル、人間の依頼から課題キーを拾うプロジェクト（カンマ区切り）', placeholder: 'PROJ,CATS' },
      { key: 'JIRA_EMAIL', label: 'メールアドレス', help: 'API トークンを作ったアカウントのメールアドレス' },
      { key: 'JIRA_API_TOKEN', label: 'API トークン', help: 'Atlassian のアカウントで作った API トークン', secret: true },
    ],
  },
  {
    id: 'notion',
    title: 'Notion',
    icon: '📓',
    help: 'Notion のページをダッシュボードで読む',
    items: [
      { key: 'NOTION_TOKEN', label: 'インテグレーションのトークン', help: 'https://www.notion.so/profile/integrations のトークン。読みたいページをそのインテグレーションに共有しておく', secret: true },
    ],
  },
  {
    id: 'claude-api',
    title: 'Claude API',
    icon: '🤖',
    help: 'ツールを使わない補助の問い合わせを、サブスクリプションの使用制限でなく API のクレジットで動かす',
    items: [
      { key: 'DURUN_DASHBOARD_ANTHROPIC_API_KEY', label: 'API キー', help: '無ければ claude -p で動かす。セッションが API のキーで動かないよう、ANTHROPIC_API_KEY ではなくこの名前で持つ', secret: true },
    ],
  },
  {
    id: 'proofread',
    title: '文章校正',
    icon: '✒️',
    help: '疑問が説明の区切りで解けたかを、TypeSafe の Jev に確かめさせる',
    items: [{ key: 'TYPESAFE_AI_API_KEY', label: 'TypeSafe の API キー', help: '無ければ haiku で確かめる', secret: true }],
  },
  {
    id: 'gcloud',
    title: 'Google Cloud',
    icon: '☁️',
    help: 'Claude が Google Cloud の認証を求めたときに動かすコマンド',
    items: [
      { key: 'GCLOUD_LOGIN_COMMAND', label: 'ログインのコマンド', help: '人間が OK を押したときに動かす', placeholder: 'gcloud auth login' },
      { key: 'GCLOUD_CHECK_COMMAND', label: '確かめのコマンド', help: '成功すれば、尋ねずに認証済みと返す', placeholder: 'gcloud auth print-access-token' },
    ],
  },
  {
    id: 'terminal',
    title: 'コマンドプロンプト',
    icon: '💻',
    help: 'アプリ「コマンドプロンプト」が、screen や tmux のセッションにアタッチするコマンド',
    items: [
      {
        key: 'TERMINAL_ATTACH_COMMAND',
        label: 'アタッチのコマンド',
        help: 'シェル（Windows では cmd.exe）で動かす。セッションの画面から開くと、そのセッションの作業ディレクトリで動かす',
        placeholder: 'tmux new-session -A -s "$(basename "$PWD")"',
      },
    ],
  },
  {
    id: 'export',
    title: '資料の書き出し',
    icon: '🖨️',
    help: '資料を PDF と PNG に書き出す Chrome',
    items: [{ key: 'CHROME_PATH', label: 'Chrome の場所', help: '無ければ macOS と Windows の標準の場所を使う' }],
  },
  {
    id: 'fl',
    title: 'FL Studio',
    icon: '🎹',
    help: 'アプリ「FL Connect」が FL Studio とつなぐための場所とポート',
    items: [
      { key: 'FL_USER_DATA', label: 'ユーザーデータのフォルダー', help: 'FL の File settings の「User data folder」。無ければ、ドキュメントの下などから探す', placeholder: 'D:\\Image-Line\\FL Studio' },
      { key: 'FL_STUDIO_PATH', label: 'FL Studio の場所', help: '既定は C:\\Program Files\\Image-Line\\FL Studio 2026\\FL64.exe' },
      { key: 'FL_CONNECT_MIDI_IN', label: 'MIDI の入力ポート', help: '既定は FL Connect (B)' },
      { key: 'FL_CONNECT_MIDI_OUT', label: 'MIDI の出力ポート', help: '既定は FL Connect (B)。自分に返るポートは、入力用と出力用の 2 つを作って指す' },
      { key: 'FL_CONNECT_DEBUG_PORT', label: 'WebView2 のデバッグのポート', help: '既定は 9222', placeholder: '9222' },
    ],
  },
];

export const ENV_SETTINGS: EnvSetting[] = ENV_GROUPS.flatMap((g) => g.items);
