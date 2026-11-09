import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { inflateSync } from 'node:zlib';

// 只解码仓库图标的非交错 RGBA8 PNG，不启动 Electron 或打开应用数据。
function rgba(png) {
  assert.equal(png.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  const width = png.readUInt32BE(16),
    height = png.readUInt32BE(20);
  assert.equal(png[24], 8);
  assert.equal(png[25], 6);
  assert.equal(png[28], 0);
  assert(width <= 1024 && height <= 1024);
  const chunks = [];
  for (let offset = 8; offset < png.length;) {
    const size = png.readUInt32BE(offset);
    assert(offset + size + 12 <= png.length);
    if (png.toString('ascii', offset + 4, offset + 8) === 'IDAT')
      chunks.push(png.subarray(offset + 8, offset + 8 + size));
    offset += size + 12;
  }
  const stride = width * 4;
  const raw = inflateSync(Buffer.concat(chunks), { maxOutputLength: (stride + 1) * height });
  assert.equal(raw.length, (stride + 1) * height);
  const pixels = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    assert(filter <= 4);
    for (let x = 0; x < stride; x++) {
      const i = y * stride + x;
      const left = x >= 4 ? pixels[i - 4] : 0,
        up = y ? pixels[i - stride] : 0;
      const corner = y && x >= 4 ? pixels[i - stride - 4] : 0;
      const p = left + up - corner;
      const a = Math.abs(p - left),
        b = Math.abs(p - up),
        c = Math.abs(p - corner);
      const predictor =
        filter === 0
          ? 0
          : filter === 1
            ? left
            : filter === 2
              ? up
              : filter === 3
                ? Math.floor((left + up) / 2)
                : a <= b && a <= c
                  ? left
                  : b <= c
                    ? up
                    : corner;
      pixels[i] = (raw[y * (stride + 1) + x + 1] + predictor) & 255;
    }
  }
  return { width, height, pixels };
}

test('Dock artwork and ICNS contain identical RGBA pixels with transparent corners', async () => {
  const source = rgba(await readFile('resources/icons/app-icon-source.png'));
  const icns = await readFile('resources/icons/icon.icns');
  assert.equal(icns.toString('ascii', 0, 4), 'icns');
  let image;
  for (let offset = 8; offset < icns.length;) {
    const size = icns.readUInt32BE(offset + 4);
    assert(size >= 8 && offset + size <= icns.length);
    if (icns.toString('ascii', offset, offset + 4) === 'ic10')
      image = rgba(icns.subarray(offset + 8, offset + size));
    offset += size;
  }
  assert.equal(source.width, 1024);
  assert.equal(source.height, 1024);
  assert.deepEqual(image, source);
  const alpha = (x, y) => source.pixels[(y * source.width + x) * 4 + 3];
  assert.deepEqual([alpha(0, 0), alpha(1023, 0), alpha(0, 1023), alpha(1023, 1023)], [0, 0, 0, 0]);
  assert.equal(alpha(512, 128), 255);
});
