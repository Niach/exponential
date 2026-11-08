import { describe, expect, it } from "vitest"
import {
  allowedDsns,
  deriveEnvironment,
  internalEnvelopeUrl,
  monitoringConfig,
  parseDsn,
  tracesSampleRate,
} from "./config"

const DSN = (key: string, id: number) => `https://${key}@glitchtip.example/${id}`
const BASE = {
  GLITCHTIP_INTERNAL_URL: `http://glitchtip-web:8000/`,
  SENTRY_DSN_WEB: DSN(`webkey`, 1),
  SENTRY_DSN_SERVER: DSN(`srvkey`, 2),
  SENTRY_DSN_DESKTOP: DSN(`deskkey`, 3),
}

describe(`monitoringConfig`, () => {
  it(`is off without the internal URL or the platform's DSN`, () => {
    expect(monitoringConfig({}, `web`, true)).toEqual({ enabled: false })
    expect(monitoringConfig({ SENTRY_DSN_WEB: DSN(`k`, 1) }, `web`, true)).toEqual({
      enabled: false,
    })
    expect(monitoringConfig(BASE, `ios`, true)).toEqual({ enabled: false })
    expect(monitoringConfig({ ...BASE, SENTRY_DSN_WEB: `nope` }, `web`, true)).toEqual({
      enabled: false,
    })
  })

  it(`answers the platform's DSN; cli shares desktop`, () => {
    expect(monitoringConfig(BASE, `web`, true)).toEqual({
      enabled: true,
      dsn: DSN(`webkey`, 1),
      environment: `prod`,
      tracesSampleRate: 0.1,
    })
    const cli = monitoringConfig(BASE, `cli`, true)
    expect(cli.enabled && cli.dsn).toBe(DSN(`deskkey`, 3))
  })
})

describe(`deriveEnvironment / tracesSampleRate`, () => {
  it(`prefers SENTRY_ENVIRONMENT, else derives from the build and base URL`, () => {
    expect(deriveEnvironment({ SENTRY_ENVIRONMENT: `staging` }, false)).toBe(`staging`)
    expect(deriveEnvironment({}, false)).toBe(`dev`)
    expect(deriveEnvironment({ BETTER_AUTH_URL: `https://next.exponential.at` }, true)).toBe(
      `staging`
    )
    expect(deriveEnvironment({ BETTER_AUTH_URL: `https://app.exponential.at` }, true)).toBe(`prod`)
    expect(deriveEnvironment({}, true)).toBe(`prod`)
  })

  it(`clamps the trace rate to [0, 1], default 0.1`, () => {
    expect(tracesSampleRate({})).toBe(0.1)
    expect(tracesSampleRate({ SENTRY_TRACES_SAMPLE_RATE: `0.5` })).toBe(0.5)
    expect(tracesSampleRate({ SENTRY_TRACES_SAMPLE_RATE: `0` })).toBe(0)
    expect(tracesSampleRate({ SENTRY_TRACES_SAMPLE_RATE: `2` })).toBe(0.1)
    expect(tracesSampleRate({ SENTRY_TRACES_SAMPLE_RATE: `x` })).toBe(0.1)
  })
})

describe(`parseDsn / allowedDsns / internalEnvelopeUrl`, () => {
  it(`parses a DSN and rejects non-DSNs`, () => {
    expect(parseDsn(DSN(`abc`, 42))).toEqual({
      publicKey: `abc`,
      host: `glitchtip.example`,
      projectId: `42`,
    })
    expect(parseDsn(`https://glitchtip.example/42`)).toBeNull()
    expect(parseDsn(`https://k@glitchtip.example/`)).toBeNull()
    expect(parseDsn(`garbage`)).toBeNull()
  })

  it(`allows exactly the configured projects`, () => {
    expect([...allowedDsns(BASE).keys()].sort()).toEqual([`1`, `2`, `3`])
    expect(allowedDsns({ ...BASE, GLITCHTIP_INTERNAL_URL: `` }).size).toBe(0)
  })

  it(`targets the internal envelope endpoint`, () => {
    expect(internalEnvelopeUrl(`http://glitchtip-web:8000`, parseDsn(DSN(`k`, 7))!)).toBe(
      `http://glitchtip-web:8000/api/7/envelope/?sentry_key=k&sentry_version=7`
    )
  })
})
