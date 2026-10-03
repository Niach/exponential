import { createRequire } from 'node:module';
const require = createRequire('/Users/niach/Exponential/repos/Niach/exponential/node_modules/');
const { chromium } = require('playwright');
const [,, ...files] = process.argv;
const browser = await chromium.launch();
for (const f of files) {
  const [name, w, h, scale] = f.split(':');
  const page = await browser.newPage({ viewport: { width: +w, height: +h }, deviceScaleFactor: +(scale || 2) });
  await page.goto(`file://${process.cwd()}/${name}.html`);
  await page.waitForTimeout(900);
  await page.screenshot({ path: `${process.cwd()}/out/${name}.png`, fullPage: name === 'sheet' });
  console.log('rendered', name);
}
await browser.close();
