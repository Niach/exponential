import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

/// The theme the painter queries: the facade's `Theme` object plus caches
/// (part looks keyed on their query, colours per mode). Painters never read
/// recipes themselves: `part(...)` resolves a sub-part the core does not
/// synthesize exactly like the core resolves a synthetic one.
public final class ThemeHandle: @unchecked Sendable {
    public let theme: Theme
    public let id: String
    private var parts: [String: PartStyle] = [:]
    private var colors: [String: Color?] = [:]
    private var rawColors: [String: String?] = [:]
    private let lock = NSLock()

    public init(_ theme: Theme) {
        self.theme = theme
        self.id = theme.id()
    }

    public static func builtin(_ id: String) -> ThemeHandle? {
        (try? Theme.builtin(id: id)).map(ThemeHandle.init)
    }

    /// Load a theme file over the built-ins.
    public static func load(json: String, parents: [String] = []) throws -> ThemeHandle {
        ThemeHandle(try Theme.load(themeJson: json, parentsJson: parents.isEmpty ? nil : "[\(parents.joined(separator: ","))]"))
    }

    /// `owner/part` resolved for the OWNER's props and `states` in `mode`.
    public func part(_ owner: String, _ part: String, props: Props, states: [String] = [], mode: Mode) -> PartStyle {
        let key = "\(owner)/\(part)|\(mode.rawValue)|\(states.sorted().joined(separator: ","))|\(props.json)"
        lock.lock()
        if let hit = parts[key] {
            lock.unlock()
            return hit
        }
        lock.unlock()
        let resolved: PartStyle
        if let p = try? theme.resolvePart(ownerComponent: owner, part: part, ownerPropsJson: props.json, states: states, mode: mode.rawValue) {
            resolved = PartStyle(style: PaintStyle(p.visual), props: JSONValue.parse(p.styleJson).object ?? [:])
        } else {
            resolved = .empty
        }
        lock.lock()
        parts[key] = resolved
        lock.unlock()
        return resolved
    }

    /// A colour token for the mode (`foreground`, `primary`, `chart1`…).
    public func color(_ name: String, mode: Mode) -> Color? {
        let key = "\(name)|\(mode.rawValue)"
        lock.lock()
        if let hit = colors[key] {
            lock.unlock()
            return hit
        }
        lock.unlock()
        let hex = theme.color(name: name, mode: mode.rawValue)
        let c = hex.flatMap { Color(hex: $0) }
        lock.lock()
        colors[key] = c
        rawColors[key] = hex
        lock.unlock()
        return c
    }

    public func colorHex(_ name: String, mode: Mode) -> String? {
        _ = color(name, mode: mode)
        lock.lock(); defer { lock.unlock() }
        return rawColors["\(name)|\(mode.rawValue)"] ?? nil
    }

    public func spacing(_ name: String) -> CGFloat { CGFloat(theme.spacing(name: name) ?? 0) }
    public func radius(_ name: String) -> CGFloat { CGFloat(theme.radius(name: name) ?? 0) }
    public func control(_ name: String, _ fallback: CGFloat) -> CGFloat { theme.control(name: name).map { CGFloat($0) } ?? fallback }
    public func fontFamily(_ kind: String) -> String? { theme.fontFamily(kind: kind) }
    public var sansFamily: String? { fontFamily("sans") }
    public var monoFamily: String? { fontFamily("mono") }

    /// The surface's default text colour.
    public func ink(mode: Mode) -> Color {
        color("foreground", mode: mode) ?? (mode == .dark ? Color(hex: "#fafafa")! : Color(hex: "#0a0a0a")!)
    }

    /// The tokens the generic primitives paint with under this theme.
    public func primitiveTokens(mode: Mode) -> PrimitiveTokens {
        var t = PrimitiveTokens()
        t.foreground = ink(mode: mode)
        t.mutedForeground = color("mutedForeground", mode: mode) ?? t.mutedForeground
        t.background = color("background", mode: mode) ?? .clear
        t.card = color("card", mode: mode) ?? .clear
        t.muted = color("muted", mode: mode) ?? t.muted
        t.border = color("border", mode: mode) ?? t.border
        t.input = color("input", mode: mode) ?? t.input
        t.primary = color("primary", mode: mode) ?? t.primary
        t.primaryForeground = color("primaryForeground", mode: mode) ?? t.primaryForeground
        t.accent = color("accent", mode: mode) ?? t.accent
        t.destructive = color("destructive", mode: mode) ?? t.destructive
        t.success = color("success", mode: mode) ?? t.success
        t.warning = color("warning", mode: mode) ?? t.warning
        t.info = color("info", mode: mode) ?? t.info
        t.ring = color("ring", mode: mode) ?? t.ring
        t.hairline = control("hairline", 1)
        t.pillHeight = control("pill", 24)
        t.inputHeight = control("input", 36)
        t.rowHeight = control("row", 32)
        t.radiusSm = radius("sm")
        t.radiusMd = radius("md")
        t.radiusLg = radius("lg")
        t.sansFamily = sansFamily
        t.monoFamily = monoFamily
        return t
    }
}

/// `light` | `dark`.
public enum Mode: String, Sendable, CaseIterable {
    case light, dark
}

/// Spacing fallbacks in geometry mode (no theme), the core's table.
enum GeometrySpacing {
    static func value(_ name: String) -> CGFloat {
        switch name {
        case "xxs": 2
        case "xs": 4
        case "sm": 8
        case "md": 12
        case "lg": 16
        case "xl": 24
        default: 0
        }
    }
}
