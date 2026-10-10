import SwiftUI
import ExponentialUICore

/// What a host sets per surface (round-1 contract §4–6, the core's
/// `SurfaceSettings`): locale, built-in string overrides, `system` mode,
/// density, contrast, font scale, safe-area insets, pointer and motion,
/// today. The `nil` / `system` values FOLLOW the platform live:
/// `ExponentialSurface` feeds the SwiftUI environment (colour scheme,
/// `colorSchemeContrast`, Dynamic Type, Reduce Motion, the safe area, an
/// iPad pointer) into the model, which resolves them here and hands the
/// result to the core (`Surface.setSettings`).
public struct SurfaceSettings: Sendable, Equatable {
    public enum ModeSetting: String, Sendable, CaseIterable { case light, dark, system }
    public enum Density: String, Sendable, CaseIterable { case compact, `default`, comfortable }
    public enum Contrast: String, Sendable, CaseIterable { case normal, high, system }

    /// BCP 47 (`en-US` default): week start, text direction, formatters.
    public var locale: String
    /// Built-in string overrides by id (`catalog/strings.json`).
    public var strings: [String: String]
    public var mode: ModeSetting
    public var density: Density
    public var contrast: Contrast
    /// The type scale; nil = Dynamic Type (`dynamicTypeSize`).
    public var fontScale: Double?
    /// Safe-area insets layers keep clear of; nil = the surface view's safe area.
    public var insets: EdgeInsets?
    /// A hover-capable pointer; nil = the platform's (macOS yes; iPadOS once
    /// a pointer hovers).
    public var hover: Bool?
    /// nil = the platform's Reduce Motion.
    public var reducedMotion: Bool?
    /// Today as `yyyy-mm-dd` (calendars open on it and mark it); nil = the
    /// device's current date.
    public var today: String?
    /// How long a hover card / tooltip stays open after the pointer left
    /// (the core's `HOVER_CLOSE_MS` for natives).
    public var hoverCloseMs: Int
    /// The IANA zone instants format in (round 2 §3); nil = the device's.
    /// The surface's Formatter (`FoundationFormatter`) is built in it.
    public var timeZone: String?

    public init(locale: String = "en-US", strings: [String: String] = [:], mode: ModeSetting = .system, density: Density = .default, contrast: Contrast = .system, fontScale: Double? = nil, insets: EdgeInsets? = nil, hover: Bool? = nil, reducedMotion: Bool? = nil, today: String? = nil, hoverCloseMs: Int = 150, timeZone: String? = nil) {
        self.locale = locale
        self.strings = strings
        self.mode = mode
        self.density = density
        self.contrast = contrast
        self.fontScale = fontScale
        self.insets = insets
        self.hover = hover
        self.reducedMotion = reducedMotion
        self.today = today
        self.hoverCloseMs = hoverCloseMs
        self.timeZone = timeZone
    }
}

/// What the platform reports (fed by `ExponentialSurface` from the SwiftUI
/// environment; tests set it directly).
public struct PlatformTraits: Sendable, Equatable {
    public var prefersDark: Bool
    public var highContrast: Bool
    /// Dynamic Type as a multiplier of the default (`large` = 1).
    public var fontScale: Double
    public var reduceMotion: Bool
    public var safeArea: EdgeInsets
    /// A pointer that hovers (macOS; an iPad trackpad / mouse once seen).
    public var pointerHover: Bool

    public init(prefersDark: Bool = false, highContrast: Bool = false, fontScale: Double = 1, reduceMotion: Bool = false, safeArea: EdgeInsets = EdgeInsets(), pointerHover: Bool = PlatformTraits.defaultPointerHover) {
        self.prefersDark = prefersDark
        self.highContrast = highContrast
        self.fontScale = fontScale
        self.reduceMotion = reduceMotion
        self.safeArea = safeArea
        self.pointerHover = pointerHover
    }

    #if os(macOS)
    public static let defaultPointerHover = true
    #else
    public static let defaultPointerHover = false
    #endif

    /// Dynamic Type → the font scale (the body size at each step ÷ 17 pt).
    public static func fontScale(_ size: DynamicTypeSize) -> Double {
        switch size {
        case .xSmall: 14.0 / 17
        case .small: 15.0 / 17
        case .medium: 16.0 / 17
        case .large: 1
        case .xLarge: 19.0 / 17
        case .xxLarge: 21.0 / 17
        case .xxxLarge: 23.0 / 17
        case .accessibility1: 28.0 / 17
        case .accessibility2: 33.0 / 17
        case .accessibility3: 40.0 / 17
        case .accessibility4: 47.0 / 17
        case .accessibility5: 53.0 / 17
        @unknown default: 1
        }
    }
}

/// Today in the device's calendar as `yyyy-mm-dd`.
func localToday(_ now: Date = Date()) -> String {
    let c = Calendar(identifier: .gregorian).dateComponents(in: .current, from: now)
    return String(format: "%04d-%02d-%02d", c.year ?? 2026, c.month ?? 1, c.day ?? 1)
}

extension SurfaceSettings {
    /// The core's settings for these host settings under `platform`.
    func ffi(_ platform: PlatformTraits) -> FfiSettings {
        let strings = JSONValue.object(self.strings.mapValues { .string($0) }).json
        let insets = self.insets ?? platform.safeArea
        return FfiSettings(
            locale: locale.isEmpty ? "en-US" : locale,
            stringsJson: self.strings.isEmpty ? "" : strings,
            mode: mode.rawValue,
            systemDark: platform.prefersDark,
            density: density.rawValue,
            contrast: contrast.rawValue,
            systemHighContrast: platform.highContrast,
            fontScale: Float(fontScale ?? platform.fontScale),
            hover: hover ?? platform.pointerHover,
            reducedMotion: reducedMotion ?? platform.reduceMotion,
            insetTop: Float(insets.top),
            insetRight: Float(insets.trailing),
            insetBottom: Float(insets.bottom),
            insetLeft: Float(insets.leading),
            today: today ?? localToday(),
            hoverCloseMs: UInt32(max(0, hoverCloseMs)),
            timeZone: timeZone ?? TimeZone.current.identifier
        )
    }
}

extension SurfaceModel {
    /// Replace the host settings (the `system` / nil parts keep following
    /// the platform).
    public func setSettings(_ settings: SurfaceSettings) {
        guard settings != self.settings else { return }
        self.settings = settings
        applySettings()
    }

    /// What the platform reports changed (`ExponentialSurface` calls this
    /// from the environment; tests and embedders without the view may too).
    public func setPlatform(_ traits: PlatformTraits) {
        guard traits != platform else { return }
        platform = traits
        applySettings()
    }

    public func setLocale(_ locale: String) { var s = settings; s.locale = locale; setSettings(s) }
    public func setStrings(_ strings: [String: String]) { var s = settings; s.strings = strings; setSettings(s) }
    public func setModeSetting(_ mode: SurfaceSettings.ModeSetting) { var s = settings; s.mode = mode; setSettings(s) }
    public func setDensity(_ density: SurfaceSettings.Density) { var s = settings; s.density = density; setSettings(s) }
    public func setContrast(_ contrast: SurfaceSettings.Contrast) { var s = settings; s.contrast = contrast; setSettings(s) }
    /// nil = follow Dynamic Type.
    public func setFontScale(_ scale: Double?) { var s = settings; s.fontScale = scale; setSettings(s) }
    public func setInsets(_ insets: EdgeInsets?) { var s = settings; s.insets = insets; setSettings(s) }
    public func setHoverCapable(_ hover: Bool?) { var s = settings; s.hover = hover; setSettings(s) }
    public func setReducedMotion(_ reduced: Bool?) { var s = settings; s.reducedMotion = reduced; setSettings(s) }
    public func setToday(_ today: String?) { var s = settings; s.today = today; setSettings(s) }

    /// A pointer hovered (an iPad trackpad / mouse): hover capability on.
    func pointerSeen() {
        if !platform.pointerHover && settings.hover == nil {
            var p = platform
            p.pointerHover = true
            setPlatform(p)
        }
    }

    /// The resolved settings into the core; the derived state (mode, the
    /// paint theme, the measurer generation) follows.
    func applySettings() {
        let ffi = settings.ffi(platform)
        let before = appliedSettings
        guard ffi != before else { return }
        do {
            try surface.setSettings(settings: ffi)
        } catch {
            return
        }
        appliedSettings = ffi
        // ONE Formatter per surface (round 2 §3): Foundation in the
        // surface locale and zone; the core formats every bound
        // `formatNumber` / `formatDate` / … call and its natives through it.
        if before?.locale != ffi.locale || before?.timeZone != ffi.timeZone {
            let f = FoundationFormatter(locale: ffi.locale, timeZone: ffi.timeZone)
            formatter = f
            surface.setFormatter(formatter: f)
        }
        // Font scale: every leaf measures again (the core rescales the type).
        if before == nil || before?.fontScale != ffi.fontScale || before?.locale != ffi.locale || before?.stringsJson != ffi.stringsJson {
            measureGeneration += 1
        }
        let resolved = Mode(rawValue: surface.mode()) ?? mode
        if resolved != mode { mode = resolved }
        refreshPaintTheme()
        invalidate(structure: true)
    }

    /// The theme the painters query: the core's EFFECTIVE theme (density
    /// scaled controls and spacing, a high-contrast overlay) when either is
    /// in force, else the host's theme as given.
    func refreshPaintTheme() {
        guard let base = baseTheme else {
            theme = nil
            primitiveTokens = .system
            return
        }
        let s = appliedSettings
        let contrastOn = s.map { $0.contrast == "high" || ($0.contrast == "system" && $0.systemHighContrast) } ?? false
        let dense = s.map { $0.density != "default" } ?? false
        var next = base
        if contrastOn || dense {
            // The core's effective `Theme` object (round 2), else the JSON
            // round trip an older core needed.
            if let t = surface.effectiveTheme() {
                next = ThemeHandle(t)
            } else if let json = surface.effectiveThemeJson(), let t = try? ThemeHandle.load(json: Self.loadableTheme(json)) {
                next = t
            }
        }
        if next !== theme { theme = next }
        primitiveTokens = next.primitiveTokens(mode: mode)
    }

    /// A RESOLVED theme (`effectiveThemeJson`) as a theme file: the
    /// resolution's `chain` (the ids it extended) is not a theme key. Text
    /// surgery keeps every other key in its order (recipes match in order).
    static func loadableTheme(_ json: String) -> String {
        var out = json
        if let r = json.range(of: #""chain":\[[^\]]*\],?"#, options: .regularExpression) {
            out.removeSubrange(r)
            out = out.replacingOccurrences(of: ",}", with: "}")
        }
        // Round 4: a theme file names its format (a resolved theme does not).
        if !out.contains("\"$schema\""), out.hasPrefix("{") {
            out.insert(contentsOf: #""$schema":"https://ui.exponential.at/schemas/theme/v1.json","#, at: out.index(after: out.startIndex))
        }
        return out
    }

    /// `ltr` | `rtl`: the surface direction the core resolved (the locale's,
    /// unless the author set `direction` on the root).
    public var isRTL: Bool { direction == "rtl" }

    /// Reduced motion in force (the host's setting, else the platform's
    /// Reduce Motion): no transitions, no layer motion.
    public var reducedMotion: Bool { settings.reducedMotion ?? platform.reduceMotion }

    /// The SwiftUI layout direction of the surface's CONTENT (frames are
    /// physical; painters use this for text alignment and mirrored glyphs).
    public var layoutDirection: LayoutDirection { isRTL ? .rightToLeft : .leftToRight }
}
