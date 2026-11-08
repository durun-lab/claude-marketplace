import { useState } from "preact/hooks";
import type { Controller } from "../controller.ts";

/** 登録コードの文字数。PC のアプリが出す登録コードと同じ。 */
export const PAIR_CODE_LENGTH = 8;

/** 登録コードに使わない文字（空白、ハイフン）を除き、大文字にそろえる。 */
export const normalizeCode = (raw: string): string =>
  raw.replace(/[\s-]/g, "").toUpperCase().slice(0, PAIR_CODE_LENGTH);

/** QR の URL に入った登録コード（`?code=`）を読む。 */
export const codeFromSearch = (search: string): string => normalizeCode(new URLSearchParams(search).get("code") ?? "");

/** 端末の名前の初期値。PC の端末の一覧で見分けるための名前。 */
export const guessDeviceName = (ua: string): string => {
  if (/iPhone/.test(ua)) return "iPhone";
  if (/iPad/.test(ua)) return "iPad";
  if (/Android/.test(ua)) return "Android";
  return "Mobile";
};

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export interface PairScreenProps {
  controller: Controller;
  initialCode: string;
  initialName: string;
}

const usePair = (controller: Controller, code: string, name: string) => {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (e: Event): Promise<void> => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await controller.pair(code, name.trim());
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  };
  return { error, busy, submit };
};

/** 端末の登録。PC のアプリが出した登録コードを入れる（QR で開いた Safari と、ホーム画面の Web アプリでクッキーが分かれる場合の経路）。 */
export const PairScreen = ({ controller, initialCode, initialName }: PairScreenProps) => {
  const [code, setCode] = useState(initialCode);
  const [name, setName] = useState(initialName);
  const { error, busy, submit } = usePair(controller, code, name);
  const ready = code.length === PAIR_CODE_LENGTH && name.trim() !== "" && !busy;
  return (
    <form class="pair" onSubmit={(e) => void submit(e)}>
      <h1>Pair this device</h1>
      <p class="hint">Open Settings on the PC app, press Add device, and enter the 8-character code.</p>
      <label>
        Code
        <input
          class="code"
          value={code}
          onInput={(e) => setCode(normalizeCode(e.currentTarget.value))}
          autocapitalize="characters"
          autocomplete="off"
          autocorrect="off"
          spellcheck={false}
          enterkeyhint="go"
          maxLength={PAIR_CODE_LENGTH + 1}
          aria-label="Pairing code"
        />
      </label>
      <label>
        Device name
        <input value={name} onInput={(e) => setName(e.currentTarget.value)} autocomplete="off" aria-label="Device name" />
      </label>
      {error && (
        <p class="error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" class="primary" disabled={!ready}>
        {busy ? "Pairing..." : "Pair"}
      </button>
    </form>
  );
};
