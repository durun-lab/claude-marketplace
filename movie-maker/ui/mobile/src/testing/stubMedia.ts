// jsdom は <video> の再生を実装せず、呼ぶと未実装のエラーを標準エラーに出す。画面のテストが読み込んで黙らせる。
HTMLMediaElement.prototype.pause = () => undefined;
HTMLMediaElement.prototype.load = () => undefined;
HTMLMediaElement.prototype.play = () => Promise.resolve();
