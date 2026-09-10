import Foundation

// The typed-action-input value rules (EXP-257/EXP-273), lifted out of the
// launcher so they are unit-testable and mirror the web helper
// apps/web/src/lib/action-inputs.ts (`buildInputsPayload` /
// `missingRequiredInputs`) one to one.
//
// The composer holds ONE flat `[key: String]` map, `""` meaning unset, for
// every input type: `repo`/`board`/`pr` are ids, and `icon` is a curated
// registry NAME (`AppIcons.pickable`, byte-equal to
// `DomainContract.boardIconValues`) — the very same string a board stores. The
// server re-validates each one (`resolveActionInputs`), so nothing here needs to
// know an id from a glyph name.
//
// EXP-825 retired the free-text `text` / `textarea` types: whatever the
// requester types is the start's `prompt` now (`AgentComposerPrompt`), so
// the trim + length rules those types carried are gone with them, and a row
// that still declares one is BLOCKED like any unknown type (the migration
// dropped them server-side; a stale synced row must not run half-filled).

public enum ActionInputValues {
    /// Any input whose `type` is outside today's contract list — `text` and
    /// `textarea` included since EXP-825. Such a run is BLOCKED rather than
    /// degraded: the desktop that executes it could not inject the value
    /// either.
    public static func hasUnsupportedType(_ defs: [ActionInputDto]) -> Bool {
        defs.contains { !DomainContract.actionInputTypeValues.contains($0.type) }
    }

    /// Every required input carries a non-blank value. Optionals — the `icon`
    /// input on the "Create action" builtin among them — never block.
    public static func requiredFilled(_ defs: [ActionInputDto], values: [String: String]) -> Bool {
        defs.allSatisfy { def in
            guard def.isRequired else { return true }
            return !trimmed(values[def.key]).isEmpty
        }
    }

    /// The submitted map: every value passed through verbatim (ids, and the
    /// picked icon's registry name), blanks DROPPED so an untouched optional
    /// is absent rather than empty.
    public static func wireValues(
        _ defs: [ActionInputDto],
        values: [String: String]
    ) -> [String: String] {
        var out: [String: String] = [:]
        for def in defs {
            let value = values[def.key] ?? ""
            if !value.isEmpty {
                out[def.key] = value
            }
        }
        return out
    }

    private static func trimmed(_ value: String?) -> String {
        (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
