import { describe, expect, it } from "vitest"
import { reconcileAgentAccount } from "./agents"
import type { DeviceAgentAccounts } from "@/db/schema"

// S-3: an agent's login is re-checked against its device on every save, so
// the definition and its conversation sessions never name a profile the
// machine does not have (Big Brother ran on one login while its definition
// named another that only existed on a different device).
describe(`reconcileAgentAccount`, () => {
  const accounts: DeviceAgentAccounts = {
    claude: {
      signedIn: true,
      profiles: [
        { id: `work`, signedIn: true },
        { id: `home`, signedIn: true, active: true },
      ],
    },
  }

  it(`keeps a profile the device has`, () => {
    expect(reconcileAgentAccount(`work`, `claude`, accounts)).toBe(`work`)
    expect(reconcileAgentAccount(`work`, null, accounts)).toBe(`work`)
  })

  it(`falls back to the device's active profile for a missing one`, () => {
    expect(reconcileAgentAccount(`gone`, `claude`, accounts)).toBe(`home`)
  })

  it(`falls back to none when the device has no active profile`, () => {
    const inactive: DeviceAgentAccounts = {
      claude: { signedIn: true, profiles: [{ id: `work`, signedIn: true }] },
    }
    expect(reconcileAgentAccount(`gone`, `claude`, inactive)).toBeNull()
  })

  it(`keeps the name when the device reported nothing to check against`, () => {
    expect(reconcileAgentAccount(`gone`, `codex`, accounts)).toBe(`gone`)
    expect(reconcileAgentAccount(`gone`, `claude`, null)).toBe(`gone`)
    expect(reconcileAgentAccount(null, `claude`, accounts)).toBeNull()
  })
})
