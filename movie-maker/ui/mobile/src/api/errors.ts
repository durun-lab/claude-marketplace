/** 登録の応答の後に資格が効かなかった（端末がクッキーを保存しなかった）ときの、画面だけの code。 */
export const PAIRING_NOT_KEPT = "pairing_not_kept";

/**
 * PC の失敗の `code` から、画面に出す英語の文言を引く。PC の `message` は日本語のログ向けなので画面に出さない。
 * code は `mm-serve` の人の待ち受け（`mobile/`）と `ServeError` の応答、通信の失敗（`network`）。
 */
const MESSAGES: Record<string, string> = {
  bad_request: "The PC could not read this request.",
  conflict: "The project changed on the PC. Reload and try again.",
  forbidden: "The PC refused this action.",
  forbidden_host: "Open this page from the Tailscale address of the PC.",
  forbidden_origin: "Open this page from the Tailscale address of the PC.",
  future_base: "The project changed on the PC. Reload and try again.",
  internal: "Something went wrong on the PC.",
  invalid: "The PC could not apply this change.",
  locked: "That part is locked on the PC.",
  login_mismatch: "This device was paired by a different Tailscale user.",
  missing_mobile_header: "Open this page from the Tailscale address of the PC.",
  name_taken: "Another device already uses this name. Pick a different name.",
  network: "Can't reach the PC. Check the connection.",
  no_device: "This device is not paired. Pair it again from the PC.",
  no_tailnet_login: "Open this page through Tailscale on your tailnet.",
  not_found: "The PC could not find it. Reload and try again.",
  not_latest: "Someone edited after this, so it can't be undone here.",
  not_on_mobile: "This change can't be applied from the phone. Turn on Decide on PC to send it as a proposal.",
  [PAIRING_NOT_KEPT]:
    "The PC paired this device, but this browser did not keep the sign-in cookie. Allow cookies for this site (turn off Block All Cookies), make a new code on the PC, and pair again.",
  pairing_unavailable: "The code has expired or was already used. Make a new code on the PC.",
  pairing_wrong_code: "That code is wrong. Check the code on the PC and try again.",
  unavailable: "The PC is busy. Try again in a moment.",
  unknown_device: "This device is not paired. Pair it again from the PC.",
};

/** 失敗の `code` の英語の文言。知らない code は HTTP の状態つきの汎用の文言にする。 */
export const errorMessage = (code: string, status: number): string =>
  MESSAGES[code] ?? (status > 0 ? `The PC could not do this (HTTP ${status}).` : MESSAGES.network ?? "");
