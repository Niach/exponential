import { describe, expect, it } from "vitest"
import { isPublicAddress, resolvesToPublicAddresses } from "@/lib/import/public-address"

// EXP-630: the address classifier behind the bundle source's SSRF guard.

describe(`isPublicAddress`, () => {
  it.each([
    `1.1.1.1`,
    `8.8.8.8`,
    `93.184.216.34`,
    `100.63.255.255`,
    `100.128.0.0`,
    `172.15.255.255`,
    `172.32.0.0`,
    `2606:4700:4700::1111`,
    `2a00:1450:4001:82a::200e`,
    `::ffff:8.8.8.8`,
    `64:ff9b::808:808`,
    `2002:0808:0808::`,
  ])(`accepts the public address %s`, (ip) => {
    expect(isPublicAddress(ip)).toBe(true)
  })

  it.each([
    [`0.0.0.0`, `unspecified`],
    [`10.1.2.3`, `RFC 1918`],
    [`100.64.0.1`, `CGNAT`],
    [`100.127.255.254`, `CGNAT`],
    [`127.0.0.1`, `loopback`],
    [`127.255.255.255`, `loopback`],
    [`169.254.169.254`, `link-local / cloud metadata`],
    [`172.16.0.1`, `RFC 1918`],
    [`172.31.255.255`, `RFC 1918`],
    [`192.0.0.1`, `IETF protocol assignments`],
    [`192.0.2.1`, `TEST-NET-1`],
    [`192.168.1.1`, `RFC 1918`],
    [`198.18.0.1`, `benchmarking`],
    [`198.51.100.1`, `TEST-NET-2`],
    [`203.0.113.1`, `TEST-NET-3`],
    [`224.0.0.1`, `multicast`],
    [`240.0.0.1`, `reserved`],
    [`255.255.255.255`, `broadcast`],
    [`::`, `v6 unspecified`],
    [`::1`, `v6 loopback`],
    [`[::1]`, `bracketed v6 loopback`],
    [`fe80::1`, `v6 link-local`],
    [`fe80::1%en0`, `v6 link-local with a zone`],
    [`febf::1`, `v6 link-local upper bound`],
    [`fc00::1`, `ULA`],
    [`fd12:3456::1`, `ULA`],
    [`fec0::1`, `site-local`],
    [`ff02::1`, `v6 multicast`],
    [`::ffff:127.0.0.1`, `v4-mapped loopback`],
    [`::ffff:7f00:1`, `v4-mapped loopback, hex form`],
    [`::ffff:10.0.0.1`, `v4-mapped RFC 1918`],
    [`::ffff:169.254.169.254`, `v4-mapped metadata`],
    [`::10.0.0.1`, `v4-compatible RFC 1918`],
    [`64:ff9b::a00:1`, `NAT64 to RFC 1918`],
    [`64:ff9b::127.0.0.1`, `NAT64 to loopback`],
    [`2002:0a00:0001::`, `6to4 carrying RFC 1918`],
    [`2001:db8::1`, `documentation prefix`],
    [`not-an-ip`, `garbage`],
    [``, `empty`],
    [`1.2.3`, `short v4`],
    [`1:2:3:4:5:6:7:8:9`, `overlong v6`],
  ])(`rejects %s (%s)`, (ip) => {
    expect(isPublicAddress(ip)).toBe(false)
  })
})

describe(`resolvesToPublicAddresses`, () => {
  it(`accepts a host whose every answer is public`, async () => {
    const ok = await resolvesToPublicAddresses(`files.example`, async () => [
      { address: `93.184.216.34` },
      { address: `2606:2800:220:1:248:1893:25c8:1946` },
    ])
    expect(ok).toBe(true)
  })

  it(`refuses when ANY answer is private`, async () => {
    const ok = await resolvesToPublicAddresses(`files.example`, async () => [
      { address: `93.184.216.34` },
      { address: `10.0.0.5` },
    ])
    expect(ok).toBe(false)
  })

  it(`refuses a host with no answers`, async () => {
    expect(await resolvesToPublicAddresses(`files.example`, async () => [])).toBe(false)
  })

  it(`lets a lookup failure propagate`, async () => {
    await expect(
      resolvesToPublicAddresses(`files.example`, async () => {
        throw new Error(`ENOTFOUND`)
      })
    ).rejects.toThrow(/ENOTFOUND/)
  })
})
