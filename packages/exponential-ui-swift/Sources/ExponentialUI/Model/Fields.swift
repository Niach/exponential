import SwiftUI
import ExponentialUICore

/// The host-owned text of one field (`Input`/`Textarea` `.field`, the
/// `Composer`, a NumberField / ChipInput `input`, a Select `search`). The
/// CLIENT owns the string (a UIKit/AppKit view, never a SwiftUI
/// `TextField`: the VAPP-4 burst finding); every edit carries a revision;
/// a 150 ms debounce sends `change`, blur sends `commit` (+ `blur`), Enter
/// in a single-line field `submit` (which submits its Form); an echo (a
/// changed prop) is written in only while the field is idle and unfocused.
@MainActor
final class FieldState {
    let id: String
    let kind: FieldKind
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

    init(id: String, kind: FieldKind, text: String, external: JSONValue) {
        self.id = id
        self.kind = kind
        self.text = text
        self.external = external
    }
}

/// What a host field is (gpui `FieldKind`): where the core echoes its text
/// and what Enter does.
enum FieldKind {
    case input, textarea, composer, number, chips, search

    init(_ n: NodeInfo) {
        switch (n.component, n.ownerComponent) {
        case ("Composer", _): self = .composer
        case ("Textarea", _): self = .textarea
        case (_, "NumberField"?): self = .number
        case (_, "ChipInput"?): self = .chips
        case (_, "Select"?): self = .search
        default: self = .input
        }
    }

    /// The prop the core echoes the field's text in (on the field node).
    var echoProp: String {
        switch self {
        case .number, .chips: "text"
        default: "value"
        }
    }
}

/// The debounce before a `change` goes out.
public let inputDebounce: Duration = .milliseconds(150)

extension SurfaceModel {
    /// The value the core shows in a field: the field node's echo prop,
    /// else (Input / Textarea parts) the owner's `value`.
    private func external(of n: NodeInfo, kind: FieldKind) -> JSONValue {
        if let v = n.props[kind.echoProp] { return v }
        if kind == .input || kind == .textarea, let o = owner(of: n.index), o.index != n.index { return o.props["value"] ?? .null }
        return .null
    }

    /// The field state for a text-field node, created from its props.
    func field(_ index: Int) -> FieldState? {
        guard let n = node(index), n.isTextField else { return nil }
        if let f = fields[n.id] { return f }
        let kind = FieldKind(n)
        let ext = external(of: n, kind: kind)
        let f = FieldState(id: n.id, kind: kind, text: ext.displayText, external: ext)
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
            self.flushField(f, index: index, event: nil)
        }
    }

    /// Enter in a field (the text views call this; Shift+Enter / a
    /// Textarea's Enter is a newline and never comes here): an Input,
    /// NumberField or Select search sends `submit` (the core commits and
    /// submits the enclosing Form); a ChipInput adds the chip and empties;
    /// the Composer sends.
    public func fieldCommitted(_ index: Int) {
        guard let f = field(index) else { return }
        switch f.kind {
        case .composer:
            composerSubmit(index)
        case .textarea:
            flushField(f, index: index, event: "commit")
        case .chips:
            flushField(f, index: index, event: "submit")
            clearField(f)
        default:
            flushField(f, index: index, event: "submit")
        }
    }

    /// Send what is pending as a plain `change` (before a stepper press).
    func flushPending(_ index: Int) {
        guard let f = field(index), f.pendingChange else { return }
        flushField(f, index: index, event: nil)
    }

    /// The platform focus of a field moved (the text views call this):
    /// blur sends the pending text as `commit`, then `blur` (`validateOn:
    /// blur` checks).
    public func fieldFocused(_ index: Int, _ focused: Bool) {
        guard let f = field(index), let n = node(index) else { return }
        f.focused = focused
        focusedField = focused ? n.id : (focusedField == n.id ? nil : focusedField)
        focusMoved(id: n.id, focused: focused)
        if !focused {
            f.debounce?.cancel()
            f.debounce = nil
            flushField(f, index: index, event: "commit")
            if let i = byId[n.id] { fire(i, "blur") }
        }
    }

    /// Send the outstanding edit as `change`, then `event` (`commit` /
    /// `submit`) when given.
    private func flushField(_ f: FieldState, index: Int, event: String?) {
        f.debounce?.cancel()
        f.debounce = nil
        let rev = f.revision
        if f.pendingChange {
            f.pendingChange = false
            // The write-through changes the prop the field mirrors: take it
            // as the new external value so the echo of our own edit never
            // applies.
            f.external = .string(f.text)
            if let events = try? surface.event(index: UInt32(index), name: "change", payloadJson: JSONValue.object(["value": .string(f.text)]).json) {
                dispatch(events, inputRevision: rev)
            }
            // A comma ends a chip: the core added it, the field empties.
            if f.kind == .chips && f.text.hasSuffix(",") { clearField(f) }
        }
        if let event, let i = byId[f.id], let events = try? surface.event(index: UInt32(i), name: event, payloadJson: JSONValue.object(["value": .string(f.text)]).json) {
            f.external = .string(f.text)
            dispatch(events, inputRevision: rev)
        }
    }

    /// Empty a field without an echo (a ChipInput after a chip was added).
    private func clearField(_ f: FieldState) {
        f.text = ""
        f.revision += 1
        f.pendingChange = false
        f.external = .string("")
        f.writeGeneration += 1
    }

    /// Composer Enter / send: `submit`, then the field empties (a busy
    /// composer sends `stop`).
    public func composerSubmit(_ index: Int) {
        guard let f = field(index), let n = node(index) else { return }
        if n.props.flag("busy") {
            fire(index, "stop")
            return
        }
        let text = f.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        let rev = f.revision
        f.debounce?.cancel()
        f.debounce = nil
        f.pendingChange = false
        f.text = ""
        f.writeGeneration += 1
        f.external = .string("")
        guard let events = try? surface.event(index: UInt32(index), name: "submit", payloadJson: JSONValue.object(["value": .string(text)]).json) else { return }
        dispatch(events, inputRevision: rev)
    }

    /// After a pass: apply changed echo props to idle, unfocused fields.
    func echoFields() {
        for (id, f) in fields {
            guard let i = byId[id], let n = node(i) else { continue }
            let ext = external(of: n, kind: f.kind)
            if ext == f.external { continue }
            f.external = ext
            if f.focused || f.pendingChange { continue }
            let text = ext.displayText
            if text != f.text {
                f.text = text
                f.writeGeneration += 1
            }
        }
    }
}
