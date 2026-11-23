import { afterEach, describe, expect, it } from "vitest"
import {
  CLIENT_VERSION_HEADER,
  DEVICE_SEALING_FLOOR,
  checkClientVersion,
  clientFloors,
  parseClientVersionHeader,
  parseVersionTuple,
  versionPayload,
} from "./client-version"
import { ASSISTANT_CLIENT_FLOOR } from "@exp/domain-contract/agent-settings"
import sealedFixture from "@exp/domain-contract/fixtures/sealed-content.json"

// The min-version gate must fail OPEN on every ambiguity: clients shipped
// before the header existed never send it, and a bad env var must never
// brick the install base. Only a well-formed header + well-formed min env +
// a genuinely lower version tuple may produce 426.

const ENV_KEYS = [
  `CLIENT_MIN_VERSION_ANDROID`,
  `CLIENT_MIN_VERSION_IOS`,
  `CLIENT_MIN_VERSION_DESKTOP`,
  `CLIENT_MIN_VERSION_CLI`,
  `CLIENT_LATEST_VERSION_ANDROID`,
  `CLIENT_LATEST_VERSION_IOS`,
  `CLIENT_LATEST_VERSION_DESKTOP`,
  `CLIENT_LATEST_VERSION_CLI`,
]

afterEach(() => {
  for (const key of ENV_KEYS) delete process.env[key]
})

function req(header?: string): Request {
  return new Request(`http://localhost/api/trpc/users.me`, {
    headers: header ? { [CLIENT_VERSION_HEADER]: header } : {},
  })
}

describe(`parseVersionTuple`, () => {
  it(`parses plain semver`, () => {
    expect(parseVersionTuple(`0.13.2`)).toEqual([0, 13, 2])
  })

  it(`strips pre-release and build suffixes`, () => {
    expect(parseVersionTuple(`0.13.2-staging`)).toEqual([0, 13, 2])
    expect(parseVersionTuple(`1.0.0+abc123`)).toEqual([1, 0, 0])
  })

  it(`pads missing segments with zeros`, () => {
    expect(parseVersionTuple(`2.0`)).toEqual([2, 0, 0])
    expect(parseVersionTuple(`3`)).toEqual([3, 0, 0])
  })

  it(`rejects garbage`, () => {
    expect(parseVersionTuple(`garbage`)).toBeNull()
    expect(parseVersionTuple(``)).toBeNull()
    expect(parseVersionTuple(`v1.2.3`)).toBeNull()
  })
})

describe(`parseClientVersionHeader`, () => {
  it(`parses platform/version`, () => {
    expect(parseClientVersionHeader(`android/0.13.2`)).toEqual({
      platform: `android`,
      version: `0.13.2`,
    })
    expect(parseClientVersionHeader(`Desktop/0.8.3`)).toEqual({
      platform: `desktop`,
      version: `0.8.3`,
    })
  })

  it(`rejects missing header, unknown platform, and malformed values`, () => {
    expect(parseClientVersionHeader(null)).toBeNull()
    expect(parseClientVersionHeader(`web/1.0.0`)).toBeNull()
    expect(parseClientVersionHeader(`android`)).toBeNull()
    expect(parseClientVersionHeader(`android/`)).toBeNull()
    expect(parseClientVersionHeader(`/1.0.0`)).toBeNull()
  })
})

describe(`checkClientVersion`, () => {
  it(`blocks a client below the configured minimum with a 426 JSON body`, async () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.15.0`
    process.env.CLIENT_LATEST_VERSION_ANDROID = `0.15.1`
    const res = checkClientVersion(req(`android/0.13.2`))
    expect(res?.status).toBe(426)
    expect(await res?.json()).toEqual({
      error: `client_upgrade_required`,
      platform: `android`,
      min: `0.15.0`,
      latest: `0.15.1`,
      message: expect.stringContaining(`update`),
    })
  })

  it(`allows a version equal to the minimum`, () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.15.0`
    expect(checkClientVersion(req(`android/0.15.0`))).toBeNull()
  })

  it(`allows a version above the minimum`, () => {
    process.env.CLIENT_MIN_VERSION_DESKTOP = `0.15.0`
    expect(checkClientVersion(req(`desktop/0.15.1`))).toBeNull()
  })

  it(`compares numerically, not lexicographically`, () => {
    process.env.CLIENT_MIN_VERSION_IOS = `1.9.0`
    expect(checkClientVersion(req(`ios/1.10.0`))).toBe(null)
  })

  it(`gates the cli platform (EXP-403)`, async () => {
    process.env.CLIENT_MIN_VERSION_CLI = `0.15.0`
    expect(checkClientVersion(req(`cli/0.15.0`))).toBeNull()
    const res = checkClientVersion(req(`cli/0.14.200`))
    expect(res?.status).toBe(426)
    expect(await res?.json()).toMatchObject({ platform: `cli`, min: `0.15.0` })
  })

  it(`the sealing floor (EDE-97) is the fixture's, below the Assistant floor that now applies on desktop and CLI`, () => {
    expect(DEVICE_SEALING_FLOOR).toBe(sealedFixture.clearTree.deviceMinVersion)
    expect(clientFloors(`desktop`)).toEqual([DEVICE_SEALING_FLOOR, ASSISTANT_CLIENT_FLOOR.desktop])
    expect(clientFloors(`cli`)).toEqual([DEVICE_SEALING_FLOOR, ASSISTANT_CLIENT_FLOOR.cli])
    expect(clientFloors(`android`)).toEqual([ASSISTANT_CLIENT_FLOOR.android])
    expect(clientFloors(`ios`)).toEqual([ASSISTANT_CLIENT_FLOOR.ios])
    // An unset minimum still fails open.
    expect(checkClientVersion(req(`desktop/0.1.0`))).toBeNull()
    expect(checkClientVersion(req(`cli/0.1.0`))).toBeNull()
  })

  it(`raises a configured minimum below the Assistant floor on every platform, with no switch (#EXP-1984, #EXP-1989)`, async () => {
    // The first release tags holding #EXP-1987 + #EXP-2020 and #EXP-1988.
    expect(ASSISTANT_CLIENT_FLOOR).toEqual({ desktop: `0.14.171`, cli: `0.14.171`, android: `0.14.152`, ios: `0.14.140` })
    process.env.CLIENT_MIN_VERSION_DESKTOP = `0.14.137`
    process.env.CLIENT_MIN_VERSION_CLI = `0.14.88`
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.14.0`
    process.env.CLIENT_MIN_VERSION_IOS = `0.14.116`
    const res = checkClientVersion(req(`android/0.14.151`))
    expect(res?.status).toBe(426)
    expect(await res?.json()).toMatchObject({ platform: `android`, min: `0.14.152` })
    expect(checkClientVersion(req(`android/0.14.152`))).toBeNull()
    expect(checkClientVersion(req(`ios/0.14.139`))?.status).toBe(426)
    expect(checkClientVersion(req(`ios/0.14.140`))).toBeNull()
    expect(checkClientVersion(req(`desktop/0.14.170`))?.status).toBe(426)
    expect(checkClientVersion(req(`desktop/0.14.171`))).toBeNull()
    const cli = checkClientVersion(req(`cli/0.14.170`))
    expect(cli?.status).toBe(426)
    expect(await cli?.json()).toMatchObject({ platform: `cli`, min: `0.14.171` })
    expect(checkClientVersion(req(`cli/0.14.171`))).toBeNull()
    // A minimum above the floor stays; unset gates nothing.
    process.env.CLIENT_MIN_VERSION_IOS = `0.15.0`
    expect(checkClientVersion(req(`ios/0.14.200`))?.status).toBe(426)
    delete process.env.CLIENT_MIN_VERSION_ANDROID
    expect(checkClientVersion(req(`android/0.1.0`))).toBeNull()
    expect(versionPayload()).toMatchObject({ desktop: { min: `0.14.171` }, cli: { min: `0.14.171` }, android: { min: null }, ios: { min: `0.15.0` } })
  })

  it(`ignores a -staging suffix on the client version`, () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.15.0`
    expect(checkClientVersion(req(`android/0.15.0-staging`))).toBeNull()
    expect(checkClientVersion(req(`android/0.13.2-staging`))?.status).toBe(426)
  })

  it(`fails open when the header is missing or unknown`, () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.15.0`
    expect(checkClientVersion(req())).toBeNull()
    expect(checkClientVersion(req(`web/0.1.0`))).toBeNull()
  })

  it(`fails open on a malformed client version`, () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.15.0`
    expect(checkClientVersion(req(`android/garbage`))).toBeNull()
  })

  it(`fails open when the min env var is unset or malformed`, () => {
    expect(checkClientVersion(req(`android/0.0.1`))).toBeNull()
    process.env.CLIENT_MIN_VERSION_ANDROID = `not-a-version`
    expect(checkClientVersion(req(`android/0.0.1`))).toBeNull()
  })

  it(`only gates the platform named in the header`, () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `99.0.0`
    expect(checkClientVersion(req(`ios/0.0.1`))).toBeNull()
  })
})

describe(`versionPayload`, () => {
  it(`returns nulls when nothing is configured`, () => {
    expect(versionPayload()).toEqual({
      android: { min: null, latest: null },
      ios: { min: null, latest: null },
      desktop: { min: null, latest: null },
      cli: { min: null, latest: null },
    })
  })

  it(`reflects the configured env vars`, () => {
    process.env.CLIENT_MIN_VERSION_ANDROID = `0.15.0`
    process.env.CLIENT_LATEST_VERSION_DESKTOP = `0.9.0`
    expect(versionPayload()).toEqual({
      android: { min: `0.15.0`, latest: null },
      ios: { min: null, latest: null },
      desktop: { min: null, latest: `0.9.0` },
      cli: { min: null, latest: null },
    })
  })
})
