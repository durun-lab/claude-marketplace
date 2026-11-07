// アイコンの PNG を依存なしで作る。背景に再生記号を置く単純な図形で、iOS のホーム画面と Android の両方が読める。
import { Buffer } from "node:buffer";
import { deflateSync } from "node:zlib";

const BG = [16, 16, 18];
const FG = [77, 148, 255];
const SAMPLES = 3;

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const body = Buffer.concat([Buffer.from(type), data]);
  const out = Buffer.alloc(body.length + 8);
  out.writeUInt32BE(data.length, 0);
  body.copy(out, 4);
  out.writeUInt32BE(crc32(body), body.length + 4);
  return out;
};

/** 点が右向きの三角（再生記号）の中にあるか。size は一辺、scale は記号の大きさの割合。 */
const inTriangle = (x, y, size, scale) => {
  const h = size * scale;
  const left = size / 2 - h * 0.35;
  const right = size / 2 + h * 0.5;
  if (x < left || x > right) return false;
  const half = ((right - x) / (right - left)) * (h / 2);
  return Math.abs(y - size / 2) <= half;
};

const coverage = (px, py, size, scale) => {
  let hit = 0;
  for (let i = 0; i < SAMPLES; i++)
    for (let j = 0; j < SAMPLES; j++) hit += inTriangle(px + (i + 0.5) / SAMPLES, py + (j + 0.5) / SAMPLES, size, scale) ? 1 : 0;
  return hit / (SAMPLES * SAMPLES);
};

/** scale は三角の大きさ。maskable は端が切られるので小さくして安全域に収める。 */
export const makeIcon = (size, scale) => {
  const raw = Buffer.alloc((size * 3 + 1) * size);
  for (let y = 0; y < size; y++) {
    const row = y * (size * 3 + 1);
    for (let x = 0; x < size; x++) {
      const a = coverage(x, y, size, scale);
      for (let c = 0; c < 3; c++) raw[row + 1 + x * 3 + c] = Math.round(BG[c] + (FG[c] - BG[c]) * a);
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
};
