// Generates the PWA icon set (solid rounded square + checkmark) as PNGs
// without any image-library dependency, plus an SVG favicon.
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const outDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'public', 'icons');
mkdirSync(outDir, { recursive: true });

/* ---------- minimal PNG encoder ---------- */

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c >>> 0;
}

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, payload) {
  const body = Buffer.concat([Buffer.from(type, 'ascii'), payload]);
  const out = Buffer.alloc(body.length + 8);
  out.writeUInt32BE(payload.length, 0);
  body.copy(out, 4);
  out.writeUInt32BE(crc32(body), body.length + 4);
  return out;
}

function encodePng(size, rgba) {
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0))
  ]);
}

/* ---------- drawing ---------- */

const BG_TOP = [0xe8, 0x6a, 0x4c];
const BG_BOTTOM = [0xd6, 0x40, 0x2a];

function clamp01(v) {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

// Distance from point (px,py) to segment (ax,ay)-(bx,by).
function segDist(px, py, ax, ay, bx, by) {
  const vx = bx - ax;
  const vy = by - ay;
  const t = clamp01(((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy));
  const dx = px - (ax + vx * t);
  const dy = py - (ay + vy * t);
  return Math.hypot(dx, dy);
}

function drawIcon(size, { maskable = false } = {}) {
  const rgba = Buffer.alloc(size * size * 4);
  const half = size / 2;
  const boxHalf = maskable ? half : half - size * 0.02;
  const radius = maskable ? 0 : size * 0.225;
  // The check shrinks a bit on maskable icons to stay inside the safe zone.
  const scale = maskable ? 0.72 : 1;
  const cp = (v) => half + (v - 0.5) * size * scale;
  const p1 = [cp(0.29), cp(0.535)];
  const p2 = [cp(0.445), cp(0.69)];
  const p3 = [cp(0.73), cp(0.35)];
  const strokeHalf = size * 0.062 * scale;

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const px = x + 0.5;
      const py = y + 0.5;
      // Rounded-rect signed distance (negative = inside).
      const qx = Math.abs(px - half) - (boxHalf - radius);
      const qy = Math.abs(py - half) - (boxHalf - radius);
      const boxDist =
        Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - radius;
      const bgAlpha = clamp01(0.5 - boxDist);
      if (bgAlpha === 0) continue;

      const t = y / size;
      let r = BG_TOP[0] + (BG_BOTTOM[0] - BG_TOP[0]) * t;
      let g = BG_TOP[1] + (BG_BOTTOM[1] - BG_TOP[1]) * t;
      let b = BG_TOP[2] + (BG_BOTTOM[2] - BG_TOP[2]) * t;

      const check = Math.min(
        segDist(px, py, p1[0], p1[1], p2[0], p2[1]),
        segDist(px, py, p2[0], p2[1], p3[0], p3[1])
      );
      const white = clamp01(strokeHalf - check + 0.5);
      r += (255 - r) * white;
      g += (255 - g) * white;
      b += (255 - b) * white;

      const i = (y * size + x) * 4;
      rgba[i] = Math.round(r);
      rgba[i + 1] = Math.round(g);
      rgba[i + 2] = Math.round(b);
      rgba[i + 3] = Math.round(bgAlpha * 255);
    }
  }
  return encodePng(size, rgba);
}

writeFileSync(join(outDir, 'pwa-192.png'), drawIcon(192));
writeFileSync(join(outDir, 'pwa-512.png'), drawIcon(512));
writeFileSync(join(outDir, 'maskable-512.png'), drawIcon(512, { maskable: true }));
writeFileSync(join(outDir, 'apple-touch-icon.png'), drawIcon(180, { maskable: true }));

const favicon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#e86a4c"/>
      <stop offset="1" stop-color="#d6402a"/>
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="96" height="96" rx="22" fill="url(#g)"/>
  <path d="M29 53.5 L44.5 69 L73 35" fill="none" stroke="#fff" stroke-width="12.5" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
`;
writeFileSync(join(outDir, 'favicon.svg'), favicon);

console.log(`icons written to ${outDir}`);
