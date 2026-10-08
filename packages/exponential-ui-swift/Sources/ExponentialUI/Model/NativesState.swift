import SwiftUI
import UniformTypeIdentifiers
import ExponentialUICore

/// The round-1 natives' transient painter state for ONE surface model (the
/// core keeps every value: chips, files, sort, selection, the open calendar
/// month; this is only what the PLATFORM owns): the host-owned inline fields
/// (a NumberField's / ChipInput's `input`, a searchable Select's `search`),
/// the surface string table, the files the platform handed a FileUpload.
/// Held beside the model (`SurfaceModel.natives`), never inside it, so the
/// natives stay one lane's code (VAPP-100 L3).
@MainActor
final class NativesState {
    var inlineFields: [String: InlineField] = [:]
    /// The surface's built-in string table (`stringTableJson` with the
    /// settings' overrides) and the overrides it was built from.
    var strings: [String: String] = [:]
    var stringsKey: String?
    /// FileUpload owner id → the platform URLs of its attached files, in
    /// the order the core lists them (the bytes a host uploads).
    var fileURLs: [String: [URL]] = [:]
    /// Number formatters per (locale, precision).
    var numberFormatters: [String: NumberFormatter] = [:]
    var dateFormatters: [String: DateFormatter] = [:]
    /// The tombstone slots of the last pass read (`isLive`).
    var removed: Set<Int> = []
    var removedPass = -1
}

/// One host-owned one-line field inside a native (gpui `view/input.rs`
/// `FieldKind::Number | Chips | Search`). The CLIENT owns the string (an
/// owned UIKit/AppKit field, the VAPP-4 rule); every edit bumps the
/// revision; a 150 ms debounce sends `change {value}` (a Select's `search`
/// fires from it), Enter sends `submit`, blur `commit`; the core's echo (a
/// re-formatted number, an emptied chip query) is written in only while the
/// field is idle and unfocused.
@MainActor
final class InlineField {
    enum Kind: Equatable {
        case number
        case chips
        case search
    }

    let id: String
    let kind: Kind
    var text: String
    var revision = 0
    var flushed = 0
    var focused = false
    /// The echo source the field last took.
    var external: String
    var debounce: Task<Void, Never>?
    /// Bumped when the model writes the text (the view reloads it).
    var writeGeneration = 0

    init(id: String, kind: Kind, text: String) {
        self.id = id
        self.kind = kind
        self.text = text
        external = text
    }

    var idle: Bool { debounce == nil && flushed == revision }
}

@MainActor private let nativesTable = NSMapTable<SurfaceModel, NativesState>.weakToStrongObjects()

extension SurfaceModel {
    /// This model's natives state (created on first use, released with it).
    var natives: NativesState {
        if let s = nativesTable.object(forKey: self) { return s }
        let s = NativesState()
        nativesTable.setObject(s, forKey: self)
        return s
    }

    // MARK: - live nodes

    /// Is node `index` live (not a TOMBSTONE the core keeps for a removed
    /// slot, `FfiNode.removed`)? `NodeInfo` does not carry the flag yet, so
    /// the removed set is read once per pass.
    func isLive(_ index: Int) -> Bool {
        let s = natives
        if s.removedPass != passCount {
            s.removed = Set(surface.nodes().filter(\.removed).map { Int($0.index) })
            s.removedPass = passCount
        }
        return !s.removed.contains(index)
    }

    /// The live parts of a native `owner` with this part name, in order.
    func liveParts(_ owner: String, _ part: String) -> [NodeInfo] {
        nodes.filter { $0.owner == owner && $0.part == part && isLive($0.index) }
    }

    // MARK: - settings the natives read

    /// The surface locale (BCP 47, the core's settings; `en-US` default).
    var surfaceLocale: String {
        let l = surface.settings().locale
        return l.isEmpty ? "en-US" : l
    }

    /// A built-in string (`catalog/strings.json` with the host's overrides)
    /// with `{name}` placeholders filled through the core's `formatString`.
    func builtinString(_ id: String, _ params: [String: JSONValue] = [:]) -> String {
        let s = natives
        let overrides = surface.settings().stringsJson
        if s.stringsKey != overrides || s.strings.isEmpty {
            let table = (try? stringTableJson(overridesJson: overrides.isEmpty ? nil : overrides)).map(JSONValue.parse)?.object ?? [:]
            s.strings = table.compactMapValues(\.string)
            s.stringsKey = overrides
        }
        let template = s.strings[id] ?? id
        if params.isEmpty { return template }
        return (try? formatString(template: template, paramsJson: JSONValue.object(params).json)) ?? template
    }

    // MARK: - locale formatting (contract §4: the platform's ICU in the SURFACE locale)

    /// `n` with `precision` fraction digits (nil = up to 2), grouped, in the
    /// surface locale: NumberField and Table number cells.
    func formatNumber(_ n: Double, precision: Int?) -> String {
        let locale = surfaceLocale
        let key = "\(locale)|\(precision.map(String.init) ?? "auto")"
        let f: NumberFormatter
        if let hit = natives.numberFormatters[key] {
            f = hit
        } else {
            f = NumberFormatter()
            f.locale = Locale(identifier: locale)
            f.numberStyle = .decimal
            f.usesGroupingSeparator = true
            f.roundingMode = .halfUp
            f.minimumFractionDigits = precision ?? 0
            f.maximumFractionDigits = precision ?? 2
            natives.numberFormatters[key] = f
        }
        return f.string(from: NSNumber(value: n)) ?? String(n)
    }

    /// Typed number text → a number: the surface locale's notation first
    /// (`1.234,5` in `de`), then the plain JS notation.
    func parseNumber(_ text: String) -> Double? {
        let t = text.trimmingCharacters(in: .whitespaces)
        if t.isEmpty { return nil }
        let f = NumberFormatter()
        f.locale = Locale(identifier: surfaceLocale)
        f.numberStyle = .decimal
        f.isLenient = true
        if let n = f.number(from: t) { return n.doubleValue }
        return Double(t)
    }

    /// An ISO date (`yyyy-mm-dd`, a date-time) as the surface locale's
    /// medium date (`Oct 7, 2026`, `7. Okt. 2026`), UTC so it never shifts.
    func formatDate(_ iso: String) -> String? {
        guard let date = DateModel.date(fromISO: String(iso.prefix(10))) else { return nil }
        let locale = surfaceLocale
        let f: DateFormatter
        if let hit = natives.dateFormatters[locale] {
            f = hit
        } else {
            f = DateFormatter()
            f.locale = Locale(identifier: locale)
            f.timeZone = TimeZone(identifier: "UTC")
            f.dateStyle = .medium
            f.timeStyle = .none
            natives.dateFormatters[locale] = f
        }
        return f.string(from: date)
    }

    /// The week's first day for the surface locale (0 = Sunday): the core's
    /// CLDR table (`weekStart`), never `Calendar.current`.
    var surfaceWeekStart: Int { Int(weekStart(locale: surfaceLocale)) }

    // MARK: - NumberField

    /// A NumberField's precision: `precision`, else the step's decimals
    /// (the core's `number_step_precision`).
    static func numberPrecision(_ props: Props) -> Int {
        if let p = props.num("precision") { return Int(max(0, min(100, p.rounded(.down)))) }
        let step = props.num("step").flatMap { $0 != 0 && $0.isFinite ? $0 : nil } ?? 1
        let s = String(step)
        guard let dot = s.firstIndex(of: "."), !s.contains("e") else { return 0 }
        let decimals = s[s.index(after: dot)...]
        return decimals == "0" ? 0 : decimals.count
    }

    /// The text a NumberField shows: its `value` in the surface locale at
    /// its precision ("" when empty).
    func numberFieldText(owner: Props) -> String {
        guard let v = owner["value"], !v.isNull, v.displayText != "" else { return "" }
        guard let n = v.number ?? Double(v.displayText) else { return v.displayText }
        return formatNumber(n, precision: Self.numberPrecision(owner))
    }

    /// Step a NumberField `times` (an arrow key, an accessibility adjust):
    /// presses of the core's own stepper parts, so clamping and the events
    /// are the core's.
    func numberFieldStep(ownerId: String, up: Bool, times: Int = 1) {
        guard let i = index(of: "\(ownerId).\(up ? "increment" : "decrement")") else { return }
        if let field = index(of: "\(ownerId).input") { inlineFlush(field, commit: nil) }
        for _ in 0..<max(times, 1) { fire(i, "press") }
    }

    // MARK: - inline fields

    /// The inline-field state of a node (nil = not an inline field).
    func inlineField(_ index: Int) -> InlineField? {
        guard let n = node(index), SurfaceMeasurer.isInlineField(n.recipeComponent, n.part) else { return nil }
        let kind: InlineField.Kind = switch n.recipeComponent {
        case "NumberField": .number
        case "ChipInput": .chips
        default: .search
        }
        let external = inlineExternal(n, kind: kind)
        if let f = natives.inlineFields[n.id] {
            // The echo: a changed prop lands only in an idle, unfocused field.
            if f.external != external {
                f.external = external
                if !f.focused, f.idle, f.text != external {
                    f.text = external
                    f.writeGeneration += 1
                }
            }
            return f
        }
        let f = InlineField(id: n.id, kind: kind, text: external)
        natives.inlineFields[n.id] = f
        return f
    }

    /// What the core echoes into an inline field: a NumberField's value in
    /// the surface locale, a ChipInput's query (`text`), a search's `value`.
    private func inlineExternal(_ n: NodeInfo, kind: InlineField.Kind) -> String {
        switch kind {
        case .number: numberFieldText(owner: ownerProps(n.index))
        case .chips: n.props.str("text")
        case .search: n.props["value"]?.displayText ?? ""
        }
    }

    /// The field's text (what the owned view shows).
    func inlineFieldText(_ index: Int) -> String { inlineField(index)?.text ?? "" }

    /// A platform edit: bump the revision, re-arm the 150 ms debounce.
    func inlineFieldEdited(_ index: Int, text: String) {
        guard let f = inlineField(index) else { return }
        f.text = text
        f.revision += 1
        f.debounce?.cancel()
        f.debounce = Task { @MainActor [weak self] in
            try? await Task.sleep(for: inputDebounce)
            guard !Task.isCancelled, let self else { return }
            self.inlineFlush(index, commit: nil)
        }
    }

    func inlineFieldFocused(_ index: Int, _ focused: Bool) {
        guard let f = inlineField(index), let n = node(index) else { return }
        f.focused = focused
        focus(n.id, focused)
        if !focused {
            inlineFlush(index, commit: "commit")
            fire(index, "blur")
        }
    }

    /// Enter: `submit` (a ChipInput adds the chip and empties the field).
    func inlineFieldReturn(_ index: Int) {
        guard let f = inlineField(index) else { return }
        inlineFlush(index, commit: "submit")
        if f.kind == .chips { clearInlineField(f) }
    }

    /// Backspace in an EMPTY ChipInput field removes the last chip (the
    /// press of the last chip's `remove`). Returns whether it did.
    @discardableResult
    func inlineFieldBackspace(_ index: Int) -> Bool {
        guard let f = inlineField(index), f.kind == .chips, f.text.isEmpty, let owner = owner(of: index) else { return false }
        guard let last = liveParts(owner.id, "remove").last else { return false }
        fire(last.index, "press")
        return true
    }

    /// Up / Down (Page Up / Down = 10 steps) in a NumberField field.
    @discardableResult
    func inlineFieldArrow(_ index: Int, up: Bool, page: Bool) -> Bool {
        guard let f = inlineField(index), f.kind == .number, let owner = owner(of: index) else { return false }
        numberFieldStep(ownerId: owner.id, up: up, times: page ? 10 : 1)
        return true
    }

    /// Send the outstanding edit (and a `commit` / `submit`).
    func inlineFlush(_ index: Int, commit: String?) {
        guard let f = inlineField(index) else { return }
        f.debounce?.cancel()
        f.debounce = nil
        let needsChange = f.flushed < f.revision
        f.flushed = f.revision
        let value = inlineValue(f)
        let revision = f.revision
        if needsChange {
            if let events = try? surface.event(index: UInt32(index), name: "change", payloadJson: JSONValue.object(["value": value]).json) {
                dispatch(events, inputRevision: revision)
            }
            // A comma ends a chip: the core added it, the field empties.
            if f.kind == .chips, f.text.hasSuffix(",") { clearInlineField(f) }
        }
        if let commit, let events = try? surface.event(index: UInt32(index), name: commit, payloadJson: JSONValue.object(["value": value]).json) {
            dispatch(events, inputRevision: revision)
        }
    }

    /// The payload value of a field: a NumberField sends a NUMBER (null
    /// when empty), parsed in the surface locale.
    private func inlineValue(_ f: InlineField) -> JSONValue {
        guard f.kind == .number else { return .string(f.text) }
        if f.text.trimmingCharacters(in: .whitespaces).isEmpty { return .null }
        return parseNumber(f.text).map(JSONValue.number) ?? .string(f.text)
    }

    /// Empty a field without waiting for the echo (a chip was added).
    private func clearInlineField(_ f: InlineField) {
        f.text = ""
        f.revision += 1
        f.flushed = f.revision
        f.external = ""
        f.writeGeneration += 1
    }

    // MARK: - FileUpload

    /// Files picked for or dropped on a FileUpload: `upload {files: [{name,
    /// size, type}]}` on its drop zone (the core refuses too-large ones,
    /// attaches the rest and fires `upload`); the URLs stay here for the
    /// host's upload (`fileUploadURLs`).
    func fileUploadReceived(ownerId: String, urls: [URL]) {
        guard !urls.isEmpty, let zone = index(of: "\(ownerId).dropzone") ?? index(of: ownerId) else { return }
        natives.fileURLs[ownerId, default: []].append(contentsOf: urls)
        fire(zone, "upload", payload: .object(["files": .array(urls.map(Self.fileDescriptor))]))
    }

    /// The platform URLs a FileUpload received (the bytes a host uploads).
    public func fileUploadURLs(_ ownerId: String) -> [URL] { natives.fileURLs[ownerId] ?? [] }

    /// `{name, size, type}` of a file URL (type = its UTType's MIME type).
    static func fileDescriptor(_ url: URL) -> JSONValue {
        let values = try? url.resourceValues(forKeys: [.fileSizeKey, .contentTypeKey])
        let size = Double(values?.fileSize ?? 0)
        let type = values?.contentType?.preferredMIMEType ?? ""
        return .object(["name": .string(url.lastPathComponent), "size": .number(size), "type": .string(type)])
    }

    // MARK: - ContextMenu

    /// Open the ContextMenu at or above node `index` at a SURFACE point (a
    /// right click, a long press): the core places its menu layer there.
    func openContextMenu(_ index: Int, at point: CGPoint?) {
        let payload: JSONValue? = point.map { .object(["x": .number(Double($0.x)), "y": .number(Double($0.y))]) }
        fire(index, "contextmenu", payload: payload)
    }

    /// A native menu entry chosen (the platform menu, not the painted
    /// layer): `select {value}` (+ `{checked}` = the NEW state of a
    /// checkbox entry) on the owner, as the core's `menu_select` fires it.
    func nativeMenuSelect(ownerIndex: Int, item: JSONValue) {
        var payload: [String: JSONValue] = ["value": item["value"] ?? .null]
        if item["kind"]?.string == "checkbox" { payload["checked"] = .bool(!(item["checked"]?.bool ?? false)) }
        fire(ownerIndex, "select", payload: .object(payload))
    }
}
