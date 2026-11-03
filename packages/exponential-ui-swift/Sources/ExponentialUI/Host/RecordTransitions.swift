import Foundation
import ExponentialUICore

/// eApps R6 (VAPP-222): a kind's state machine on one enum field, the core's
/// one rule (`@exponential_app/eui/host` record-transitions, eui-core
/// `host::record_transitions`, `fixtures/record-transitions.json`).
/// `kind` = the kind's definition (`records.<kind>`, `transitions` inside).
public enum RecordTransitions {
    /// Why a write may not move the state (`{code, message}`, code
    /// `BAD_REQUEST` or `FORBIDDEN`), nil = it may. `before` nil = a create;
    /// `after` = the data as it would be stored; `viewer` = `{id, role}`;
    /// `row` = the record's `{authorId, createdBy}`.
    public static func refusal(kind: JSONValue, before: JSONValue?, after: JSONValue, viewer: JSONValue, row: JSONValue? = nil) -> JSONValue? {
        guard let out = try? transitionRefusalJson(kindJson: kind.json, beforeJson: before?.json, afterJson: after.json, viewerJson: viewer.json, rowJson: row?.json) else { return nil }
        return JSONValue.parse(out)
    }

    /// The next states a record may move to (declared order; nil record = the
    /// initial states), only those `viewer` (`{id, role}`) may take when given.
    public static func from(kind: JSONValue, record: JSONValue?, viewer: JSONValue? = nil, row: JSONValue? = nil) -> [String] {
        guard let out = try? transitionsFromJson(kindJson: kind.json, recordJson: record?.json, viewerJson: viewer?.json, rowJson: row?.json) else { return [] }
        return (JSONValue.parse(out).array ?? []).compactMap(\.string)
    }

    /// The issues of a kind's `transitions` (`[{path, message}]` under `<at>/transitions`).
    public static func validate(kind: JSONValue, at: String, kindName: String) -> [JSONValue] {
        guard let out = try? validateRecordTransitionsJson(kindJson: kind.json, at: at, kindName: kindName) else { return [] }
        return JSONValue.parse(out).array ?? []
    }
}
