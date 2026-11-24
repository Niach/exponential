import type { Page } from "@playwright/test"
import { eq } from "drizzle-orm"
import { db } from "../../src/db/connection"
import { users } from "../../src/db/auth-schema"
import { agents, devices, teams } from "../../src/db/schema"
import { buildSecurityHeaders } from "../../src/lib/security-headers"
import { createTeamThroughOnboarding, registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

// #EXP-2102: a web call under the PRODUCTION Content-Security-Policy. The
// dev server sends no CSP, which hid that the call's AudioWorklet modules
// were blob: URLs the policy refuses (`script-src 'self'`): in a real browser
// every call ended at "This browser can't make calls" before
// `voice.startCall`. Here every document carries the headers the published
// image stamps (lib/security-headers.ts, cloud shape), and the call must get
// as far as asking the server: the microphone and BOTH worklets opened.
test.use({
  launchOptions: { args: [`--use-fake-device-for-media-stream`, `--use-fake-ui-for-media-stream`] },
  permissions: [`microphone`],
})

async function underProductionCsp(page: Page, baseURL: string) {
  const headers = buildSecurityHeaders({ CLOUD_INSTANCE: `true`, BETTER_AUTH_URL: baseURL })
  await page.route(`**/*`, async (route) => {
    if (route.request().resourceType() !== `document`) return route.fallback()
    const response = await route.fetch()
    await route.fulfill({ response, headers: { ...response.headers(), ...headers } })
  })
  return headers[`Content-Security-Policy`]
}

/** The member's own desktop, online and able to speak, and an agent on it. */
async function seedCallableAgent(app: AppFixture, teamSlug: string): Promise<string> {
  const [owner] = await db.select({ id: users.id }).from(users).where(eq(users.email, app.owner.email))
  const [team] = await db.select({ id: teams.id }).from(teams).where(eq(teams.slug, teamSlug))
  const deviceId = `pw-desk-${app.namespace}`
  await db.insert(devices).values({
    userId: owner!.id,
    deviceId,
    label: `Desk`,
    kind: `desktop`,
    sharedTeamIds: [],
    lastSeenAt: new Date(),
    doctor: { checkedAt: new Date().toISOString(), items: [{ key: `speech`, group: `voice`, state: `ok` }] } as never,
  })
  const [agent] = await db
    .insert(agents)
    .values({ teamId: team!.id, name: `Echo`, icon: null, color: null, instructions: ``, deviceId, ownerId: owner!.id, role: `agent` })
    .returning({ id: agents.id })
  return agent!.id
}

test(`a web call opens its audio and asks voice.startCall under the production CSP`, async ({ app, page, baseURL }) => {
  const csp = await underProductionCsp(page, baseURL!)
  // The policy the fix must live under: scripts from this origin only.
  const scriptSrc = csp.split(`; `).find((part) => part.startsWith(`script-src `))!
  expect(scriptSrc).not.toContain(`blob:`)
  expect(scriptSrc).not.toContain(`data:`)

  await registerUser(page, app.owner)
  const teamSlug = await createTeamThroughOnboarding(page, `${app.namespace} team`, app.boardName)
  const agentId = await seedCallableAgent(app, teamSlug)

  const refused: string[] = []
  page.on(`console`, (message) => {
    if (/Content Security Policy|worklet/i.test(message.text())) refused.push(message.text())
  })
  await page.goto(`/t/${teamSlug}/agents/${agentId}`)

  // The control: the policy is live on this page, and it does refuse a blob module.
  const blob = await page.evaluate(async () => {
    const context = new AudioContext()
    const url = URL.createObjectURL(new Blob([`registerProcessor('x', class extends AudioWorkletProcessor { process() { return true } })`], { type: `text/javascript` }))
    try {
      await context.audioWorklet.addModule(url)
      return `loaded`
    } catch (error) {
      return (error as Error).name
    } finally {
      void context.close()
    }
  })
  expect(blob).toBe(`AbortError`)
  refused.length = 0

  const call = page.getByTestId(`agent-chat-call`)
  await expect(call).toBeEnabled({ timeout: 30_000 })
  // A tab whose key holder is still starting waits for it first (up to 20 s).
  const asked = page.waitForRequest((request) => request.url().includes(`voice.startCall`), { timeout: 45_000 })
  await call.click()
  // `voice.startCall` is asked only after the microphone and both worklets opened.
  await asked
  await expect(page.getByText(`This browser can't make calls`)).toHaveCount(0)
  expect(refused).toEqual([])
})
