import { useEffect, useRef, useState } from "preact/hooks";
import type { Device, DevicesApi, MobileStatus, Pairing } from "./api.ts";
import { formatCode, formatPairedAt, remaining } from "./model.ts";
import { useDevices } from "./useDevices.ts";

export interface DevicesPanelProps {
  api: DevicesApi;
  /** 増えるたびに読み直す（タブを開いたとき）。 */
  refreshKey?: number;
  /** 時刻の基準（ミリ秒）。固定すると符号の残りが進まない。スクリーンショットとテストで使う。 */
  nowMs?: number;
}

type Running = Extract<MobileStatus, { state: "running" }>;

/**
 * 登録した端末 1 件。登録を外すときは、その場で確かめてから外す。確かめている間は Cancel にフォーカスを置き、
 * Enter の押し続けや Escape で外してしまわないようにする。
 */
const DeviceRow = ({ device, busy, onUnpair }: { device: Device; busy: boolean; onUnpair: () => void }) => {
  const [confirming, setConfirming] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  const unpair = useRef<HTMLButtonElement>(null);
  const asked = useRef(false);
  useEffect(() => {
    // やめたときは Unpair に戻し、キーボードの位置を見失わせない。
    if (confirming) cancel.current?.focus();
    else if (asked.current) unpair.current?.focus();
    asked.current = confirming;
  }, [confirming]);
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") setConfirming(false);
  };
  return (
    <li class="mm-devices-row">
      <span class="mm-devices-name">{device.name}</span>
      <span class="mm-devices-meta" title={device.login}>
        Paired {formatPairedAt(device.paired_at_ms)}
      </span>
      {confirming ? (
        <span class="mm-devices-confirm" onKeyDown={onKeyDown}>
          <button type="button" class="mm-devices-button mm-devices-danger" disabled={busy} onClick={onUnpair}>
            Unpair {device.name}
          </button>
          <button type="button" class="mm-devices-button" ref={cancel} onClick={() => setConfirming(false)}>
            Cancel
          </button>
        </span>
      ) : (
        <button type="button" class="mm-devices-button" ref={unpair} aria-label={`Unpair ${device.name}`} onClick={() => setConfirming(true)}>
          Unpair
        </button>
      )}
    </li>
  );
};

const PairingCard = ({ pairing, nowMs, busy, onNew, onClose }: { pairing: Pairing; nowMs: number; busy: boolean; onNew: () => void; onClose: () => void }) => {
  const left = remaining(pairing.expiresAtMs, nowMs);
  return (
    <div class="mm-devices-pairing" role="group" aria-label="Pairing code">
      {pairing.qrSvg && <div class="mm-devices-qr" role="img" aria-label="QR code for the pairing page" dangerouslySetInnerHTML={{ __html: pairing.qrSvg }} />}
      <p class="mm-devices-hint">{pairing.qrSvg ? "Scan with the phone camera, or type the code on the pairing page." : "Type the code on the phone's pairing page."}</p>
      <p class={`mm-devices-code${left ? "" : " mm-devices-expired"}`} aria-label="Pairing code">
        {formatCode(pairing.code)}
      </p>
      <p class="mm-devices-hint" role="status">
        {left ? `Expires in ${left}` : "This code has expired."}
      </p>
      <div class="mm-devices-actions">
        <button type="button" class="mm-devices-button" disabled={busy} onClick={onNew}>
          New code
        </button>
        <button type="button" class="mm-devices-button" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
};

const Notices = ({ status, onRecheck }: { status: Running; onRecheck: () => void }) => {
  if (status.notices.length === 0) return null;
  const needsServe = status.notices.some((n) => n.kind === "serveNotConfigured");
  return (
    <div class="mm-devices-notices">
      {status.notices.map((n) => (
        <p key={n.kind} class="mm-devices-notice" role="alert">
          {n.message}
        </p>
      ))}
      {needsServe && <code class="mm-devices-command">{status.serveCommand}</code>}
      <div class="mm-devices-actions">
        <button type="button" class="mm-devices-button" onClick={onRecheck}>
          Check again
        </button>
      </div>
    </div>
  );
};

/** スマホの登録。上から、待ち受けの状態と足りない設定、符号と QR、登録した端末の一覧。 */
export const DevicesPanel = ({ api, refreshKey = 0, nowMs }: DevicesPanelProps) => {
  const d = useDevices(api, refreshKey, nowMs);
  const running = d.status?.state === "running" ? d.status : null;
  return (
    <section class="mm-devices" aria-label="Devices">
      {d.error && (
        <p class="mm-devices-error" role="alert">
          {d.error}
        </p>
      )}
      {d.status?.state === "starting" && <p class="mm-devices-hint">Starting the phone listener...</p>}
      {d.status?.state === "unavailable" && (
        <p class="mm-devices-notice" role="alert">
          {d.status.message}
        </p>
      )}
      {running && (
        <>
          <p class="mm-devices-hint">{running.url ? `Phone address: ${running.url}` : "Phone address: unknown"}</p>
          <Notices status={running} onRecheck={d.recheck} />
          {!d.pairing && (
            <button type="button" class="mm-devices-button mm-devices-primary" disabled={d.busy} onClick={d.addDevice}>
              Add device
            </button>
          )}
        </>
      )}
      {d.justPaired && (
        <p class="mm-devices-success" role="status">
          Paired {d.justPaired}.
        </p>
      )}
      {d.pairing && <PairingCard pairing={d.pairing} nowMs={d.nowMs} busy={d.busy} onNew={d.addDevice} onClose={d.closePairing} />}
      {running && (
        <>
          <h3 class="mm-devices-heading">Paired devices</h3>
          {d.devices.length === 0 ? (
            <p class="mm-devices-hint">No devices yet.</p>
          ) : (
            <ul class="mm-devices-list" aria-label="Paired devices">
              {d.devices.map((device) => (
                <DeviceRow key={device.id} device={device} busy={d.busy} onUnpair={() => d.unpair(device.id)} />
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
};
