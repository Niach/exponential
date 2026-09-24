// EXP-630: the bundle source fetches asset refs a script handed it, so a
// hostname check alone is not enough against SSRF — `evil.example` may
// resolve to 10.0.0.5 or ::1. `isPublicAddress` classifies one literal
// address (v4, v6, and the v4 carried inside v4-mapped / NAT64 / 6to4 v6);
// `resolvesToPublicAddresses` looks the host up and refuses when ANY
// answer is non-public, so a mixed record cannot slip a private target
// past a lucky pick.
//
// Bun's native fetch takes no custom dispatcher, so the fetch cannot be
// pinned to the vetted address; the caller resolves IMMEDIATELY before
// fetching with `redirect: "error"`, which leaves a DNS-rebind race of a few
// milliseconds as the residual — accepted for an owner-only, rate-limited
// path.
import { lookup as dnsLookup } from "node:dns/promises"
import { isIP } from "node:net"

type Bytes4 = [number, number, number, number]

function parseIPv4(ip: string): Bytes4 | null {
  if (isIP(ip) !== 4) return null
  return ip.split(`.`).map(Number) as Bytes4
}

function isPublicIPv4([a, b, c, d]: Bytes4): boolean {
  if (a === 0) return false // 0.0.0.0/8 "this network"
  if (a === 10) return false // RFC 1918
  if (a === 100 && b >= 64 && b <= 127) return false // 100.64/10 CGNAT
  if (a === 127) return false // loopback
  if (a === 169 && b === 254) return false // link-local (cloud metadata)
  if (a === 172 && b >= 16 && b <= 31) return false // RFC 1918
  if (a === 192 && b === 0 && c === 0) return false // 192.0.0/24 IETF
  if (a === 192 && b === 0 && c === 2) return false // TEST-NET-1
  if (a === 192 && b === 168) return false // RFC 1918
  if (a === 198 && (b === 18 || b === 19)) return false // benchmarking
  if (a === 198 && b === 51 && c === 100) return false // TEST-NET-2
  if (a === 203 && b === 0 && c === 113) return false // TEST-NET-3
  if (a >= 224) return false // multicast + reserved + broadcast
  void d
  return true
}

// 16 bytes, or null when the text is not an IPv6 literal.
function parseIPv6(raw: string): Uint8Array | null {
  const ip = raw.replace(/^\[|\]$/g, ``).split(`%`)[0]!
  if (isIP(ip) !== 6) return null
  const halves = ip.split(`::`)
  if (halves.length > 2) return null
  const expand = (part: string): number[] => {
    if (part === ``) return []
    const words: number[] = []
    for (const group of part.split(`:`)) {
      if (group.includes(`.`)) {
        const v4 = parseIPv4(group)
        if (!v4) throw new Error(`bad v4 tail`)
        words.push((v4[0] << 8) | v4[1], (v4[2] << 8) | v4[3])
      } else {
        words.push(parseInt(group, 16))
      }
    }
    return words
  }
  let words: number[]
  try {
    const head = expand(halves[0]!)
    const tail = halves.length === 2 ? expand(halves[1]!) : []
    const fill = 8 - head.length - tail.length
    if (fill < 0 || (halves.length === 1 && fill !== 0)) return null
    words = [...head, ...new Array<number>(Math.max(fill, 0)).fill(0), ...tail]
  } catch {
    return null
  }
  const bytes = new Uint8Array(16)
  words.forEach((word, index) => {
    bytes[index * 2] = word >> 8
    bytes[index * 2 + 1] = word & 0xff
  })
  return bytes
}

function isPublicIPv6(bytes: Uint8Array): boolean {
  const allZero = (from: number, to: number) => bytes.slice(from, to).every((b) => b === 0)
  // :: and ::1
  if (allZero(0, 15) && (bytes[15] === 0 || bytes[15] === 1)) return false
  // ::ffff:a.b.c.d (v4-mapped) and ::a.b.c.d (v4-compatible, deprecated)
  if (allZero(0, 10) && ((bytes[10] === 0xff && bytes[11] === 0xff) || (bytes[10] === 0 && bytes[11] === 0))) {
    return isPublicIPv4([bytes[12]!, bytes[13]!, bytes[14]!, bytes[15]!])
  }
  // 64:ff9b::/96 NAT64 — the v4 target rides in the last 32 bits
  if (bytes[0] === 0x00 && bytes[1] === 0x64 && bytes[2] === 0xff && bytes[3] === 0x9b && allZero(4, 12)) {
    return isPublicIPv4([bytes[12]!, bytes[13]!, bytes[14]!, bytes[15]!])
  }
  // 2002:a.b.c.d::/48 6to4 — the v4 lives in bytes 2..5
  if (bytes[0] === 0x20 && bytes[1] === 0x02) {
    return isPublicIPv4([bytes[2]!, bytes[3]!, bytes[4]!, bytes[5]!])
  }
  if ((bytes[0]! & 0xfe) === 0xfc) return false // fc00::/7 ULA
  if (bytes[0] === 0xfe && (bytes[1]! & 0xc0) === 0x80) return false // fe80::/10 link-local
  if (bytes[0] === 0xfe && (bytes[1]! & 0xc0) === 0xc0) return false // fec0::/10 site-local (deprecated)
  if (bytes[0] === 0xff) return false // ff00::/8 multicast
  if (bytes[0] === 0x20 && bytes[1] === 0x01 && bytes[2] === 0x0d && bytes[3] === 0xb8) return false // 2001:db8::/32 docs
  return true
}

// True only for a globally routable unicast address. Anything unparseable
// is NOT public.
export function isPublicAddress(ip: string): boolean {
  const v4 = parseIPv4(ip)
  if (v4) return isPublicIPv4(v4)
  const v6 = parseIPv6(ip)
  if (v6) return isPublicIPv6(v6)
  return false
}

export type AddressLookup = (host: string) => Promise<{ address: string }[]>

const defaultLookup: AddressLookup = (host) => dnsLookup(host, { all: true })

// Every answer must be public; no answer at all is a refusal too.
export async function resolvesToPublicAddresses(
  host: string,
  lookup: AddressLookup = defaultLookup
): Promise<boolean> {
  const answers = await lookup(host)
  if (answers.length === 0) return false
  return answers.every((answer) => isPublicAddress(answer.address))
}
