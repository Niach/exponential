/**
 * Records the end-to-end Google Calendar OAuth flow as a webm video,
 * for submission to Google's OAuth verification reviewers.
 *
 * Usage:
 *   EXP_EMAIL=you@example.com EXP_PASSWORD=… bun run scripts/record-google-verification.ts
 *
 * Optional env vars:
 *   EXP_URL            — defaults to https://exponential.home.straehhuber.com
 *   EXP_PROJECT_SLUG   — first project found if unset
 *   RECORD_DIR         — defaults to ./recordings
 *
 * What it automates:
 *   1. Sign-in to Exponential (email/password)
 *   2. Navigate to /account/integrations
 *   3. Disconnect Google if already connected (so the recording starts clean)
 *   4. Click "Connect Google Calendar"
 *
 * What it pauses for (you handle interactively):
 *   5. The Google OAuth consent screen — pick the account, click Continue,
 *      grant calendar.events. Script auto-resumes when the browser lands
 *      back on /account/integrations.
 *   6. Issue creation with a due date — script asks you to do it in the
 *      browser, press Enter in the terminal when done.
 *   7. Opens calendar.google.com in a new tab — switch to the right week,
 *      point at the synced event, press Enter when done.
 *
 * The video is saved to ./recordings/<timestamp>.webm
 */

import { chromium } from "playwright"
import { mkdir, readdir, rename } from "node:fs/promises"
import { existsSync } from "node:fs"
import { createInterface } from "node:readline/promises"
import { stdin, stdout } from "node:process"
import { join, resolve } from "node:path"
import { homedir } from "node:os"

const APP_URL = (process.env.EXP_URL ?? `https://exponential.home.straehhuber.com`).replace(/\/$/, ``)
const EMAIL = process.env.EXP_EMAIL
const PASSWORD = process.env.EXP_PASSWORD
const RECORD_DIR = resolve(process.env.RECORD_DIR ?? `./recordings`)
const PROFILE_DIR = join(homedir(), `.cache`, `exponential-recording-profile`)

if (!EMAIL || !PASSWORD) {
  console.error(`error: set EXP_EMAIL and EXP_PASSWORD in env`)
  process.exit(1)
}

const rl = createInterface({ input: stdin, output: stdout })
const prompt = (msg: string) => rl.question(`\n>>> ${msg}\n    Press Enter when done… `)

await mkdir(RECORD_DIR, { recursive: true })
await mkdir(PROFILE_DIR, { recursive: true })

const isFirstRun = !existsSync(join(PROFILE_DIR, `Default`))
if (isFirstRun) {
  console.log(`
============================================================
First run — opening a fresh Chromium profile.
You'll need to sign in to Google manually during the OAuth
step. The profile is saved at:
  ${PROFILE_DIR}
so subsequent runs won't ask for Google credentials again.
============================================================
`)
}

console.log(`Launching Chromium…`)
const context = await chromium.launchPersistentContext(PROFILE_DIR, {
  headless: false,
  viewport: { width: 1280, height: 720 },
  recordVideo: {
    dir: RECORD_DIR,
    size: { width: 1280, height: 720 },
  },
  ignoreHTTPSErrors: true,
  args: [
    `--disable-blink-features=AutomationControlled`,
    `--window-size=1300,800`,
  ],
})

const page = context.pages()[0] ?? (await context.newPage())

try {
  // 1. Login to Exponential
  console.log(`[1/7] Signing in to Exponential at ${APP_URL}`)
  await page.goto(`${APP_URL}/auth/login`, { waitUntil: `domcontentloaded` })
  await page.locator(`#email`).fill(EMAIL)
  await page.locator(`#password`).fill(PASSWORD)
  await page.getByRole(`button`, { name: /sign in|log in/i }).first().click()
  await page.waitForURL((url) => !url.pathname.includes(`/auth/`), {
    timeout: 30_000,
  })
  console.log(`      Signed in as ${EMAIL}`)

  // 2. Navigate to integrations
  console.log(`[2/7] Opening /account/integrations`)
  await page.goto(`${APP_URL}/account/integrations`)
  await page.waitForLoadState(`networkidle`)

  // 3. Disconnect if already connected, so the recording shows a clean Connect flow
  const disconnect = page.getByRole(`button`, { name: /disconnect/i })
  if (await disconnect.isVisible({ timeout: 2000 }).catch(() => false)) {
    console.log(`[3/7] Already connected — disconnecting first for a clean recording`)
    await disconnect.click()
    await page.waitForTimeout(2500)
  } else {
    console.log(`[3/7] Not connected — good`)
  }

  await page.waitForTimeout(1500) // breathing room for the recording

  // 4. Click Connect
  console.log(`[4/7] Clicking "Connect Google Calendar"`)
  await page
    .getByRole(`button`, { name: /connect google calendar/i })
    .click()

  // 5. Wait for the user to complete Google OAuth manually
  console.log(`
[5/7] Waiting for Google OAuth flow…
       In the browser window:
         • pick your Google account
         • PAUSE briefly so the consent screen is readable
         • grant access to calendar.events
       The script auto-resumes when you return to /account/integrations.
`)
  await page.waitForURL(
    (url) => url.pathname === `/account/integrations` && !url.host.includes(`google.com`),
    { timeout: 5 * 60 * 1000 }
  )
  await page.waitForLoadState(`networkidle`)
  await page.waitForTimeout(3000) // hold on the "Connected" state for the camera

  // 6. Manual: create an issue with a due date
  console.log(`
[6/7] Now create an issue with a due date in the browser:
       • navigate to a project
       • new issue: title "Verification demo: Google Calendar sync"
       • set a due date a few days out
       • save the issue
`)
  await prompt(`Done creating the issue?`)

  // 7. Open Google Calendar so the synced event is visible
  console.log(`[7/7] Opening Google Calendar in a new tab`)
  const calPage = await context.newPage()
  await calPage.goto(`https://calendar.google.com/`, {
    waitUntil: `domcontentloaded`,
    timeout: 60_000,
  })
  await prompt(`Navigate to the week showing the new event, then press Enter to stop recording`)
} catch (error) {
  console.error(`\nrecording aborted:`, error)
} finally {
  console.log(`\nFinalizing recording…`)
  await context.close()
  rl.close()

  // Rename the latest webm to a timestamped, predictable filename
  const files = (await readdir(RECORD_DIR))
    .filter((f) => f.endsWith(`.webm`))
    .map((f) => ({ f, mtime: 0 }))
  if (files.length > 0) {
    const newest = files
      .map(({ f }) => ({
        f,
        mtime: Number(f.split(`-`).slice(-1)[0]?.replace(`.webm`, ``)) || Date.now(),
      }))
      .sort((a, b) => b.mtime - a.mtime)[0].f
    const stamp = new Date().toISOString().replace(/[:.]/g, `-`).slice(0, 19)
    const out = join(RECORD_DIR, `verification-${stamp}.webm`)
    await rename(join(RECORD_DIR, newest), out)
    console.log(`\n✓ Saved: ${out}`)
  } else {
    console.log(`\nno video produced (recording dir: ${RECORD_DIR})`)
  }
}
