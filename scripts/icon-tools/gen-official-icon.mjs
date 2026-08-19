// Rasterize DeepSeek Harness' official favicon.svg into app icons using sharp.
// Outputs:
//   src-tauri/icons/icon.png        (512px reference)
//   src-tauri/icons/icon.ico        (multi-size, PNG-embedded)
// Usage: node scripts/icon-tools/gen-official-icon.mjs
import fs from "node:fs";
import path from "node:path";
import sharp from "sharp";

const ROOT = path.resolve(process.cwd());
const SRC_SVG = path.join(
  ROOT,
  "dsh-runtime/node_modules/@deepseek-ai/dsh-web-frontend/dist/favicon.svg",
);
const OUT_DIR = path.join(ROOT, "src-tauri/icons");
const SIZES = [16, 24, 32, 48, 64, 128, 256, 512];

const svg = fs.readFileSync(SRC_SVG);
// Render once at ~512px native (viewBox is 50x50), then downscale per size.
const base = await sharp(svg, { density: 738 }).resize(512, 512).png().toBuffer();
const pngs = {};
for (const s of SIZES) {
  pngs[s] = await sharp(base).resize(s, s).png().toBuffer();
}

fs.mkdirSync(OUT_DIR, { recursive: true });
fs.writeFileSync(path.join(OUT_DIR, "icon.png"), pngs[512]);
console.log(`wrote ${path.join(OUT_DIR, "icon.png")} (${pngs[512].length}B)`);

// Compose an ICO with PNG-compressed entries (Windows Vista+ supports these).
const useSizes = [16, 32, 48, 64, 128, 256];
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(useSizes.length, 4);

const entries = [];
const blocks = [];
let offset = 6 + useSizes.length * 16;
for (const s of useSizes) {
  const data = pngs[s];
  const e = Buffer.alloc(16);
  e[0] = s >= 256 ? 0 : s;
  e[1] = s >= 256 ? 0 : s;
  e[2] = 0;
  e[3] = 0;
  e.writeUInt16LE(1, 4); // planes
  e.writeUInt16LE(32, 6); // bit count
  e.writeUInt32LE(data.length, 8);
  e.writeUInt32LE(offset, 12);
  entries.push(e);
  blocks.push(data);
  offset += data.length;
}

fs.writeFileSync(
  path.join(OUT_DIR, "icon.ico"),
  Buffer.concat([header, ...entries, ...blocks]),
);
console.log(`wrote ${path.join(OUT_DIR, "icon.ico")}`);
