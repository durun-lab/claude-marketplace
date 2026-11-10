// WebView とアプリの間の橋。画面は持たない。ui/dist の shell に入れる（アプリが initialization_script で読み込む）。
// WebView にフォーカスがあるときのキーをアプリのキーの表に送り、再生位置と Timeline の選択（Rust の Tauri イベント）を
// shell のパネルが受ける DOM のイベントにつなぐ。パネルが投げる素材の選択とシークの頼みはアプリに送る。
// 再生位置は Rust の時計が持つ。ここでは受けた位置を、送られてから経った時間だけ進めて知らせる。
(() => {
  const { invoke, Channel } = window.__TAURI__.core;
  const LAYER_SELECTED_EVENT = "mm:layer-selected";
  // Media パネルが素材を選んだときに投げる DOM のイベント（detail.asset が素材の ID）。Source で開く。
  const ASSET_SELECTED_EVENT = "mm:asset-selected";
  // 再生位置を shell の Inspector と Graph に知らせる DOM のイベントと、前後のキーフレームへのシークの頼み（detail.time は秒）。
  const PLAYHEAD_EVENT = "mm:playhead";
  const SEEK_EVENT = "mm:seek";
  // キー操作を再生に渡さない入力部品。値を打つ・選ぶ操作が Space や矢印を使う。
  const TEXT_ENTRY = "input, select, textarea, [contenteditable]";
  // ボタンや単語などフォーカスを受ける部品。編集のキーはここから送らない（Timeline と Source はネイティブのサーフェスで、キーは Rust 側が受ける）。
  const FOCUSABLE = "button, a[href], summary, [role=button], [role=tab], [role=option], [tabindex]";
  // keys.rs の BINDINGS のうち編集（clip.*、edit.*、source.*）のキーと同じ。WebView の部品にフォーカスがあるときの Delete などが Timeline の編集にならないよう、送る前に選ぶ。
  const EDIT_KEYS = new Set([
    "KeyI", "KeyO", "Comma", "Period", "KeyB", "Delete", "Backspace", "Ctrl+KeyZ", "Ctrl+KeyY", "Ctrl+Shift+KeyZ", "Alt+KeyX",
  ]);
  const nowUs = () => Math.round((performance.timeOrigin + performance.now()) * 1000);

  // 開いているコンポジションの fps と長さ（project_info）。再生位置を進める上限と、再生位置のイベントの fps に使う。
  let info = null;
  let last = null;

  const shown = () => {
    if (!last) return 0;
    const age = last.playing ? Math.max(0, nowUs() - last.sentUs) / 1e6 : 0;
    const t = last.t + age;
    return info ? Math.min(t, info.duration) : t;
  };

  const act = (action, value) => invoke("transport", { action, value: value ?? null });
  const openSource = (asset) => invoke("source_open", { asset });
  const keyName = (e) =>
    (e.ctrlKey ? `Ctrl+${e.shiftKey ? "Shift+" : ""}` : "") + (e.altKey ? "Alt+" : "") + e.code;

  // 画面に出した時刻を ack で返し、アプリが通知の遅れと WebView の時計の誤差を測る。
  const onTick = (msg) => {
    const recv = nowUs();
    last = { t: msg.t, sentUs: msg.sent_us, playing: msg.playing };
    requestAnimationFrame(() => {
      const t = shown();
      if (info) window.dispatchEvent(new CustomEvent(PLAYHEAD_EVENT, { detail: { time: t, fps: info.fps } }));
      invoke("sync_ack", { seq: msg.seq, recvUs: recv, rafUs: nowUs(), shownT: t });
    });
  };

  const start = async () => {
    info = await invoke("project_info");
    const channel = new Channel();
    channel.onmessage = onTick;
    await invoke("subscribe", { onTick: channel });
    window.addEventListener("keydown", (e) => {
      // パネルが処理したキー（単語の矢印など）は二重に送らない。
      if (e.repeat || e.defaultPrevented) return;
      const target = e.target instanceof Element ? e.target : null;
      if (target?.closest(TEXT_ENTRY)) return;
      const key = keyName(e);
      if (EDIT_KEYS.has(key) && target?.closest(FOCUSABLE)) return;
      if (e.code === "Space") e.preventDefault();
      invoke("key_event", { source: "webview", key });
    });
    // 選択かプロジェクトのデータが変わったら、Inspector に中身を渡し、fps と長さを取り直す。
    window.__TAURI__.event.listen(LAYER_SELECTED_EVENT, async (e) => {
      window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail: e.payload }));
      info = await invoke("project_info");
    });
    window.addEventListener(ASSET_SELECTED_EVENT, (e) => e.detail?.asset && openSource(e.detail.asset));
    window.addEventListener(SEEK_EVENT, (e) => Number.isFinite(e.detail?.time) && act("seek", e.detail.time));
  };

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
