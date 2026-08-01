import Foundation

// The typed-action-input value rules (EXP-257/EXP-273), lifted out of the
// Start-coding sheet so they are unit-testable and mirror the web helper
// apps/web/src/lib/action-inputs.ts (`buildInputsPayload` /
// `missingRequiredInputs`) one to one.
//
// The run dialog holds ONE flat `[key: String]` map, `""` meaning unset, for
// every input type: `text` is what the user typed, `repo`/`board`/`pr` are ids,
// and `icon` is a curated registry NAME (`AppIcons.pickable`, byte-equal to
// `DomainContract.boardIconValues`) — the very same string a board stores. The
// server re-validates each one (`resolveActionInputs`), so nothing here needs to
// know an id from a glyph name.

public enum ActionInputValues {
    /// Any input whose `type` is outside today's contract list. Such a run is
    /// BLOCKED rather than degraded to text: the desktop that executes it could
    /// not inject the value either.
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

    /// `text` inputs stay within the contract's length cap.
    public static func textsWithinLimit(_ defs: [ActionInputDto], values: [String: String]) -> Bool {
        defs.allSatisfy { def in
            def.type != "text" || (values[def.key] ?? "").count <= DomainContract.actionInputTextMax
        }
    }

    /// The submitted map: `text` trimmed, every other type passed through
    /// verbatim (ids, and the picked icon's registry name), blanks DROPPED so an
    /// untouched optional is absent rather than empty.
    public static func wireValues(
        _ defs: [ActionInputDto],
        values: [String: String]
    ) -> [String: String] {
        var out: [String: String] = [:]
        for def in defs {
            let raw = values[def.key] ?? ""
            let value = def.type == "text" ? trimmed(raw) : raw
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
