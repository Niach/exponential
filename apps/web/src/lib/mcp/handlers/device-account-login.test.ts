import { describe, expect, it, vi } from "vitest"
import {
  deviceAccountLogin,
  type DeviceAccountLoginDeps,
  type DeviceCommandRow,
} from "./device-account-login"

// EXP-1199: the MCP half of a remote sign-in — the target rule shared with
// every client's Add account, the bounded wait on the device's early answer,
// and the code hand-off.

const URL_RESULT = JSON.stringify({
  agent: `claude`,
  phase: `url`,
  url: `https://claude.com/cai/oauth/authorize?code=true`,
  profileId: `a1b2c3d4`,
})

function deps(
  answers: Array<Partial<DeviceCommandRow>>,
  accounts: unknown = { claude: { signedIn: true, profiles: [{ id: `system`, signedIn: true }] } }
) {
  let tick = 0
  const created: unknown[] = []
  const built: DeviceAccountLoginDeps = {
    loadAccounts: vi.fn(async () => accounts as never),
    createCommand: vi.fn(async (input) => {
      created.push(input)
      return { id: `cmd-1` }
    }),
    getCommand: vi.fn(async (id) => {
      const answer = answers[Math.min(tick, answers.length - 1)]
      tick += 1
      return { id, kind: `agent_login`, status: `pending`, result: null, ...answer }
    }),
    sleep: vi.fn(async () => {}),
  }
  return { built, created }
}

describe(`deviceAccountLogin (EXP-1199)`, () => {
  it(`adds a NEW profile beside a signed-in default login and returns the URL`, async () => {
    const { built, created } = deps([{}, { status: `done`, result: URL_RESULT }])
    const result = await deviceAccountLogin({ deviceId: `dev`, agent: `claude` }, built)
    expect(created[0]).toEqual({
      deviceId: `dev`,
      kind: `agent_login`,
      agent: `claude`,
      switch: false,
      newProfileLabel: `Claude Code account 2`,
    })
    expect(result).toMatchObject({
      status: `url`,
      commandId: `cmd-1`,
      url: `https://claude.com/cai/oauth/authorize?code=true`,
      code: null,
      profileId: `a1b2c3d4`,
    })
  })

  it(`signs the default login in while it is still signed out`, async () => {
    const { built, created } = deps([{ status: `done`, result: URL_RESULT }], {
      codex: { signedIn: false },
    })
    await deviceAccountLogin({ deviceId: `dev`, agent: `codex` }, built)
    expect(created[0]).toMatchObject({ profileId: `system` })
  })

  it(`names the new login after name, or re-signs profileId`, async () => {
    const named = deps([{ status: `done`, result: URL_RESULT }])
    await deviceAccountLogin({ deviceId: `dev`, agent: `claude`, name: ` Work ` }, named.built)
    expect(named.created[0]).toMatchObject({ newProfileLabel: `Work` })
    const resign = deps([{ status: `done`, result: URL_RESULT }])
    await deviceAccountLogin({ deviceId: `dev`, agent: `claude`, profileId: `p1` }, resign.built)
    expect(resign.created[0]).toMatchObject({ profileId: `p1` })
    await expect(
      deviceAccountLogin({ deviceId: `dev`, agent: `claude`, name: `a`, profileId: `p1` }, resign.built)
    ).rejects.toThrow(/not both/)
  })

  it(`hands back the commandId when the machine has not answered in time`, async () => {
    const { built } = deps([{}])
    const result = await deviceAccountLogin({ deviceId: `dev`, agent: `claude` }, built)
    expect(result).toMatchObject({ status: `pending`, commandId: `cmd-1` })
    // A later call checks on it.
    const later = deps([{ status: `done`, result: URL_RESULT }])
    expect(
      await deviceAccountLogin({ deviceId: `dev`, agent: `claude`, commandId: `cmd-1` }, later.built)
    ).toMatchObject({ status: `url` })
    expect(later.built.createCommand).not.toHaveBeenCalled()
  })

  it(`carries codex's device code and a device failure`, async () => {
    const codex = deps([
      {
        status: `done`,
        result: JSON.stringify({ agent: `codex`, phase: `url`, url: `https://auth.openai.com/codex/device`, code: `WDJB-MJHT` }),
      },
    ])
    expect(await deviceAccountLogin({ deviceId: `dev`, agent: `codex` }, codex.built)).toMatchObject({
      status: `url`,
      code: `WDJB-MJHT`,
    })
    const failed = deps([{ status: `failed`, result: `codex is not installed` }])
    expect(await deviceAccountLogin({ deviceId: `dev`, agent: `codex` }, failed.built)).toEqual({
      status: `failed`,
      commandId: `cmd-1`,
      message: `codex is not installed`,
    })
  })

  it(`queues the browser code as agent_login_code`, async () => {
    const { built, created } = deps([{ kind: `agent_login_code`, status: `done`, result: `ok` }])
    const result = await deviceAccountLogin(
      { deviceId: `dev`, agent: `claude`, code: `abc#123` },
      built
    )
    expect(created[0]).toEqual({ deviceId: `dev`, kind: `agent_login_code`, agent: `claude`, code: `abc#123` })
    expect(result).toMatchObject({ status: `signing_in` })
    expect(built.loadAccounts).not.toHaveBeenCalled()
  })

  it(`refuses a device that is not the caller's`, async () => {
    const { built } = deps([{}], undefined)
    built.loadAccounts = vi.fn(async () => undefined)
    await expect(deviceAccountLogin({ deviceId: `dev`, agent: `claude` }, built)).rejects.toThrow(
      /Device not found/
    )
  })
})
