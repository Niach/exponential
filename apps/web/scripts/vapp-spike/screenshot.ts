// VAPP-4 spike: screenshot the real-CSS kitchen sink wide + phone.
import { launch, openSignedIn, openSurface } from "./common"

const browser = await launch()
try {
  const wide = await openSignedIn(browser, { width: 1440, height: 960 })
  await openSurface(wide)
  await wide.screenshot({ path: `/tmp/vapp4-web-wide.png`, fullPage: true })
  const narrow = await openSignedIn(browser, { width: 390, height: 844, mobile: true })
  await openSurface(narrow)
  await narrow.screenshot({ path: `/tmp/vapp4-web-narrow.png`, fullPage: true })
  console.log(`wrote /tmp/vapp4-web-wide.png /tmp/vapp4-web-narrow.png`)
} finally {
  await browser.close()
}
