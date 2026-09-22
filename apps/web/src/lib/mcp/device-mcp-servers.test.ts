import { describe, expect, it } from "vitest"
import {
  configKey,
  deviceMcpServerSourceLabel,
  deviceMcpServerTarget,
  groupDeviceMcpServers,
  validateDeviceMcpServerInput,
  type DeviceMcpServerListRow,
} from "@/lib/mcp/device-mcp-servers"

// EXP-891: the presentation + validation rules the web pane and the
// `deviceMcpServers` router share.

function row(over: Partial<DeviceMcpServerListRow> = {}): DeviceMcpServerListRow {
  return {
    id: `r-${over.name ?? `x`}-${over.deviceRowId ?? `d1`}`,
    deviceRowId: `d1`,
    deviceId: `dev-1`,
    deviceLabel: `MacBook`,
    userId: `me`,
    ownerName: `Me`,
    name: `linear`,
    transport: `http`,
    url: `https://mcp.linear.app/mcp`,
    command: null,
    args: [],
    source: `detected`,
    agent: `claude`,
    enabled: true,
    updatedAt: `2026-09-19T00:00:00.000Z`,
    ...over,
  }
}

describe(`deviceMcpServerTarget`, () => {
  it(`is the URL for http and the command line for stdio`, () => {
    expect(deviceMcpServerTarget(row())).toBe(`https://mcp.linear.app/mcp`)
    expect(
      deviceMcpServerTarget(
        row({ transport: `stdio`, url: null, command: `npx`, args: [`-y`, `@acme/mcp`] })
      )
    ).toBe(`npx -y @acme/mcp`)
    expect(deviceMcpServerTarget(row({ url: null }))).toBe(``)
  })
})

describe(`deviceMcpServerSourceLabel`, () => {
  it(`names the local config a detected row came from`, () => {
    expect(deviceMcpServerSourceLabel(row())).toBe(`Detected from Claude Code`)
    expect(deviceMcpServerSourceLabel(row({ agent: `codex` }))).toBe(`Detected from Codex`)
    expect(deviceMcpServerSourceLabel(row({ agent: null }))).toBe(`Detected on the device`)
    expect(deviceMcpServerSourceLabel(row({ source: `manual`, agent: null }))).toBe(
      `Added on the device`
    )
  })
})

describe(`groupDeviceMcpServers`, () => {
  it(`groups per device, own machines first, names case-insensitively`, () => {
    const rows = [
      row({ deviceRowId: `d2`, deviceId: `dev-2`, deviceLabel: `mini`, userId: `other`, ownerName: `Sam`, name: `sentry` }),
      row({ name: `Zed` }),
      row({ name: `atlas` }),
      row({ deviceRowId: `d3`, deviceId: `dev-3`, deviceLabel: `Air`, name: `linear` }),
    ]
    const groups = groupDeviceMcpServers(rows, `me`)
    expect(groups.map((g) => g.deviceLabel)).toEqual([`Air`, `MacBook`, `mini`])
    expect(groups[1]!.servers.map((s) => s.name)).toEqual([`atlas`, `Zed`])
    expect(groups[2]!.ownerName).toBe(`Sam`)
  })
})

describe(`validateDeviceMcpServerInput`, () => {
  const base = { source: `manual` as const, enabled: true }
  it(`accepts an https url and a loopback http url`, () => {
    expect(
      validateDeviceMcpServerInput({ ...base, name: `linear`, transport: `http`, url: `https://x.y/mcp` })
    ).toBeNull()
    expect(
      validateDeviceMcpServerInput({ ...base, name: `local`, transport: `http`, url: `http://localhost:3333/mcp` })
    ).toBeNull()
  })
  it(`refuses a missing or plain-http url, a missing command, blank and reserved names`, () => {
    expect(validateDeviceMcpServerInput({ ...base, name: `a`, transport: `http` })).toMatch(/needs a URL/)
    expect(
      validateDeviceMcpServerInput({ ...base, name: `a`, transport: `http`, url: `http://x.y/mcp` })
    ).toMatch(/https/)
    expect(validateDeviceMcpServerInput({ ...base, name: `a`, transport: `http`, url: `nope` })).toMatch(/parse/)
    expect(validateDeviceMcpServerInput({ ...base, name: `a`, transport: `stdio` })).toMatch(/command/)
    expect(validateDeviceMcpServerInput({ ...base, name: `  `, transport: `stdio`, command: `x` })).toMatch(/name/)
    expect(
      validateDeviceMcpServerInput({ ...base, name: `Exponential`, transport: `http`, url: `https://x.y/` })
    ).toMatch(/reserved/)
    expect(
      validateDeviceMcpServerInput({ ...base, name: `x`.repeat(65), transport: `stdio`, command: `x` })
    ).toMatch(/at most/)
  })
  it(`matches loopback on the exact hostname`, () => {
    for (const url of [
      `http://localhost.corp/mcp`,
      `http://127.0.0.1.evil.example/mcp`,
      `http://localhost@evil.example/mcp`,
      `http://192.168.1.10:3000/mcp`,
    ]) {
      expect(
        validateDeviceMcpServerInput({ ...base, name: `a`, transport: `http`, url })
      ).toMatch(/must be https/)
    }
  })
  // Review B1: teammates read these rows, so no credential rides in a URL.
  it(`refuses a username, a password or a query string in the URL`, () => {
    for (const url of [
      `https://user:pass@mcp.example.com/mcp`,
      `https://token@mcp.example.com/mcp`,
      `https://:pass@mcp.example.com/mcp`,
      `https://mcp.example.com/mcp?api_key=sk-123`,
      `http://localhost:3000/mcp?token=abc`,
    ]) {
      expect(
        validateDeviceMcpServerInput({ ...base, name: `leaky`, transport: `http`, url })
      ).toBe(
        `leaky: the URL must not carry a username, a password or a query string (they would be shared with your team)`
      )
    }
    expect(
      validateDeviceMcpServerInput({
        ...base,
        name: `ok`,
        transport: `http`,
        url: `https://mcp.example.com/mcp#section`,
      })
    ).toBeNull()
  })
})

describe(`configKey`, () => {
  it(`folds like the desktop McpServerWire::config_key`, () => {
    expect(configKey(`Linear`)).toBe(`linear`)
    expect(configKey(`My Server-2`)).toBe(`my_server_2`)
    expect(configKey(`   `)).toBe(`server`)
  })
})
