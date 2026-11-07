import type { Op, OpResult, Transport } from "@mm/sdk";
import { API_ROOT, ApiError, type Api } from "./http.ts";

/**
 * 操作を `POST /m/api/ops` で PC に送る Transport。
 * PC は `base_version` を必須とするので、最後に読んだプロジェクトのバージョンを毎回付ける。
 */
export const createMobileTransport = (api: Api, baseVersion: () => number): Transport => ({
  send: async (op: Op): Promise<OpResult> => {
    try {
      return { ok: true, value: await api.post(`${API_ROOT}/ops`, { op, base_version: baseVersion() }) };
    } catch (e) {
      return { ok: false, error: e instanceof ApiError ? e.message : String(e) };
    }
  },
});
