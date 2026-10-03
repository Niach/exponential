import { createRequire } from 'node:module';
const require = createRequire('/Users/niach/Exponential/repos/Niach/exponential/node_modules/');
const sharp = require('sharp');
const out = process.cwd() + '/out/';
async function row(names, file, gap = 40) {
  const imgs = await Promise.all(names.map(n => sharp(out + n + '.png').toBuffer({ resolveWithObject: true })));
  const w = imgs.reduce((a, i) => a + i.info.width, 0) + gap * (imgs.length + 1);
  const h = Math.max(...imgs.map(i => i.info.height)) + gap * 2;
  let x = gap;
  const composite = imgs.map(i => { const c = { input: i.data, left: x, top: gap }; x += i.info.width + gap; return c; });
  await sharp({ create: { width: w, height: h, channels: 4, background: '#0a0a0d' } }).composite(composite).webp({ quality: 82 }).toFile(out + file);
  console.log(file);
}
await row(['make', 'nohost', 'home'], 'phones-1.webp');
await row(['thread', 'building', 'app'], 'phones-2.webp');
for (const n of ['tray', 'public', 'sheet']) { await sharp(out + n + '.png').webp({ quality: 82 }).toFile(out + n + '.webp'); console.log(n + '.webp'); }
