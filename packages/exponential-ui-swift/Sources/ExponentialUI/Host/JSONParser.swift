import Foundation

/// VAPP-142: the facade's JSON strings (every node's props and
/// accessibility, every leaf request) parsed straight from their UTF-8
/// bytes into `JSONValue`: what `JSONSerialization` + `JSONValue(any:)`
/// answer (RFC 8259 plus Foundation's leniencies: top-level scalars, a
/// trailing comma, the FIRST duplicate key wins; any malformed input =
/// nil; a long number correctly rounded),
/// without the Foundation object tree
/// and its dynamic casts (≈ 6× on a props object).
/// `JSONParserTests` holds it to `JSONSerialization` over every fixture.
struct JSONParser {
    private let bytes: UnsafeBufferPointer<UInt8>
    private var at = 0
    private var depth = 0
    /// Deeper than any surface the core emits; `JSONSerialization` refuses far less deep input too.
    private static let maxDepth = 512

    private init(_ bytes: UnsafeBufferPointer<UInt8>) { self.bytes = bytes }

    static func parse(_ json: String) -> JSONValue? {
        var json = json
        return json.withUTF8 { buffer in
            var p = JSONParser(buffer)
            p.skipSpace()
            guard let v = p.value() else { return nil }
            p.skipSpace()
            return p.at == buffer.count ? v : nil
        }
    }

    private mutating func skipSpace() {
        while at < bytes.count {
            switch bytes[at] {
            case 0x20, 0x09, 0x0A, 0x0D: at += 1
            default: return
            }
        }
    }

    private mutating func literal(_ word: StaticString) -> Bool {
        let n = word.utf8CodeUnitCount
        guard at + n <= bytes.count else { return false }
        let w = word.utf8Start
        for k in 0..<n where bytes[at + k] != w[k] { return false }
        at += n
        return true
    }

    private mutating func value() -> JSONValue? {
        guard at < bytes.count else { return nil }
        switch bytes[at] {
        case UInt8(ascii: "{"): return object()
        case UInt8(ascii: "["): return array()
        case UInt8(ascii: "\""): return string().map(JSONValue.string)
        case UInt8(ascii: "t"): return literal("true") ? .bool(true) : nil
        case UInt8(ascii: "f"): return literal("false") ? .bool(false) : nil
        case UInt8(ascii: "n"): return literal("null") ? .null : nil
        default: return number()
        }
    }

    private mutating func object() -> JSONValue? {
        depth += 1
        defer { depth -= 1 }
        guard depth <= Self.maxDepth else { return nil }
        at += 1
        var o: [String: JSONValue] = [:]
        skipSpace()
        if at < bytes.count, bytes[at] == UInt8(ascii: "}") { at += 1; return .object(o) }
        while true {
            skipSpace()
            if at < bytes.count, bytes[at] == UInt8(ascii: "}"), !o.isEmpty { at += 1; return .object(o) }
            guard at < bytes.count, bytes[at] == UInt8(ascii: "\""), let key = string() else { return nil }
            skipSpace()
            guard at < bytes.count, bytes[at] == UInt8(ascii: ":") else { return nil }
            at += 1
            skipSpace()
            guard let v = value() else { return nil }
            if o[key] == nil { o[key] = v }
            skipSpace()
            guard at < bytes.count else { return nil }
            if bytes[at] == UInt8(ascii: ",") { at += 1; continue }
            if bytes[at] == UInt8(ascii: "}") { at += 1; return .object(o) }
            return nil
        }
    }

    private mutating func array() -> JSONValue? {
        depth += 1
        defer { depth -= 1 }
        guard depth <= Self.maxDepth else { return nil }
        at += 1
        var a: [JSONValue] = []
        skipSpace()
        if at < bytes.count, bytes[at] == UInt8(ascii: "]") { at += 1; return .array(a) }
        while true {
            skipSpace()
            if at < bytes.count, bytes[at] == UInt8(ascii: "]"), !a.isEmpty { at += 1; return .array(a) }
            guard let v = value() else { return nil }
            a.append(v)
            skipSpace()
            guard at < bytes.count else { return nil }
            if bytes[at] == UInt8(ascii: ",") { at += 1; continue }
            if bytes[at] == UInt8(ascii: "]") { at += 1; return .array(a) }
            return nil
        }
    }

    /// A string at `"`: the plain run decoded in one go, escapes through a byte buffer.
    private mutating func string() -> String? {
        at += 1
        let start = at
        while at < bytes.count {
            let b = bytes[at]
            if b == UInt8(ascii: "\"") {
                let s = String(decoding: UnsafeBufferPointer(rebasing: bytes[start..<at]), as: UTF8.self)
                at += 1
                return s
            }
            if b == UInt8(ascii: "\\") { break }
            if b < 0x20 { return nil }
            at += 1
        }
        guard at < bytes.count else { return nil }
        var out = [UInt8](UnsafeBufferPointer(rebasing: bytes[start..<at]))
        while at < bytes.count {
            let b = bytes[at]
            switch b {
            case UInt8(ascii: "\""):
                at += 1
                return String(decoding: out, as: UTF8.self)
            case UInt8(ascii: "\\"):
                at += 1
                guard at < bytes.count else { return nil }
                let e = bytes[at]
                at += 1
                switch e {
                case UInt8(ascii: "\""), UInt8(ascii: "\\"), UInt8(ascii: "/"): out.append(e)
                case UInt8(ascii: "b"): out.append(0x08)
                case UInt8(ascii: "f"): out.append(0x0C)
                case UInt8(ascii: "n"): out.append(0x0A)
                case UInt8(ascii: "r"): out.append(0x0D)
                case UInt8(ascii: "t"): out.append(0x09)
                case UInt8(ascii: "u"):
                    guard var scalar = hex4() else { return nil }
                    if (0xD800...0xDBFF).contains(scalar) {
                        // A high surrogate needs its low half (`😀`).
                        guard at + 1 < bytes.count, bytes[at] == UInt8(ascii: "\\"), bytes[at + 1] == UInt8(ascii: "u") else { return nil }
                        at += 2
                        guard let low = hex4(), (0xDC00...0xDFFF).contains(low) else { return nil }
                        scalar = 0x10000 + ((scalar - 0xD800) << 10) + (low - 0xDC00)
                    } else if (0xDC00...0xDFFF).contains(scalar) {
                        return nil
                    }
                    guard let u = Unicode.Scalar(scalar) else { return nil }
                    out.append(contentsOf: UTF8.encode(u)!)
                default: return nil
                }
            default:
                if b < 0x20 { return nil }
                out.append(b)
                at += 1
            }
        }
        return nil
    }

    private mutating func hex4() -> UInt32? {
        guard at + 4 <= bytes.count else { return nil }
        var v: UInt32 = 0
        for _ in 0..<4 {
            let b = bytes[at]
            let d: UInt8
            switch b {
            case UInt8(ascii: "0")...UInt8(ascii: "9"): d = b - UInt8(ascii: "0")
            case UInt8(ascii: "a")...UInt8(ascii: "f"): d = b - UInt8(ascii: "a") + 10
            case UInt8(ascii: "A")...UInt8(ascii: "F"): d = b - UInt8(ascii: "A") + 10
            default: return nil
            }
            v = v << 4 | UInt32(d)
            at += 1
        }
        return v
    }

    /// RFC 8259's number grammar; a short integer exactly, anything else through `Double(String)` (strtod).
    private mutating func number() -> JSONValue? {
        let start = at
        func digit(_ b: UInt8) -> Bool { b >= UInt8(ascii: "0") && b <= UInt8(ascii: "9") }
        if at < bytes.count, bytes[at] == UInt8(ascii: "-") { at += 1 }
        guard at < bytes.count, digit(bytes[at]) else { return nil }
        if bytes[at] == UInt8(ascii: "0") {
            at += 1
        } else {
            while at < bytes.count, digit(bytes[at]) { at += 1 }
        }
        var integral = true
        if at < bytes.count, bytes[at] == UInt8(ascii: ".") {
            integral = false
            at += 1
            guard at < bytes.count, digit(bytes[at]) else { return nil }
            while at < bytes.count, digit(bytes[at]) { at += 1 }
        }
        if at < bytes.count, bytes[at] == UInt8(ascii: "e") || bytes[at] == UInt8(ascii: "E") {
            integral = false
            at += 1
            if at < bytes.count, bytes[at] == UInt8(ascii: "+") || bytes[at] == UInt8(ascii: "-") { at += 1 }
            guard at < bytes.count, digit(bytes[at]) else { return nil }
            while at < bytes.count, digit(bytes[at]) { at += 1 }
        }
        let negative = bytes[start] == UInt8(ascii: "-")
        let digits = at - start - (negative ? 1 : 0)
        if integral, digits <= 15 {
            var v: Int64 = 0
            for k in (start + (negative ? 1 : 0))..<at { v = v * 10 + Int64(bytes[k] - UInt8(ascii: "0")) }
            // `-0` is an integer zero to JSONSerialization too.
            return .number(Double(negative ? -v : v))
        }
        // Correctly rounded (strtod; Foundation's decimal path can land one
        // ulp off on a 17+ digit integer, the core and the web never do).
        // Out of range = malformed, as `JSONSerialization` refuses it.
        let text = String(decoding: UnsafeBufferPointer(rebasing: bytes[start..<at]), as: UTF8.self)
        guard let d = Double(text), d.isFinite else { return nil }
        return .number(d)
    }
}

extension JSONValue {
    /// VAPP-131: the JSON handed to the core (`setData`, `apply`, a recipe
    /// query) written straight to UTF-8: keys sorted (a stable cache key),
    /// compact, a whole number < 1e15 without a fraction, a non-finite
    /// number as `null` (what `json` writes for them too). The same VALUE
    /// as `json` (`parse(wireJSON) == parse(json)`), not the same bytes:
    /// `JSONSerialization` prints doubles with 17 digits and escapes `/`.
    var wireJSON: String {
        var out: [UInt8] = []
        out.reserveCapacity(64)
        JSONValue.write(self, into: &out)
        return String(decoding: out, as: UTF8.self)
    }

    private static let hexDigits = Array("0123456789abcdef".utf8)

    private static func write(_ v: JSONValue, into out: inout [UInt8]) {
        switch v {
        case .null: out.append(contentsOf: "null".utf8)
        case let .bool(b): out.append(contentsOf: (b ? "true" : "false").utf8)
        case let .number(n):
            if !n.isFinite {
                out.append(contentsOf: "null".utf8)
            } else if n == n.rounded(), abs(n) < 1e15 {
                out.append(contentsOf: String(Int64(n)).utf8)
            } else {
                out.append(contentsOf: n.description.utf8)
            }
        case let .string(s): writeString(s, into: &out)
        case let .array(a):
            out.append(UInt8(ascii: "["))
            for (k, e) in a.enumerated() {
                if k > 0 { out.append(UInt8(ascii: ",")) }
                write(e, into: &out)
            }
            out.append(UInt8(ascii: "]"))
        case let .object(o):
            out.append(UInt8(ascii: "{"))
            for (k, key) in o.keys.sorted().enumerated() {
                if k > 0 { out.append(UInt8(ascii: ",")) }
                writeString(key, into: &out)
                out.append(UInt8(ascii: ":"))
                write(o[key]!, into: &out)
            }
            out.append(UInt8(ascii: "}"))
        }
    }

    private static func writeString(_ s: String, into out: inout [UInt8]) {
        out.append(UInt8(ascii: "\""))
        for b in s.utf8 {
            switch b {
            case UInt8(ascii: "\""): out.append(contentsOf: [UInt8(ascii: "\\"), UInt8(ascii: "\"")])
            case UInt8(ascii: "\\"): out.append(contentsOf: [UInt8(ascii: "\\"), UInt8(ascii: "\\")])
            case 0x0A: out.append(contentsOf: [UInt8(ascii: "\\"), UInt8(ascii: "n")])
            case 0x0D: out.append(contentsOf: [UInt8(ascii: "\\"), UInt8(ascii: "r")])
            case 0x09: out.append(contentsOf: [UInt8(ascii: "\\"), UInt8(ascii: "t")])
            case 0x00..<0x20:
                out.append(contentsOf: "\\u00".utf8)
                out.append(hexDigits[Int(b >> 4)])
                out.append(hexDigits[Int(b & 0xF)])
            default: out.append(b)
            }
        }
        out.append(UInt8(ascii: "\""))
    }
}

extension Dictionary where Key == String, Value == JSONValue {
    /// `JSONValue.wireJSON` of props (a recipe query, its cache key).
    var wireJSON: String { JSONValue.object(self).wireJSON }
}
