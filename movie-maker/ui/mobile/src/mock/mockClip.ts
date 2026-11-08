import type { Proposal } from "../proposals/api.ts";
import { hasClip } from "../proposals/view.ts";

const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const MASK_64 = 0xffffffffffffffffn;
const KEY_DIGITS = 16;
const HEX = 16;

/** 64 ビットの FNV-1a を 16 桁の 16 進にする。PC の短い動画のキーと同じ形。 */
const fingerprint = (text: string): string => {
  let h = FNV_OFFSET;
  for (const c of new TextEncoder().encode(text)) h = ((h ^ BigInt(c)) * FNV_PRIME) & MASK_64;
  return h.toString(HEX).padStart(KEY_DIGITS, "0");
};

/**
 * 提案の応答の `clip_url`。キーはプロジェクトのバージョンと範囲と操作から作り、編集のたびに変わる（PC は範囲の中の編集だけで変わる）。
 * 保留中で範囲のある提案だけ URL を返す。
 */
export const mockClipUrl = (p: Proposal, version: number, query: URLSearchParams): string | null => {
  if (!hasClip(p)) return null;
  const comp = query.get("comp") ?? "c1";
  const ops = query.get("ops");
  const key = fingerprint(`${version}|${p.id}|${comp}|${ops ?? "all"}`);
  return `/m/api/proposals/${p.id}/clip/${key}.mp4?comp=${encodeURIComponent(comp)}${ops ? `&ops=${ops}` : ""}`;
};
