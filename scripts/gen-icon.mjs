// Generate a minimal valid Windows .ico embedding a 256x256 PNG, plus a PNG.
// Tauri's tauri-build requires `icons/icon.ico` to emit the Windows resource.
// Usage: node scripts/gen-icon.mjs <out-ico> <out-png>

import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

const SIZE = 256;
const outIco = process.argv[2] || "src-tauri/icons/icon.ico";
const outPng = process.argv[3] || "src-tauri/icons/icon.png";

// Simple brand-ish solid with a subtle diagonal gradient (RGBA).
function pixel(x, y) {
  const r = 11 + Math.floor(x / SIZE * 8);
  const g = 23 + Math.floor(y / SIZE * 10);
  const b = 42 + Math.floor((x + y) / (SIZE * 2) * 20);
  return [r, g, b, 255];
}

function crc32(buf) {
  let table = crc32.table;
  if (!table) {
    table = crc32.table = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      table[n] = c;
    }
  }
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = table[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

function makePng() {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(SIZE, 0);
  ihdr.writeUInt32BE(SIZE, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
  let o = 0;
  for (let y = 0; y < SIZE; y++) {
    raw[o++] = 0; // filter: none
    for (let x = 0; x < SIZE; x++) {
      const [r, g, b, a] = pixel(x, y);
      raw[o++] = r;
      raw[o++] = g;
      raw[o++] = b;
      raw[o++] = a;
    }
  }
  const idat = zlib.deflateSync(raw);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function wrapIco(png) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(1, 4);
  const entry = Buffer.alloc(16);
  entry[0] = 0; // width 256
  entry[1] = 0; // height 256
  entry[2] = 0; // color count
  entry[3] = 0;
  entry.writeUInt16LE(1, 4); // planes
  entry.writeUInt16LE(32, 6); // bit count
  entry.writeUInt32LE(png.length, 8);
  entry.writeUInt32LE(22, 12); // offset
  return Buffer.concat([header, entry, png]);
}

const png = makePng();
fs.mkdirSync(path.dirname(outIco), { recursive: true });
fs.writeFileSync(outPng, png);
fs.writeFileSync(outIco, wrapIco(png));
console.log(`wrote ${outPng} (${png.length}B) and ${outIco}`);
