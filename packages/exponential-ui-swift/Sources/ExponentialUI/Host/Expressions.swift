import Foundation
import ExponentialUICore

/// VAPP-191 (eApps Q2): expressions, the core's one evaluator
/// (`@exponential_app/eui` `src/cel.ts`, eui-core `cel`,
/// `fixtures/expressions.json`). The string form is a CEL subset (literals,
/// field paths, `+ - * / %`, comparisons, `&& || !`, `?:`, calls, the
/// macros `map/filter/all/exists` one level) over one number type; dates
/// run in the team zone. A surface evaluates the same functions at bind
/// time (`{"call": "expr", …}`); this is the direct entry for a host.
public enum Expression {
    public enum Outcome: Equatable {
        /// The value (`evaluate`) or the type (`check`, spelled as JSON).
        case value(JSONValue)
        case error(String)
    }

    /// Evaluates `expr` over `vars` at `now` in `timeZone` (the team zone;
    /// nil = UTC). `maxSteps` = the step limit (nil = 10 000).
    public static func evaluate(_ expr: String, vars: [String: JSONValue] = [:], now: Date = Date(), timeZone: TimeZone? = nil, maxSteps: UInt32? = nil) -> Outcome {
        evaluate(expr, vars: vars, nowMs: now.timeIntervalSince1970 * 1000, zone: timeZone.map(TimeZoneOffset.init), maxSteps: maxSteps)
    }

    /// `evaluate` with the clock in epoch ms and any `HostZone`.
    public static func evaluate(_ expr: String, vars: [String: JSONValue], nowMs: Double?, zone: HostZone?, maxSteps: UInt32? = nil) -> Outcome {
        guard let out = try? evaluateExpressionJson(expr: expr, varsJson: JSONValue.object(vars).json, nowMs: nowMs, zone: zone, maxSteps: maxSteps) else { return .error("vars is an object") }
        return outcome(JSONValue.parse(out), key: "value")
    }

    /// Type-checks `expr` against `types` (name → type spelled as JSON:
    /// `"number"`, `["date"]`, a row object): `.value(type)` or `.error`.
    public static func check(_ expr: String, types: [String: JSONValue]) -> Outcome {
        guard let out = try? checkExpressionJson(expr: expr, typesJson: JSONValue.object(types).json) else { return .error("types is an object") }
        return outcome(JSONValue.parse(out), key: "type")
    }

    private static func outcome(_ v: JSONValue, key: String) -> Outcome {
        if let error = v["error"]?.string { return .error(error) }
        return .value(v[key] ?? .null)
    }
}

/// A Foundation `TimeZone` as the core's `HostZone` (its offset per instant).
public final class TimeZoneOffset: HostZone, @unchecked Sendable {
    let timeZone: TimeZone
    public init(_ timeZone: TimeZone) { self.timeZone = timeZone }
    public func offsetMinutes(epochMs: Double) -> Int32 {
        Int32(timeZone.secondsFromGMT(for: Date(timeIntervalSince1970: epochMs / 1000)) / 60)
    }
}
