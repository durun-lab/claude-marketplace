// WebView とアプリの間の中継スクリプト。画面は持たない。ui/dist の shell に入れる（アプリが initialization_script で読み込む）。
// WebView にフォーカスがあるときのキーをアプリのキーの表に送り、再生位置と Timeline の選択（Rust の Tauri イベント）を
// shell のパネルが受ける DOM のイベントにつなぐ。パネルが投げる素材の選択とシークの頼みはアプリに送る。
// 再生位置は Rust の時計が持つ。ここでは受けた位置を、送られてから経った時間だけ進めて知らせる。
(() => {
  const { invoke, Channel } = window.__TAURI__.core;
  const LAYER_SELECTED_EVENT = "mm:layer-selected";
  // shell が選択の DOM のイベントを聞き始めたときに投げる頼み。今の選択を読んで LAYER_SELECTED_EVENT で答える。
  const LAYER_SELECTION_REQUEST_EVENT = "mm:layer-selection-request";
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
    "KeyI", "KeyO", ",", ".", "KeyB", "Delete", "Backspace", "Ctrl+KeyZ", "Ctrl+KeyY", "Ctrl+Shift+KeyZ", "Alt+KeyX",
  ]);
  // code の名前のキーのうち、Shift を名前に付けるもの（英字、数字、矢印）。keys.rs の with_modifiers と同じ。
  const SHIFT_NAMED = /^(Key[A-Z]|Digit[0-9]|Arrow(Up|Down|Left|Right))$/;
  const nowUs = () => Math.round((performance.timeOrigin + performance.now()) * 1000);

  // 開いているコンポジションの fps と長さ（始めは project_info、後は選択の中身）。再生位置を進める上限と、再生位置のイベントの fps に使う。
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
  // キーの表（keys.rs）の名前。英字、数字、矢印、Space などは code の名前（KeyZ）、記号は押された文字（key の 1 字）を名前にし、
  // JIS と US のキー配列で記号のキーの位置が違っても同じ名前にする。Shift は英字、数字、矢印に付け、記号には付けない
  // （押された文字に入っている）。Space などほかの名前のキーは Ctrl と一緒のときだけ付ける。
  const keyName = (e) => {
    const symbol = e.key.length === 1 && !SHIFT_NAMED.test(e.code) && e.code !== "Space";
    const shift = e.shiftKey && !symbol && (SHIFT_NAMED.test(e.code) || e.ctrlKey);
    const mods = (e.ctrlKey ? "Ctrl+" : "") + (shift ? "Shift+" : "") + (e.altKey ? "Alt+" : "");
    return mods + (symbol ? e.key : e.code);
  };

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

  // 選択の中身のコンポジションの要約から fps と長さを取り直す。project_info はレイヤーの一覧まで運ぶので、選択と編集のたびには読まない。
  const takeFpsAndDuration = (selection) => {
    const comp = selection?.composition;
    if (comp) info = { fps: comp.fps, duration: comp.duration };
  };

  const readSelection = async () => {
    const detail = await invoke("layer_selection");
    if (!detail) return;
    takeFpsAndDuration(detail);
    window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail }));
  };

  // 選択の Tauri イベントを購読し終えたら解ける。購読より前に Rust が送った選択（起動時の 1 回目）は届かないので、
  // shell の頼みには購読の後に今の選択を読んで答える。このスクリプトは shell より先に走るので、頼みを取りこぼさない。
  let markSubscribed;
  const subscribed = new Promise((resolve) => {
    markSubscribed = resolve;
  });
  window.addEventListener(LAYER_SELECTION_REQUEST_EVENT, () => subscribed.then(readSelection));

  const start = async () => {
    info = await invoke("project_info");
    const channel = new Channel();
    channel.onmessage = onTick;
    await invoke("subscribe", { onTick: channel });
    window.addEventListener("keydown", (e) => {
      // パネルが処理したキー（単語の矢印など）は二重に送らない。押し続けの繰り返しも送り、繰り返すかは keys.rs が決める。
      if (e.defaultPrevented) return;
      const target = e.target instanceof Element ? e.target : null;
      if (target?.closest(TEXT_ENTRY)) return;
      const key = keyName(e);
      if (EDIT_KEYS.has(key) && target?.closest(FOCUSABLE)) return;
      if (e.code === "Space") e.preventDefault();
      invoke("key_event", { source: "webview", key, repeat: e.repeat });
    });
    // 選択かプロジェクトのデータが変わったら、Inspector に中身を渡し、fps と長さを取り直す。
    window.__TAURI__.event.listen(LAYER_SELECTED_EVENT, (e) => {
      window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail: e.payload }));
      takeFpsAndDuration(e.payload);
    }).then(markSubscribed);
    window.addEventListener(ASSET_SELECTED_EVENT, (e) => e.detail?.asset && openSource(e.detail.asset));
    window.addEventListener(SEEK_EVENT, (e) => Number.isFinite(e.detail?.time) && act("seek", e.detail.time));
  };

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
