// movie-maker のモバイル版の決まった値。サーバーの判定と画面の手順で同じ値を使う。

/** 人の待ち受け（127.0.0.1）のポート。movie-maker の MOBILE_PORT と同じ。 */
export const MOBILE_PORT = 7338;

/** モバイル版の待ち受けの tailnet 上の HTTPS のポート。movie-maker の MOBILE_HTTPS_PORT と同じ。 */
export const MOBILE_HTTPS_PORT = 8443;

/** 人の待ち受けを tailnet に出すコマンド。movie-maker の設定の手順（12-mobile.md）と同じ。 */
export const SERVE_COMMAND = `tailscale serve --bg --https=${MOBILE_HTTPS_PORT} http://127.0.0.1:${MOBILE_PORT}`;
