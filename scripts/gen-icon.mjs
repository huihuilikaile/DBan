// 生成 DBan 应用图标源图（1024x1024 PNG：白色圆角方块 + 黑色对勾）
// 纯 Node 实现 PNG 编码（zlib + CRC32），无第三方依赖
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";

const S = 1024;

// ---------- CRC32 ----------
const CRC_TABLE = new Int32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  CRC_TABLE[n] = c;
}
function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

// ---------- 形状 SDF ----------
function sdRoundBox(px, py, cx, cy, hx, hy, r) {
  const qx = Math.abs(px - cx) - (hx - r);
  const qy = Math.abs(py - cy) - (hy - r);
  const ax = Math.max(qx, 0), ay = Math.max(qy, 0);
  return Math.min(Math.max(qx, qy), 0) + Math.hypot(ax, ay) - r;
}
function sdSegment(px, py, ax, ay, bx, by) {
  const pax = px - ax, pay = py - ay, bax = bx - ax, bay = by - ay;
  const h = Math.max(0, Math.min(1, (pax * bax + pay * bay) / (bax * bax + bay * bay)));
  return Math.hypot(pax - bax * h, pay - bay * h);
}

// ---------- 逐像素绘制 ----------
const raw = Buffer.alloc(S * (S * 4 + 1));
for (let y = 0; y < S; y++) {
  raw[y * (S * 4 + 1)] = 0; // filter: none
  for (let x = 0; x < S; x++) {
    const px = x + 0.5, py = y + 0.5;
    const dBox = sdRoundBox(px, py, S / 2, S / 2, S / 2 - 4, S / 2 - 4, 190);
    const alphaBox = Math.max(0, Math.min(1, 1 - dBox / 2)); // 2px 抗锯齿

    // 对勾两段线，粗 74px，圆头由距离场近似
    const d1 = sdSegment(px, py, S * 0.30, S * 0.53, S * 0.445, S * 0.675);
    const d2 = sdSegment(px, py, S * 0.445, S * 0.675, S * 0.735, S * 0.365);
    const dCheck = Math.min(d1, d2) - 40;
    const aCheck = Math.max(0, Math.min(1, 1 - dCheck / 2));

    const off = y * (S * 4 + 1) + 1 + x * 4;
    // 白底 + 黑勾
    const r = Math.round(255 * (1 - aCheck) + 23 * aCheck);
    const g = Math.round(255 * (1 - aCheck) + 23 * aCheck);
    const b = Math.round(255 * (1 - aCheck) + 26 * aCheck);
    raw[off] = r;
    raw[off + 1] = g;
    raw[off + 2] = b;
    raw[off + 3] = Math.round(255 * alphaBox);
  }
}

// ---------- 组装 PNG ----------
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8;  // bit depth
ihdr[9] = 6;  // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

mkdirSync("scripts", { recursive: true });
writeFileSync("scripts/app-icon.png", png);
console.log("written scripts/app-icon.png", png.length, "bytes");
