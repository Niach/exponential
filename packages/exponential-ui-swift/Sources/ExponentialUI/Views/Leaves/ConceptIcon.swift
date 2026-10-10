import SwiftUI

/// The glyphs the natives draw for their OWN parts (contract §3 "Built-in
/// glyphs"): the core names a part's glyph from `core.catalog.json`
/// `builtinIcons` (a copy button's `ui-copy`, a drop zone's `upload`, a sort
/// arrow's `ui-chevron-up`, a toast's `ui-error`) and puts the NAME in the
/// part's props; the painter draws that name through the host's icon
/// registry, else its own SF Symbol for every built-in name (gpui
/// `paint/icons.rs` `Glyph::for_concept`), so the chrome works in any host.
enum BuiltinIcons {
    /// `builtinIconSlots` → `builtinIconNames` (generated in
    /// `packages/exponential-ui/generated/ExponentialUICatalog.generated.swift`;
    /// this package does not compile that file yet, so the pairs are mirrored
    /// here and `Round1NativesTests` checks them against it).
    static let slots: [String: String] = [
        "Accordion.trigger": "ui-chevron-down", "Carousel.previous": "ui-chevron-left", "Carousel.next": "ui-chevron-right",
        "Checkbox.check": "ui-check", "ChipInput.remove": "ui-close", "CodeBlock.copy": "ui-copy", "CodeBlock.copied": "ui-check",
        "Composer.attachment": "ui-attach", "Composer.send": "ui-send", "Composer.stop": "ui-stop",
        "DatePicker.trigger": "calendar", "DatePicker.previousMonth": "ui-chevron-left", "DatePicker.nextMonth": "ui-chevron-right",
        "DateRangePicker.trigger": "calendar", "Dialog.close": "ui-close",
        "Menu.check": "ui-check", "Menu.submenuIndicator": "ui-chevron-right",
        "FileUpload.icon": "upload", "FileUpload.file": "ui-file", "FileUpload.remove": "ui-close",
        "Image.fallback": "ui-icon-placeholder", "NumberField.decrement": "ui-minus", "NumberField.increment": "ui-add",
        "Select.trigger": "ui-selector", "Select.search": "search", "Select.check": "ui-check",
        "Table.sortIcon.asc": "ui-chevron-up", "Table.sortIcon.desc": "ui-chevron-down", "TimePicker.trigger": "ui-clock",
        "Toast.close": "ui-close", "Toast.icon.info": "ui-info", "Toast.icon.success": "ui-success",
        "Toast.icon.warning": "ui-warning", "Toast.icon.error": "ui-error", "Unknown.root": "ui-warning",
    ]

    /// The icon name of a built-in slot (`Table.sortIcon.asc`).
    static func name(_ slot: String) -> String { slots[slot] ?? "ui-icon-placeholder" }

    /// The SF Symbol of a concept name (nil = the placeholder circle).
    static func symbol(_ name: String) -> String? {
        switch name {
        case "ui-check", "check": "checkmark"
        case "ui-chevron-down", "chevron-down": "chevron.down"
        case "ui-chevron-up", "chevron-up": "chevron.up"
        case "ui-chevron-left", "chevron-left": "chevron.left"
        case "ui-chevron-right", "chevron-right": "chevron.right"
        case "ui-selector", "chevrons-up-down": "chevron.up.chevron.down"
        case "ui-close", "x": "xmark"
        case "ui-search", "search": "magnifyingglass"
        case "calendar", "ui-calendar": "calendar"
        case "ui-clock", "clock": "clock"
        case "ui-send", "send": "arrow.up"
        case "ui-attach", "paperclip": "paperclip"
        case "ui-stop": "stop.fill"
        case "ui-copy", "copy": "doc.on.doc"
        case "upload", "ui-upload": "square.and.arrow.up"
        case "ui-file", "file": "doc"
        case "ui-add", "plus": "plus"
        case "ui-minus", "minus": "minus"
        case "ui-info", "info": "info.circle"
        case "ui-success", "circle-check": "checkmark.circle"
        case "ui-warning", "triangle-alert": "exclamationmark.triangle"
        case "ui-error", "circle-x": "xmark.circle"
        case "ui-back", "arrow-left": "arrow.left"
        case "ui-arrow-right", "arrow-right": "arrow.right"
        case "ui-star", "star": "star"
        case "ui-image", "image": "photo"
        case "ui-external-link", "external-link": "arrow.up.right"
        case "ui-undo", "undo-2": "arrow.uturn.backward"
        case "ui-menu", "menu": "line.3.horizontal"
        case "ui-play", "play": "play.fill"
        case "pause", "run-pause": "pause.fill"
        case "ui-more", "ellipsis": "ellipsis"
        default: nil
        }
    }

    /// Does a name resolve to a real glyph (not the placeholder)?
    static func isKnown(_ name: String) -> Bool { symbol(name) != nil }
}

/// THE icon view of the painter: a catalog icon NAME `size` square in
/// `color` through the host's registry, else the built-in SF Symbol, else
/// the placeholder ring. Directional glyphs (`rtlMirroredIcons`) mirror
/// under rtl (contract §4); an `Icon` NODE mirrors at its box, outside its
/// own transform, and tells its content (`xuiGlyphMirrored`) so nothing
/// flips twice.
struct ConceptIcon: View {
    let name: String
    let size: CGFloat
    let color: Color
    let model: SurfaceModel
    var weight: Font.Weight = .medium
    @Environment(\.xuiRTL) private var rtl
    @Environment(\.xuiGlyphMirrored) private var mirroredOutside

    var body: some View {
        Group {
            if let view = model.host.icon(name, size: size) {
                view.frame(width: size, height: size).foregroundStyle(color)
            } else if let symbol = BuiltinIcons.symbol(name) {
                Image(systemName: symbol)
                    .font(.system(size: size * 0.8, weight: weight))
                    .foregroundStyle(color)
                    .frame(width: size, height: size)
            } else {
                Circle()
                    .strokeBorder(color.opacity(0.5), lineWidth: 1.5)
                    .frame(width: size * 0.75, height: size * 0.75)
                    .frame(width: size, height: size)
            }
        }
        .mirroredForRTL(!mirroredOutside && RTLGlyphs.mirrors(name, rtl: rtl))
        .accessibilityHidden(true)
    }
}
