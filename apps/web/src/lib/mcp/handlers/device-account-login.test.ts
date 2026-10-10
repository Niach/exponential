import { describe, expect, it, vi } from "vitest"
import {
  deviceAccountLogin,
  type DeviceAccountLoginDeps,
  type DeviceCommandRow,
} from "./device-account-login"

// EXP-1199: the MCP half of a remote sign-in — the same command as every
// client's Add account / Sign in, the bounded wait on the device's early answer,
// and the code hand-off.

const URL_RESULT = JSON.stringify({
  agent: `claude`,
  phase: `url`,
  url: `https://claude.com/cai/oauth/authorize?code=true`,
  profileId: `a1b2c3d4`,
})

function deps(answers: Array<Partial<DeviceCommandRow>>) {
  let tick = 0
  const created: unknown[] = []
  const built: DeviceAccountLoginDeps = {
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
  it(`adds an account with no profile (it lands by email) and returns the URL`, async () => {
    const { built, created } = deps([{}, { status: `done`, result: URL_RESULT }])
    const result = await deviceAccountLogin({ deviceId: `dev`, agent: `claude` }, built)
    expect(created[0]).toEqual({
      deviceId: `dev`,
      kind: `agent_login`,
      agent: `claude`,
      switch: false,
    })
    expect(result).toMatchObject({
      status: `url`,
      commandId: `cmd-1`,
      url: `https://claude.com/cai/oauth/authorize?code=true`,
      code: null,
      profileId: `a1b2c3d4`,
    })
  })

  it(`re-signs profileId`, async () => {
    const resign = deps([{ status: `done`, result: URL_RESULT }])
    await deviceAccountLogin({ deviceId: `dev`, agent: `claude`, profileId: `p1` }, resign.built)
    expect(resign.created[0]).toMatchObject({ profileId: `p1` })
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
  })

  it(`surfaces the server's refusal for a device that is not the caller's`, async () => {
    const { built } = deps([{}])
    built.createCommand = vi.fn(async () => {
      throw new Error(`Device not found`)
    })
    await expect(deviceAccountLogin({ deviceId: `dev`, agent: `claude` }, built)).rejects.toThrow(
      /Device not found/
    )
  })
})
