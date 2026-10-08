import SwiftUI
import ExponentialUICore

/// The host-owned text of one field (`Input`/`Textarea` `.field`, the
/// `Composer`). The CLIENT owns the string (a UIKit/AppKit view, never a
/// SwiftUI `TextField`: the VAPP-4 burst finding); every edit carries a
/// revision; a 150 ms debounce sends `change`, blur / Enter send `commit`;
/// an echo (a changed `value` prop) is written in only while the field is
/// idle and unfocused.
@MainActor
final class FieldState {
    let id: String
    var text: String
    var revision = 0
    var focused = false
    /// The prop value the field last took (echo detection).
    var external: JSONValue
    var debounce: Task<Void, Never>?
    /// An edit not yet sent as `change`.
    var pendingChange = false
    /// Bumped when the model writes the text (the view reloads it).
    var writeGeneration = 0

    init(id: String, text: String, external: JSONValue) {
        self.id = id
        self.text = text
        self.external = external
    }
}

/// The debounce before a `change` goes out.
public let inputDebounce: Duration = .milliseconds(150)

extension SurfaceModel {
    /// The field state for a text-field node, created from its props.
    func field(_ index: Int) -> FieldState? {
        guard let n = node(index), n.isTextField else { return nil }
        if let f = fields[n.id] { return f }
        let external = (owner(of: index) ?? n).props["value"] ?? .null
        let f = FieldState(id: n.id, text: external.displayText, external: external)
        fields[n.id] = f
        return f
    }

    func pruneFields() {
        for id in Array(fields.keys) where byId[id] == nil {
            fields[id]?.debounce?.cancel()
            fields[id] = nil
        }
    }

    /// The field's text (what the view shows).
    public func fieldText(_ index: Int) -> String { field(index)?.text ?? "" }

    /// A UIKit/AppKit edit: bump the revision, re-arm the debounce.
    public func fieldEdited(_ index: Int, text: String) {
        guard let f = field(index) else { return }
        f.text = text
        f.revision += 1
        f.pendingChange = true
        f.debounce?.cancel()
        f.debounce = Task { @MainActor [weak self, weak f] in
            try? await Task.sleep(for: inputDebounce)
            guard !Task.isCancelled, let self, let f else { return }
            self.flushField(f, index: index, commit: false)
        }
    }

    /// Blur / Enter: send what is pending as a `commit`.
    public func fieldCommitted(_ index: Int) {
        guard let f = field(index) else { return }
        f.debounce?.cancel()
        f.debounce = nil
        flushField(f, index: index, commit: true)
    }

    public func fieldFocused(_ index: Int, _ focused: Bool) {
        guard let f = field(index), let n = node(index) else { return }
        f.focused = focused
        focusedField = focused ? n.id : (focusedField == n.id ? nil : focusedField)
        focus(n.id, focused)
        if !focused { fieldCommitted(index) }
    }

    private func flushField(_ f: FieldState, index: Int, commit: Bool) {
        if !commit && !f.pendingChange { return }
        f.pendingChange = false
        f.debounce = nil
        let name = commit ? "commit" : "change"
        guard let events = try? surface.event(index: UInt32(index), name: name, payloadJson: JSONValue.object(["value": .string(f.text)]).json) else { return }
        // The write-through changed the prop the field mirrors: take it as
        // the new external value so the echo of our own edit never applies.
        f.external = .string(f.text)
        dispatch(events, inputRevision: f.revision)
    }

    /// Composer Enter / send: `submit`, then the field empties.
    public func composerSubmit(_ index: Int) {
        guard let f = field(index), let n = node(index) else { return }
        if n.props.flag("busy") {
            fire(index, "stop")
            return
        }
        let text = f.text
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        f.debounce?.cancel()
        f.pendingChange = false
        f.text = ""
        f.writeGeneration += 1
        guard let events = try? surface.event(index: UInt32(index), name: "submit", payloadJson: JSONValue.object(["value": .string(text)]).json) else { return }
        dispatch(events, inputRevision: f.revision)
    }

    /// After a pass: apply changed `value` props to idle, unfocused fields.
    func echoFields() {
        for n in nodes where n.isTextField {
            guard let f = fields[n.id] else { continue }
            let external = (owner(of: n.index) ?? n).props["value"] ?? .null
            if external == f.external { continue }
            f.external = external
            if f.focused || f.pendingChange { continue }
            let text = external.displayText
            if text != f.text {
                f.text = text
                f.writeGeneration += 1
            }
        }
    }
}
