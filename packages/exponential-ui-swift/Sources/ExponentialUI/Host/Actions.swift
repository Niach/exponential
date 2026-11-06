import Foundation
import ExponentialUICore

/// VAPP-194 (eApps E1, spec `docs/spec/apps.md` §4.5): typed effects, the
/// core's one action runner (`@exponential_app/eui` `src/host/actions.ts`,
/// eui-core `host::actions`, `fixtures/actions.json`). Expressions are PURE;
/// a package's `actions: {<name>: {params?, steps}}` are the only place it
/// writes. A Button's `on.press` names one as the function call
/// `actions.<name>`; `ExponentialHost.callFunction` runs it: the PLAN
/// (`run`, one snapshot), every host call's gate, the record writes as ONE
/// `records.batch`, the data sets, then the calls. This is the direct entry
/// for a host (and the tests).
public enum Actions {
    /// What calling a function does (`functionEffect`).
    public enum Effect: String, Sendable, Equatable {
        /// Every expression and bind-time function.
        case pure
        /// `set`, `records.upsert` / `.delete` / `.batch`.
        case write
        /// `openUrl`, `bind`, every `actions.` `harness.` `exponential.`
        /// `tools.` `upstream.` name and every Engine capability's
        /// (`photos.`, `files.`, `print.`, `share.`, …; VAPP-271).
        case host
    }

    /// The function-call prefix that runs a package action.
    public static let callPrefix = "actions."
    /// The host function a host commits an action's record writes through
    /// (args `{writes}`; answers `{ids}`).
    public static let batchFunction = "records.batch"

    /// The action a call name runs (`actions.log` → `log`), else nil.
    public static func actionName(call: String) -> String? {
        call.hasPrefix(callPrefix) ? String(call.dropFirst(callPrefix.count)) : nil
    }

    /// What one run plans, as the core spells it: `{writes, sets, calls}`.
    public struct Plan: Equatable, Sendable {
        /// One data-model write (a JSON pointer, the value).
        public struct Assignment: Equatable, Sendable {
            public let path: String
            public let value: JSONValue
        }

        /// One host function call (after the commit).
        public struct Call: Equatable, Sendable {
            public let name: String
            public let args: Props
        }

        /// The plan as JSON (`{writes, sets, calls}`).
        public let json: JSONValue

        public init(json: JSONValue) { self.json = json }

        /// The record writes (`{op: "upsert", kind, data, id?}` |
        /// `{op: "delete", id}`), passed to `records.batch` unchanged.
        public var writes: [JSONValue] { json["writes"]?.array ?? [] }

        public var sets: [Assignment] {
            (json["sets"]?.array ?? []).map { Assignment(path: $0["path"]?.string ?? "", value: $0["value"] ?? .null) }
        }

        public var calls: [Call] {
            (json["calls"]?.array ?? []).map { Call(name: $0["name"]?.string ?? "", args: $0["args"]?.object ?? [:]) }
        }
    }

    /// VAPP-271: the first `call` step with a `result` the run reached
    /// unanswered: the host gates and runs it (before any commit), then
    /// runs the action again over the SAME snapshot with its answer under
    /// `results[result]`.
    public struct Pending: Equatable, Sendable {
        /// The step's index in the action.
        public let step: Int
        /// The host function to call.
        public let name: String
        public let args: Props
        /// The result name later steps read it as (`results.<result>`).
        public let result: String
    }

    /// The plan, a result call still to answer (VAPP-271), or the one
    /// error that discards the whole action.
    public enum Outcome: Equatable, Sendable {
        case plan(Plan)
        case pending(Pending)
        case error(String)
    }

    /// The batch applied (every row after it, each write's id), or the
    /// first refusal (`write <i>: …`, nothing applied).
    public enum BatchOutcome: Equatable, Sendable {
        case applied(rows: [JSONValue], ids: [String])
        case error(String)
    }

    /// Runs action `name` of `package` (its JSON) with `params` against one
    /// snapshot: `data` (the surface data model), `me` (the member; nil =
    /// unknown), the clock in epoch ms (nil = the wall clock), `zone` (the
    /// team zone; nil = UTC), `maxSteps` (the run's budget; nil = the
    /// contract's 100 000), `results` (VAPP-271: the answers of the result
    /// calls so far, by result name). Pure: nothing is written.
    public static func run(
        _ package: JSONValue, action name: String, params: JSONValue? = nil, data: JSONValue? = nil,
        me: String? = nil, nowMs: Double? = nil, zone: HostZone? = nil, maxSteps: UInt32? = nil,
        results: [String: JSONValue] = [:]
    ) -> Outcome {
        guard let out = try? runPackageActionJson(
            packageJson: package.json, name: name, paramsJson: (params ?? .null).json, dataJson: (data ?? .null).json,
            me: me, nowMs: nowMs, zone: zone, maxSteps: maxSteps,
            resultsJson: results.isEmpty ? nil : JSONValue.object(results).json
        ) else { return .error("the package is JSON") }
        let v = JSONValue.parse(out)
        if let error = v["error"]?.string { return .error(error) }
        if let p = v["pending"] {
            return .pending(Pending(
                step: Int(p["step"]?.number ?? 0), name: p["name"]?.string ?? "", args: p["args"]?.object ?? [:],
                result: p["result"]?.string ?? ""
            ))
        }
        return .plan(Plan(json: v["plan"] ?? .null))
    }

    /// `run` at `now` in a Foundation `TimeZone`.
    public static func run(
        _ package: JSONValue, action name: String, params: JSONValue?, data: JSONValue?, me: String?, now: Date,
        timeZone: TimeZone? = nil, results: [String: JSONValue] = [:]
    ) -> Outcome {
        run(package, action: name, params: params, data: data, me: me, nowMs: now.timeIntervalSince1970 * 1000, zone: timeZone.map(TimeZoneOffset.init), results: results)
    }

    /// Applies `writes` to `rows` (record rows as JSON: `{id, kind, data,
    /// createdBy, createdAt, updatedAt}`) all or nothing, checked against
    /// `records` (the package's kinds): a create is stamped `me` / `now`
    /// and takes the next of `newIds`. What a host's in-memory store does
    /// for `records.batch`.
    public static func applyBatch(
        rows: [JSONValue], writes: [JSONValue], records: JSONValue, me: String?, now: String, newIds: [String]
    ) -> BatchOutcome {
        guard let out = try? applyRecordBatchJson(
            rowsJson: JSONValue.array(rows).json, writesJson: JSONValue.array(writes).json, recordsJson: records.json,
            me: me, now: now, newIdsJson: JSONValue.array(newIds.map(JSONValue.string)).json
        ) else { return .error("the batch is JSON") }
        let v = JSONValue.parse(out)
        if let error = v["error"]?.string { return .error(error) }
        return .applied(rows: v["rows"]?.array ?? [], ids: (v["ids"]?.array ?? []).compactMap(\.string))
    }

    /// What calling `name` does.
    public static func effect(_ name: String) -> Effect {
        Effect(rawValue: functionEffect(name: name)) ?? .pure
    }
}

/// A package action that landed nothing (a false `require`'s message, a
/// refused write, a refused host call): the host's failed-action toast.
public struct ActionFailure: Equatable, Sendable {
    public let surfaceId: String
    /// The action's name (`log` for `actions.log`).
    public let action: String
    public let message: String

    public init(surfaceId: String, action: String, message: String) {
        self.surfaceId = surfaceId
        self.action = action
        self.message = message
    }
}
