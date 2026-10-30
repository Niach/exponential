// EXP-1471: a browser node has no discovery, so it only reaches a peer it
// has an address for. A teammate's install row gives the endpoint id; every
// install homes on the same relay, so the address is `enet1…` with that one
// relay (the daemon does the same in crates/steer/src/enet.rs
// `apply_allowlist`). Encoding = crates/enet-core/src/ticket.rs: `enet1` +
// lowercase unpadded base32 of [32 id][u8 relays, u8 len + url…][u8 addrs].

const BASE32 = `abcdefghijklmnopqrstuvwxyz234567`

function base32(bytes: Uint8Array): string {
  let out = ``
  let buffer = 0
  let bits = 0
  for (const byte of bytes) {
    buffer = (buffer << 8) | byte
    bits += 8
    while (bits >= 5) {
      bits -= 5
      out += BASE32[(buffer >> bits) & 31]
    }
    buffer &= (1 << bits) - 1
  }
  if (bits > 0) out += BASE32[(buffer << (5 - bits)) & 31]
  return out
}

/** `enet1…` for the hex `endpointId` homed on `relayUrl`; undefined for a malformed id. */
export function relayTicket(endpointId: string, relayUrl: string): string | undefined {
  if (!/^[0-9a-f]{64}$/i.test(endpointId)) return undefined
  const relay = new TextEncoder().encode(new URL(relayUrl).href)
  if (relay.length > 255) return undefined
  const bytes = new Uint8Array(32 + 2 + relay.length + 1)
  for (let i = 0; i < 32; i++) bytes[i] = Number.parseInt(endpointId.slice(i * 2, i * 2 + 2), 16)
  bytes[32] = 1
  bytes[33] = relay.length
  bytes.set(relay, 34)
  bytes[34 + relay.length] = 0
  return `enet1${base32(bytes)}`
}
