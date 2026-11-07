import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { Device, DevicesApi, MobileStatus, Pairing } from "./api.ts";

const STARTING_RETRY_MS = 1000;
/** 符号を出している間、スマホが登録したかを確かめる間隔。 */
const PAIRING_POLL_MS = 2000;
const TICK_MS = 1000;

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

type Report = (m: string | null) => void;

/** 状態と端末の一覧を読む。待ち受けを始めている間は読み直し続ける。最後に頼んだ読み直しだけを使う。 */
const useLoad = (api: DevicesApi, refreshKey: number, report: Report) => {
  const [status, setStatus] = useState<MobileStatus | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const loads = useRef(0);
  const load = useCallback(async () => {
    const id = ++loads.current;
    try {
      const s = await api.status();
      const list = s.state === "running" ? await api.list() : [];
      if (id !== loads.current) return;
      setStatus(s);
      setDevices(list);
      report(null);
    } catch (e) {
      if (id === loads.current) report(message(e));
    }
  }, [api, report]);
  useEffect(() => {
    void load();
  }, [load, refreshKey]);
  useEffect(() => {
    if (status?.state !== "starting") return;
    const t = setTimeout(() => void load(), STARTING_RETRY_MS);
    return () => clearTimeout(t);
  }, [status, load]);
  return { status, devices, setDevices, load };
};

/** 符号を出している間、登録の完了を一覧の増加で見張り、期限の残りを進める。 */
const usePairing = (api: DevicesApi, setDevices: (d: Device[]) => void, report: Report, fixedNowMs?: number) => {
  const [pairing, setPairing] = useState<Pairing | null>(null);
  const [justPaired, setJustPaired] = useState<string | null>(null);
  const [now, setNow] = useState(fixedNowMs ?? Date.now());
  const known = useRef<Set<string>>(new Set());
  useEffect(() => {
    if (!pairing) return;
    const poll = setInterval(() => {
      api.list().then(
        (list) => {
          setDevices(list);
          const added = list.find((d) => !known.current.has(d.id));
          if (added) {
            setJustPaired(added.name);
            setPairing(null);
          }
        },
        (e) => report(message(e)),
      );
    }, PAIRING_POLL_MS);
    return () => clearInterval(poll);
  }, [api, pairing, setDevices, report]);
  useEffect(() => {
    if (!pairing || fixedNowMs !== undefined) return;
    const tick = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(tick);
  }, [pairing, fixedNowMs]);
  const show = (p: Pairing, before: Device[]) => {
    known.current = new Set(before.map((d) => d.id));
    setNow(fixedNowMs ?? Date.now());
    setJustPaired(null);
    setPairing(p);
  };
  return { pairing, justPaired, now, show, close: () => setPairing(null) };
};

export interface DevicesView {
  status: MobileStatus | null;
  devices: Device[];
  pairing: Pairing | null;
  /** 符号で登録できた端末の名前。次の符号を出すまで残す。 */
  justPaired: string | null;
  error: string | null;
  busy: boolean;
  /** 表示の基準の時刻（ミリ秒）。符号を出している間は進む。 */
  nowMs: number;
  recheck: () => void;
  addDevice: () => void;
  closePairing: () => void;
  unpair: (id: string) => void;
}

/** 端末の登録の画面の状態。`fixedNowMs` があれば時計を進めない。 */
export const useDevices = (api: DevicesApi, refreshKey: number, fixedNowMs?: number): DevicesView => {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { status, devices, setDevices, load } = useLoad(api, refreshKey, setError);
  const p = usePairing(api, setDevices, setError, fixedNowMs);

  const guarded = (f: () => Promise<void>) => {
    setBusy(true);
    f().then(
      () => setError(null),
      (e) => setError(message(e)),
    ).finally(() => setBusy(false));
  };

  return {
    status,
    devices,
    pairing: p.pairing,
    justPaired: p.justPaired,
    error,
    busy,
    nowMs: p.now,
    recheck: () => void load(),
    addDevice: () => guarded(async () => p.show(await api.issueCode(), devices)),
    closePairing: p.close,
    unpair: (id) =>
      guarded(async () => {
        await api.revoke(id);
        setDevices(await api.list());
      }),
  };
};
