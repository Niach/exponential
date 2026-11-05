import Foundation
import ExponentialUICore

/// VAPP-206 (eApps, spec `docs/spec/apps.md` §4.1): the HOST-PROVIDED binding
/// params (`@exponential_app/eui` `src/host/host-params.ts`, eui-core
/// `host::host_params`, `fixtures/host-router.json` `hostParams`). `${today}`,
/// `${now}` and `${month}` in the team zone, read at each bind; the pure router
/// fills them from the `hostParams` of an `applyTemplate` or `bindDataModel`.
public enum HostParams {
    /// The params at `nowMs` (nil = the wall clock) in `zone` (nil = UTC).
    public static func values(nowMs: Double?, zone: HostZone?) -> JSONValue {
        func read(_ expr: String) -> String {
            if case .value(let v) = Expressions.evaluate(expr, vars: [:], nowMs: nowMs, zone: zone) { return v.string ?? "" }
            return ""
        }
        let today = read("today()")
        return .object(["today": .string(today), "now": .string(read("now()")), "month": .string(String(today.prefix(7)))])
    }

    /// The message with `hostParams` in its `applyTemplate` or `bindDataModel`
    /// body; any other message as it is.
    static func adding(to messageJSON: String, nowMs: Double?, zone: HostZone?) -> String {
        guard messageJSON.contains("applyTemplate") || messageJSON.contains("bindDataModel") else { return messageJSON }
        guard var message = JSONValue.parse(messageJSON).object else { return messageJSON }
        for kind in ["applyTemplate", "bindDataModel"] {
            guard var body = message[kind]?.object else { continue }
            body["hostParams"] = values(nowMs: nowMs, zone: zone)
            message[kind] = .object(body)
            return JSONValue.object(message).json
        }
        return messageJSON
    }
}
