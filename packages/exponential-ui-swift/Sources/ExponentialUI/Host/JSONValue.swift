import Foundation

/// A JSON value (props, payloads, data) the painter reads and the host
/// receives. Parsed from the facade's JSON strings once per structure.
public enum JSONValue: Equatable, Sendable, CustomStringConvertible {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(any: Any) {
        switch any {
        case let n as NSNumber:
            // JSONSerialization hands booleans as NSNumber too (and a Swift
            // `as Bool` would swallow 0 / 1): the CFBoolean type id decides.
            if CFGetTypeID(n) == CFBooleanGetTypeID() { self = .bool(n.boolValue) } else { self = .number(n.doubleValue) }
        case let b as Bool: self = .bool(b)
        case let s as String: self = .string(s)
        case let a as [Any]: self = .array(a.map { JSONValue(any: $0) })
        case let o as [String: Any]: self = .object(o.mapValues { JSONValue(any: $0) })
        default: self = .null
        }
    }

    /// Parse a JSON document (`{}` on failure).
    public static func parse(_ json: String) -> JSONValue {
        guard let data = json.data(using: .utf8), let any = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) else { return .null }
        return JSONValue(any: any)
    }

    public var any: Any {
        switch self {
        case .null: NSNull()
        case let .bool(b): b
        case let .number(n): n
        case let .string(s): s
        case let .array(a): a.map(\.any)
        case let .object(o): o.mapValues(\.any)
        }
    }

    /// Serialized (compact).
    public var json: String {
        guard let data = try? JSONSerialization.data(withJSONObject: any, options: [.fragmentsAllowed, .sortedKeys]) else { return "null" }
        return String(decoding: data, as: UTF8.self)
    }

    public var description: String { json }

    public subscript(key: String) -> JSONValue? {
        if case let .object(o) = self { return o[key] }
        return nil
    }

    public var string: String? {
        if case let .string(s) = self { return s }
        return nil
    }

    /// A scalar as display text (`3` → "3"; the React `String(v)`).
    public var displayText: String {
        switch self {
        case .null: ""
        case let .string(s): s
        case let .bool(b): b ? "true" : "false"
        case let .number(n): n == n.rounded() && abs(n) < 1e15 ? String(Int64(n)) : String(n)
        case .array, .object: json
        }
    }

    public var number: Double? {
        switch self {
        case let .number(n): n
        case let .string(s): Double(s.trimmingCharacters(in: .whitespaces))
        case let .bool(b): b ? 1 : 0
        default: nil
        }
    }

    public var bool: Bool? {
        switch self {
        case let .bool(b): b
        case let .string(s): s == "true" ? true : (s == "false" ? false : nil)
        default: nil
        }
    }

    public var array: [JSONValue]? {
        if case let .array(a) = self { return a }
        return nil
    }

    public var object: [String: JSONValue]? {
        if case let .object(o) = self { return o }
        return nil
    }

    public var isNull: Bool {
        if case .null = self { return true }
        return false
    }
}

/// A node's props (`[String: JSONValue]`) with typed readers.
public typealias Props = [String: JSONValue]

public extension Props {
    func str(_ key: String) -> String { self[key]?.string ?? "" }
    func text(_ key: String) -> String { self[key]?.displayText ?? "" }
    func num(_ key: String) -> Double? { self[key]?.number }
    func flag(_ key: String) -> Bool { self[key]?.bool ?? false }
    func list(_ key: String) -> [JSONValue] { self[key]?.array ?? [] }

    /// A CSS-ish length prop in px (number / `"Npx"`); percentages return nil.
    func px(_ key: String) -> CGFloat? {
        switch self[key] {
        case let .number(n): return CGFloat(n)
        case let .string(s):
            let t = s.trimmingCharacters(in: .whitespaces)
            if t.hasSuffix("%") { return nil }
            return Double(t.hasSuffix("px") ? String(t.dropLast(2)) : t).map { CGFloat($0) }
        default: return nil
        }
    }

    /// A `"N%"` prop as a fraction.
    func percent(_ key: String) -> CGFloat? {
        guard case let .string(s) = self[key], s.hasSuffix("%"), let p = Double(s.dropLast()) else { return nil }
        return CGFloat(p / 100)
    }

    var json: String { JSONValue.object(self).json }
}
