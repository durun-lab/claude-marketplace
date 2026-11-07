import { useEffect, useState } from "preact/hooks";
import type { ImproveStatus, SettingsPatch } from "./api.ts";
import { headline } from "./model.ts";

interface SettingsBarProps {
  status: ImproveStatus;
  busy: boolean;
  nowMs: number;
  onSettings: (patch: SettingsPatch) => void;
  onStop: () => void;
}

/** 数の入力欄。確定（change）したときだけ送り、空と 0 以下の値は送らない（予算は外せない）。 */
const NumberField = ({ label, value, step, onCommit }: { label: string; value: number | null; step: string; onCommit: (v: number) => void }) => {
  const [text, setText] = useState(value === null ? "" : String(value));
  useEffect(() => setText(value === null ? "" : String(value)), [value]);
  return (
    <label class="mm-feedback-field">
      <span>{label}</span>
      <input
        type="number"
        min="0"
        step={step}
        value={text}
        onInput={(e) => setText(e.currentTarget.value)}
        onChange={(e) => {
          const v = Number(e.currentTarget.value);
          if (e.currentTarget.value !== "" && Number.isFinite(v) && v > 0) onCommit(v);
        }}
      />
    </label>
  );
};

/** 見出し。自動の改善の ON / OFF、回の予算と時間の上限、すぐ止めるボタン、いまの状態の 1 行。 */
export const SettingsBar = ({ status, busy, nowMs, onSettings, onStop }: SettingsBarProps) => {
  const s = status.settings;
  const stoppable = status.running || status.run !== null || s.enabled;
  return (
    <div class="mm-feedback-bar">
      <div class="mm-feedback-bar-row">
        <label class="mm-feedback-switch">
          <input type="checkbox" role="switch" checked={s.enabled} disabled={busy} onChange={(e) => onSettings({ enabled: e.currentTarget.checked })} />
          <span>Auto-improve</span>
        </label>
        <button type="button" class="mm-feedback-button mm-feedback-stop" disabled={busy || !stoppable} onClick={onStop}>
          Stop now
        </button>
      </div>
      <div class="mm-feedback-bar-row">
        <NumberField label="Budget $" value={s.budgetUsd} step="0.5" onCommit={(v) => onSettings({ budgetUsd: v })} />
        <NumberField label="Minutes" value={s.wallMinutes} step="5" onCommit={(v) => onSettings({ wallMinutes: v })} />
      </div>
      <p class="mm-feedback-headline" role="status" data-running={status.running}>
        {headline(status, nowMs)}
      </p>
    </div>
  );
};
