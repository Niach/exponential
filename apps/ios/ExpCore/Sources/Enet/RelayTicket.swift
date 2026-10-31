import Foundation

// EXP-1471 on iOS: the node has no discovery, so it only reaches a peer it
// has an address for. A teammate's install row gives the endpoint id; every
// install homes on the same relay, so the address is `enet1…` with that one
// relay (the web's relay-ticket.ts, the daemon's `apply_allowlist`).
// Encoding = crates/enet-core/src/ticket.rs: `enet1` + lowercase unpadded
// base32 of [32 id][u8 relays, u8 len + url…][u8 addrs].
public enum RelayTicket {
    public static let relayUrl = "https://relay.exponential.at"

    private static let alphabet = Array("abcdefghijklmnopqrstuvwxyz234567")

    /// `enet1…` for the hex `endpointId` homed on `relayUrl`; nil for a malformed id.
    public static func encode(endpointId: String, relayUrl: String = relayUrl) -> String? {
        let hex = Array(endpointId.lowercased().utf8)
        guard hex.count == 64 else { return nil }
        var bytes: [UInt8] = []
        for i in stride(from: 0, to: 64, by: 2) {
            guard let byte = UInt8(String(decoding: hex[i..<i + 2], as: UTF8.self), radix: 16) else { return nil }
            bytes.append(byte)
        }
        // `new URL(…).href` / iroh's RelayUrl: an empty path prints as `/`.
        guard var url = URLComponents(string: relayUrl), url.host != nil else { return nil }
        if url.path.isEmpty { url.path = "/" }
        guard let href = url.string else { return nil }
        let relay = Array(href.utf8)
        guard relay.count <= 255 else { return nil }
        bytes.append(1)
        bytes.append(UInt8(relay.count))
        bytes.append(contentsOf: relay)
        bytes.append(0)
        return "enet1" + base32(bytes)
    }

    private static func base32(_ bytes: [UInt8]) -> String {
        var out = ""
        var buffer = 0
        var bits = 0
        for byte in bytes {
            buffer = (buffer << 8) | Int(byte)
            bits += 8
            while bits >= 5 {
                bits -= 5
                out.append(alphabet[(buffer >> bits) & 31])
            }
            buffer &= (1 << bits) - 1
        }
        if bits > 0 { out.append(alphabet[(buffer << (5 - bits)) & 31]) }
        return out
    }
}
