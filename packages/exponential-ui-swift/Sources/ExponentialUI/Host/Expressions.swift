import Foundation
import ExponentialUICore

/// VAPP-191 (eApps Q2): the expression language (a CEL subset with maths,
/// date/time and duration functions; `catalog/expressions.json`) through
/// the core's one evaluator and type checker over the FFI. The TS
/// reference, the Rust core and Compose's `Expressions` answer the same
/// (`fixtures/expressions.json`).
public enum Expressions {
    /// An evaluation: the value, or why it has none (`column N: …`).
    public enum Outcome: Equatable {
        case value(JSONValue)
        case error(String)
    }

    /// A type check: the expression's static type and every problem.
    public struct Check: Equatable {
        public let type: String
        public let issues: [String]
    }

    /// `source` over `vars` (a JSON object; a missing name reads null).
    /// `today()`/`now()` read `now` (default: the wall clock) in the team
    /// zone `timeZone` (default UTC).
    public static func eval(_ source: String, vars: JSONValue = .object([:]), now: Date? = nil, timeZone: TimeZone? = nil) -> Outcome {
        let ms = now.map { $0.timeIntervalSince1970 * 1000 }
        let offset = timeZone.map { $0.secondsFromGMT(for: now ?? Date()) / 60 }
        return eval(source, vars: vars, nowMs: ms, offsetMinutes: offset)
    }

    /// The same with the clock as epoch ms and the zone as a fixed UTC offset.
    public static func eval(_ source: String, vars: JSONValue, nowMs: Double?, offsetMinutes: Int?) -> Outcome {
        guard let out = try? evalExpressionJson(source: source, varsJson: vars.json, now: nowMs, offsetMinutes: offsetMinutes.map { Int32($0) }) else {
            return .error("vars are a JSON object")
        }
        let parsed = JSONValue.parse(out)
        if let error = parsed["error"]?.string { return .error(error) }
        return .value(parsed["value"] ?? .null)
    }

    /// Check `source` against declared types (`types`: name → `number`,
    /// `list<entry>`, a kind's name…) and the package's record `kinds`.
    public static func check(_ source: String, types: [String: String] = [:], kinds: JSONValue = .object([:])) -> Check {
        let typesJson = JSONValue.object(types.mapValues { .string($0) }).json
        guard let out = try? checkExpressionJson(source: source, typesJson: typesJson, kindsJson: kinds.json) else {
            return Check(type: "dyn", issues: ["kinds are a JSON object"])
        }
        let parsed = JSONValue.parse(out)
        return Check(type: parsed["type"]?.string ?? "dyn", issues: (parsed["issues"]?.array ?? []).compactMap(\.string))
    }

    /// One function in its A2UI call form (`mul {a, b}`, `expr {source,
    /// vars}`…); nil when the call yields nothing.
    public static func call(_ name: String, args: JSONValue, nowMs: Double? = nil, offsetMinutes: Int? = nil) -> JSONValue? {
        guard let out = try? callExpressionJson(name: name, argsJson: args.json, now: nowMs, offsetMinutes: offsetMinutes.map { Int32($0) }) else { return nil }
        return JSONValue.parse(out)
    }
}
