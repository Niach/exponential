import SwiftUI
import ExponentialUI
import ExponentialUICore

struct LaunchOptions {
    var theme = "exponential"
    var mode = "dark"
    var rtl = false
    var width: CGFloat? = nil
    var shot: String? = nil
    /// `-a11yDump <path>`: write the UIAccessibility walk (one label per line).
    var a11yDump: String? = nil
    /// `-dump <path>`: write the whole surface (every pixel, not the viewport) as a PNG.
    var dump: String? = nil
    /// `-frames <path>` (VAPP-127): two seconds after the first layout, every visible node's frame
    /// (points, surface coordinates) as JSON `{surface, width, frames: [{id, x, y, w, h}]}`.
    var frames: String? = nil

    static func parse(_ args: [String] = CommandLine.arguments) -> LaunchOptions {
        var o = LaunchOptions()
        var i = 0
        func value() -> String? { i + 1 < args.count ? args[i + 1] : nil }
        while i < args.count {
            switch args[i] {
            case "-theme": o.theme = value() ?? o.theme; i += 1
            case "-mode": o.mode = value() ?? o.mode; i += 1
            case "-rtl": o.rtl = true
            case "-width": o.width = value().flatMap { Double($0) }.map { CGFloat($0) }; i += 1
            case "-shot": o.shot = value(); i += 1
            case "-a11yDump": o.a11yDump = value(); i += 1
            case "-dump": o.dump = value(); i += 1
            case "-frames": o.frames = value(); i += 1
            default: break
            }
            i += 1
        }
        return o
    }
}

/// The catalog icon concepts the kitchen sink names, as SF Symbols. A real
/// host hands the renderer its own icon registry (the Exponential app's
/// Lucide set); the example keeps the SDK free of one.
let symbolIcons: [String: String] = [
    "nav-boards": "square.grid.2x2", "nav-inbox": "tray", "pr-open": "checkmark.rectangle", "nav-search": "magnifyingglass",
    "ui-folder": "folder", "ui-archive": "archivebox", "ui-send": "paperplane", "ui-issue": "circle.dashed", "ui-pin": "pin",
    "ui-success": "checkmark.circle", "ui-assignee": "person", "ui-device": "laptopcomputer", "settings-statuses": "circle.lefthalf.filled",
    "editor-bold": "bold", "editor-italic": "italic", "ui-warning": "exclamationmark.triangle", "ui-info": "info.circle",
    "ui-error": "xmark.octagon", "ui-close": "xmark", "ui-plus": "plus", "ui-more": "ellipsis", "ui-settings": "gearshape",
    "ui-chevron-right": "chevron.right", "ui-chevron-left": "chevron.left", "ui-chevron-down": "chevron.down", "ui-chevron-up": "chevron.up",
    "ui-selector": "chevron.up.chevron.down", "ui-check": "checkmark", "ui-search": "magnifyingglass", "ui-calendar": "calendar",
    "ui-image": "photo", "ui-link": "link", "ui-external": "arrow.up.right", "ui-copy": "doc.on.doc", "ui-trash": "trash",
    "nav-issues": "list.bullet", "nav-settings": "gearshape", "ui-edit": "pencil", "ui-properties": "slider.horizontal.3",
    // The specimens' bare Lucide names (Segmented `variant · bar · icons`, VAPP-163).
    "message-circle": "message", "users": "person.2", "settings": "gearshape",
]

/// The non-Lucide marks (icons.json `custom` + `imported`) no SF Symbol draws
/// the same: their 24-unit geometry from `custom-icons.generated.ts`, filled.
let markIcons: [String: String] = [
    "os-apple": "M18.859 9.138c-0.105 0.081 -1.948 1.118 -1.948 3.428c0 2.672 2.345 3.618 2.415 3.639c-0.011 0.056 -0.373 1.294 -1.238 2.556c-0.77 1.107 -1.575 2.215 -2.798 2.215s-1.54 -0.71 -2.953 -0.71c-1.378 0 -1.867 0.735 -2.985 0.735s-1.902 -1.027 -2.798 -2.285c-1.041 -1.48 -1.881 -3.779 -1.881 -5.959c0 -3.498 2.275 -5.354 4.514 -5.354c1.188 0 2.183 0.78 2.929 0.78c0.71 0 1.821 -0.83 3.175 -0.83c0.513 0 2.355 0.046 3.568 1.786m-4.212 -3.27c0.559 -0.664 0.956 -1.586 0.956 -2.507c0 -0.127 -0.011 -0.257 -0.035 -0.362c-0.911 0.035 -1.993 0.608 -2.647 1.364c-0.513 0.584 -0.991 1.505 -0.991 2.44c0 0.141 0.025 0.281 0.035 0.327c0.056 0.011 0.151 0.025 0.246 0.025c0.816 -0.004 1.842 -0.548 2.436 -1.287",
    "os-windows": "M3 3h8.532v8.529H3Zm9.468 0H21v8.529H12.468ZM3 12.468h8.532V21H3Zm9.468 0H21V21H12.468",
    "os-linux": "M19.471 17.752c-0.12 -0.109 -0.299 -0.19 -0.418 -0.25c-0.179 -0.07 -0.299 -0.162 -0.418 -0.267c-0.179 -0.109 -0.239 -0.232 -0.299 -0.362l-0.06 -0.439c-0.06 -0.281 -0.12 -0.461 -0.179 -0.541s-0.12 -0.13 -0.239 -0.13c0.12 -0.387 0.179 -0.787 0.179 -1.199c0 -0.83 -0.179 -1.603 -0.478 -2.201c-0.239 -0.654 -0.654 -1.311 -1.192 -1.905c-0.239 -0.359 -0.478 -0.654 -0.654 -0.953c-0.179 -0.239 -0.299 -0.478 -0.418 -0.714c-0.12 -0.179 -0.179 -0.418 -0.239 -0.654c-0.06 -0.299 -0.06 -0.538 -0.12 -0.714c0 -0.179 -0.06 -0.418 -0.06 -0.773c-0.06 -0.418 -0.06 -0.714 -0.12 -1.012c-0.06 -0.478 -0.179 -0.833 -0.299 -1.192c-0.179 -0.299 -0.359 -0.598 -0.598 -0.773c-0.179 -0.179 -0.418 -0.299 -0.654 -0.418c-0.299 -0.12 -0.538 -0.179 -0.714 -0.179C12.316 3 12.137 3 11.961 3c-0.478 0 -0.893 0.06 -1.252 0.179s-0.598 0.359 -0.773 0.598c-0.204 0.239 -0.341 0.538 -0.418 0.833c-0.077 0.299 -0.113 0.598 -0.113 1.012c0 0.538 0.028 1.132 0.088 1.73c0.067 0.598 0.095 0.953 0.095 1.072c0 0.239 -0.06 0.538 -0.172 0.773s-0.316 0.538 -0.626 0.893L8.344 10.629c-0.359 0.418 -0.584 0.833 -0.679 1.252c-0.12 0.478 -0.225 0.833 -0.327 1.072c-0.095 0.239 -0.225 0.478 -0.401 0.798l-0.207 0.376c-0.214 0.401 -0.316 0.703 -0.316 0.918c-0.011 0.035 -0.011 0.095 -0.011 0.162c0 0.095 0.025 0.172 0.07 0.232c-0.042 0.049 -0.07 0.095 -0.095 0.137c-0.011 0.028 -0.025 0.067 -0.035 0.102c-0.011 0.025 -0.025 0.077 -0.053 0.148c-0.025 0.07 -0.06 0.148 -0.102 0.246c-0.127 0.07 -0.207 0.102 -0.264 0.102H5.236c-0.13 0 -0.257 0.077 -0.387 0.225s-0.19 0.359 -0.19 0.619c0 0.084 0.025 0.281 0.07 0.591c0.049 0.316 0.07 0.496 0.07 0.555s-0.011 0.12 -0.025 0.179c-0.011 0.067 -0.035 0.12 -0.06 0.172c-0.025 0.06 -0.049 0.113 -0.084 0.165c-0.025 0.06 -0.06 0.12 -0.102 0.186c-0.042 0.06 -0.084 0.127 -0.123 0.19c-0.042 0.109 -0.06 0.211 -0.06 0.306s0.042 0.179 0.123 0.253c0.088 0.074 0.207 0.137 0.359 0.183c0.155 0.056 0.299 0.095 0.422 0.123c0.137 0.028 0.292 0.056 0.461 0.081c0.172 0.028 0.281 0.049 0.327 0.06c0.482 0.109 1.037 0.274 1.663 0.489c0.619 0.225 1.079 0.341 1.371 0.341c0.464 0 0.798 -0.186 1.097 -0.541v0.018c0.418 -0.207 1.012 -0.313 1.789 -0.313c0.478 0 1.252 0.056 2.204 0.172c0.179 0.362 0.478 0.601 0.953 0.728l0.418 0.049c0.239 0 0.478 -0.053 0.654 -0.162c0.179 -0.102 0.418 -0.267 0.654 -0.499l0.598 -0.506c0.299 -0.253 0.598 -0.468 0.893 -0.643c0.359 -0.169 0.598 -0.288 0.714 -0.348c0.179 -0.053 0.299 -0.137 0.418 -0.257c0.12 -0.109 0.179 -0.257 0.179 -0.461c0.004 -0.134 -0.056 -0.257 -0.172 -0.376M15.364 11.438c0.299 0.06 0.538 0.239 0.773 0.598c0.239 0.359 0.359 0.773 0.359 1.371c0 0.429 -0.06 0.918 -0.239 1.473c0.06 0.011 0.179 0.025 0.239 0.028c0.06 0 0.06 0 0.12 0.011c0 0 0.06 0 0.12 0.007c0.359 0.049 0.654 0.162 0.833 0.341c0.179 0.172 0.239 0.327 0.239 0.461c0 0.053 0 0.077 -0.06 0.077c0 0 -0.06 -0.035 -0.06 -0.113c-0.06 -0.077 -0.12 -0.162 -0.179 -0.25c-0.06 -0.095 -0.179 -0.179 -0.418 -0.257c-0.179 -0.084 -0.478 -0.127 -0.773 -0.127c-0.179 0 -0.299 0.053 -0.478 0.155c-0.12 0.102 -0.179 0.204 -0.299 0.302c-0.06 0.102 -0.12 0.155 -0.12 0.155c-0.06 0 -0.06 -0.028 -0.06 -0.084c0.179 -0.345 0.418 -0.566 0.654 -0.661c0.239 -0.633 0.299 -1.125 0.299 -1.459c0 -0.418 -0.06 -0.773 -0.239 -1.192c-0.179 -0.359 -0.478 -0.598 -0.773 -0.714c0.004 -0.063 0.004 -0.063 0.063 -0.123M12.084 5.848q0.269 -0.179 0.538 -0.179c0.239 0 0.418 0.06 0.598 0.239c0.12 0.179 0.179 0.418 0.179 0.773c0 0.299 -0.06 0.538 -0.12 0.893c-0.12 -0.06 -0.179 -0.12 -0.299 -0.12c-0.06 0 -0.12 -0.06 -0.12 -0.06q0.359 -0.09 0.359 -0.538c0 -0.179 -0.06 -0.299 -0.12 -0.478c-0.12 -0.12 -0.239 -0.239 -0.418 -0.239q-0.179 0 -0.359 0.179c-0.12 0.179 -0.12 0.299 -0.12 0.538c0 0.12 0 0.239 0.06 0.299c-0.06 0 -0.12 -0.06 -0.179 -0.06c-0.12 -0.239 -0.179 -0.478 -0.179 -0.773c0 -0.176 0.06 -0.295 0.179 -0.475m0.657 0.657c0 -0.06 -0.06 -0.06 -0.06 -0.12c0.06 0 0.12 -0.06 0.12 -0.06c0.06 0 0.12 0.06 0.179 0.12s0.06 0.12 0.06 0.179s0 0.12 0.06 0.179h-0.12c-0.06 -0.179 -0.12 -0.299 -0.239 -0.299m-0.179 0.893c0.598 0.179 0.893 0.359 0.953 0.478c0.06 0 0.06 0.06 0.06 0.12c0 0.179 -0.12 0.299 -0.239 0.418l-0.538 0.359c-0.598 0.418 -1.072 0.654 -1.491 0.654c-0.299 0 -0.598 -0.12 -0.833 -0.359l-0.359 -0.359c-0.06 -0.06 -0.12 -0.179 -0.179 -0.299c0.359 0.299 0.773 0.478 1.192 0.478c0.478 0 1.072 -0.239 1.849 -0.773c0 0 0 -0.06 -0.06 -0.06v-0.12c-0.773 0.598 -1.371 0.833 -1.849 0.833c-0.359 0 -0.714 -0.179 -1.072 -0.478c-0.06 -0.12 -0.12 -0.179 -0.12 -0.179c0 -0.239 0.239 -0.478 0.598 -0.714c0.418 -0.239 0.714 -0.359 0.893 -0.359c0.243 0 0.657 0.12 1.195 0.359m-2.264 -0.893v-0.12c0.06 0 0.06 -0.06 0.06 -0.06c0.179 0 0.299 0.179 0.299 0.478h-0.12c0 -0.12 -0.06 -0.179 -0.06 -0.299Zm-0.359 -0.418c0.06 -0.179 0.239 -0.299 0.418 -0.299c0.239 0 0.359 0.12 0.478 0.299s0.12 0.359 0.12 0.538c0 0.239 0 0.359 -0.06 0.359v0.06h-0.06v-0.12c0 -0.179 -0.06 -0.359 -0.12 -0.538c-0.12 -0.12 -0.179 -0.179 -0.299 -0.179s-0.239 0.06 -0.299 0.179c-0.12 0.12 -0.12 0.239 -0.12 0.418c0 0.299 0.06 0.478 0.299 0.598c-0.06 0.06 -0.12 0.06 -0.179 0.12c-0.239 -0.239 -0.323 -0.478 -0.323 -0.714c0 -0.302 0.025 -0.482 0.144 -0.721m0.053 14.126c-0.285 0.359 -0.633 0.541 -1.048 0.541c-0.162 0 -0.601 -0.137 -1.322 -0.401s-1.361 -0.439 -1.912 -0.52v-0.004c-0.721 -0.095 -1.114 -0.257 -1.174 -0.489c0 -0.137 0.077 -0.327 0.225 -0.584c0.148 -0.246 0.221 -0.489 0.221 -0.735c0 -0.127 -0.025 -0.302 -0.07 -0.524c-0.049 -0.225 -0.07 -0.387 -0.07 -0.482c0 -0.207 0.077 -0.404 0.225 -0.577c0.113 -0.06 0.327 -0.095 0.643 -0.095h0.162c0.077 -0.007 0.13 -0.007 0.162 -0.007c0.137 -0.028 0.239 -0.06 0.299 -0.095c0.06 -0.042 0.095 -0.127 0.113 -0.25c0.011 -0.13 0.06 -0.257 0.144 -0.369s0.257 -0.197 0.506 -0.246c0.197 0.049 0.376 0.137 0.531 0.274c0.162 0.137 0.285 0.274 0.362 0.411c0.088 0.127 0.204 0.302 0.327 0.538c0.13 0.232 0.232 0.404 0.316 0.517l0.404 0.541c0.679 0.865 1.02 1.575 1.02 2.141c-0.004 0.158 -0.004 0.295 -0.063 0.411m1.311 -1.167c-0.714 0 -1.252 -0.165 -1.501 -0.506c-0.127 -0.232 -0.186 -0.352 -0.186 -0.359c0 -0.028 0.007 -0.042 0.018 -0.042c0.137 0 0.299 -0.07 0.418 -0.214c0.179 -0.137 0.239 -0.264 0.239 -0.387c0 -0.239 -0.359 -0.643 -1.03 -1.227l-0.633 -0.411c-0.499 -0.362 -0.816 -0.65 -0.949 -0.847c-0.12 -0.204 -0.186 -0.436 -0.186 -0.703c0 -0.362 0.13 -0.858 0.38 -1.396c0.25 -0.598 0.461 -1.012 0.633 -1.192l0.06 0.06c0.049 0 0.077 0 0.088 0.06c-0.207 0.239 -0.422 0.598 -0.636 1.132c-0.221 0.538 -0.327 0.988 -0.327 1.399c0 0.334 0.13 0.591 0.401 0.773c-0.025 -0.144 -0.035 -0.285 -0.035 -0.422c0 -0.686 0.246 -1.515 0.728 -2.408l0.446 -0.893c0.011 0 0.028 -0.12 0.053 -0.239c0.018 -0.179 0.109 -0.418 0.274 -0.833l0.376 -0.714c0.12 -0.239 0.179 -0.478 0.179 -0.654l0.359 0.299c0.179 0.239 0.478 0.299 0.833 0.299c0.418 0 0.893 -0.179 1.491 -0.598l0.538 -0.418c0.12 0.239 0.239 0.538 0.359 0.833c0.179 0.299 0.299 0.598 0.478 0.893c0 0.299 0.06 0.478 0.12 0.714c0.12 0.179 0.179 0.359 0.299 0.538l0.359 0.538c0.12 0.239 0.239 0.538 0.299 0.893c0.12 0.418 0.12 0.865 0.12 1.406c0 0.359 0 0.756 -0.06 1.216c-0.12 -0.088 -0.239 -0.13 -0.359 -0.13c-0.179 0 -0.299 0.088 -0.418 0.281c-0.12 0.19 -0.179 0.482 -0.179 0.868v0.221c0.06 0.077 0.06 0.264 0.06 0.559c-0.78 1.076 -1.793 1.61 -3.104 1.61m7.991 -0.503c-0.12 0.095 -0.239 0.165 -0.418 0.214c-0.12 0.053 -0.359 0.148 -0.598 0.292c-0.299 0.144 -0.538 0.309 -0.773 0.499l-1.012 0.875c-0.06 0.028 -0.06 0.06 -0.179 0.102c-0.06 0.049 -0.12 0.088 -0.179 0.13c-0.06 0.049 -0.179 0.088 -0.299 0.113q-0.179 0.053 -0.359 0.053c-0.478 0 -0.893 -0.225 -1.132 -0.675v-0.014l0.12 -0.661c0 -0.236 0 -0.446 0.06 -0.633c0 -0.186 0.06 -0.302 0.06 -0.359v-1.853c0 -0.626 0.12 -0.935 0.418 -0.935c0.12 0 0.179 0.053 0.299 0.155c0.06 0.25 0.12 0.471 0.239 0.661c0.179 0.257 0.418 0.387 0.893 0.387c0.12 0 0.299 -0.053 0.478 -0.155c0.12 -0.102 0.299 -0.207 0.359 -0.327c0.12 -0.12 0.239 -0.232 0.359 -0.334c0.06 -0.102 0.179 -0.155 0.239 -0.155s0.06 0.007 0.06 0.025c0.06 0.067 0.06 0.186 0.12 0.345l0.06 0.345c0.06 0.281 0.179 0.496 0.299 0.65c0.179 0.162 0.299 0.257 0.418 0.299s0.239 0.109 0.359 0.204s0.239 0.225 0.239 0.401c-0.007 0.148 -0.067 0.264 -0.127 0.352m-7.805 -11.204s-0.06 -0.06 -0.06 -0.12s0 -0.06 0.06 -0.06l0.12 0.06c0.06 0.06 0.12 0.12 0.12 0.179s-0.06 0.06 -0.06 0.06h-0.06c-0.06 0 -0.06 -0.06 -0.12 -0.12m-0.598 0.06c0 -0.06 0 -0.12 0.06 -0.179c0.12 0 0.179 -0.06 0.179 -0.06c0.06 0 0.06 0.06 0.06 0.06c0 0.06 -0.06 0.12 -0.179 0.239h-0.06c-0.06 0 -0.06 0 -0.06 -0.06",
]

/// A filled 24-unit SVG path (M L H V C S Q Z, absolute and relative).
struct SVGMark: Shape {
    let d: String
    func path(in rect: CGRect) -> Path {
        var p = Path()
        var tokens: [String] = []
        var cur = ""
        for ch in d {
            if ch.isLetter {
                if !cur.isEmpty { tokens.append(cur); cur = "" }
                tokens.append(String(ch))
            } else if ch == " " || ch == "," {
                if !cur.isEmpty { tokens.append(cur); cur = "" }
            } else if ch == "-" && !cur.isEmpty && !cur.hasSuffix("e") {
                tokens.append(cur); cur = "-"
            } else if ch == "." && cur.contains(".") {
                tokens.append(cur); cur = "."
            } else {
                cur.append(ch)
            }
        }
        if !cur.isEmpty { tokens.append(cur) }
        var i = 0, cmd = "M"
        var pt = CGPoint.zero, start = CGPoint.zero, ctrl = CGPoint.zero
        func num() -> CGFloat { defer { i += 1 }; return CGFloat(Double(tokens[i]) ?? 0) }
        while i < tokens.count {
            if let c = tokens[i].first, c.isLetter { cmd = tokens[i]; i += 1 }
            let rel = cmd == cmd.lowercased()
            let o = rel ? pt : .zero
            switch cmd.uppercased() {
            case "Z": p.closeSubpath(); pt = start; ctrl = pt; continue
            case "M":
                pt = CGPoint(x: o.x + num(), y: o.y + num()); p.move(to: pt); start = pt
                cmd = rel ? "l" : "L"
            case "L": pt = CGPoint(x: o.x + num(), y: o.y + num()); p.addLine(to: pt)
            case "H": pt.x = (rel ? pt.x : 0) + num(); p.addLine(to: pt)
            case "V": pt.y = (rel ? pt.y : 0) + num(); p.addLine(to: pt)
            case "C":
                let c1 = CGPoint(x: o.x + num(), y: o.y + num()), c2 = CGPoint(x: o.x + num(), y: o.y + num())
                pt = CGPoint(x: o.x + num(), y: o.y + num()); p.addCurve(to: pt, control1: c1, control2: c2); ctrl = c2
                continue
            case "S":
                let c1 = CGPoint(x: 2 * pt.x - ctrl.x, y: 2 * pt.y - ctrl.y), c2 = CGPoint(x: o.x + num(), y: o.y + num())
                pt = CGPoint(x: o.x + num(), y: o.y + num()); p.addCurve(to: pt, control1: c1, control2: c2); ctrl = c2
                continue
            case "Q":
                let c = CGPoint(x: o.x + num(), y: o.y + num())
                pt = CGPoint(x: o.x + num(), y: o.y + num()); p.addQuadCurve(to: pt, control: c); ctrl = c
                continue
            default: i += 1
            }
            ctrl = pt
        }
        let s = min(rect.width, rect.height) / 24
        return p.applying(CGAffineTransform(scaleX: s, y: s).translatedBy(x: rect.minX / s, y: rect.minY / s))
    }
}

/// Lucide's `progress-2-4`: the 2-unit ring (r 10) around a filled half wedge (r 6).
struct ProgressMark: View {
    let size: CGFloat
    var body: some View {
        let s = size / 24
        ZStack {
            Circle().stroke(lineWidth: 2 * s).frame(width: 20 * s, height: 20 * s)
            Path { p in
                p.move(to: CGPoint(x: 12 * s, y: 12 * s))
                p.addArc(center: CGPoint(x: 12 * s, y: 12 * s), radius: 6 * s, startAngle: .degrees(-90), endAngle: .degrees(90), clockwise: false)
                p.closeSubpath()
            }
        }
        .frame(width: size, height: size)
    }
}

@MainActor
final class SinkState: ObservableObject {
    @Published var echo = ""
    @Published var themeId: String
    @Published var mode: Mode
    let model: SurfaceModel
    let options: LaunchOptions
    /// The specimen's nested node when `-shot` names one.
    let specimenJSON: String?
    /// An eval specimen's data model (VAPP-127).
    let specimenData: JSONValue?
    var isSpecimen: Bool { specimenJSON != nil }

    init(options: LaunchOptions) {
        self.options = options
        let specimen = SinkState.specimen(options.shot)
        specimenJSON = specimen?.node
        specimenData = specimen?.data
        let mode = Mode(rawValue: options.mode) ?? .dark
        themeId = options.theme
        self.mode = mode
        var surfaceOptions = SurfaceOptions()
        surfaceOptions.theme = SinkState.theme(options.theme)
        surfaceOptions.mode = mode
        model = try! SurfaceModel(id: "kitchen-sink", options: surfaceOptions)
        let host = ClosureHost(
            icons: { name, size in
                if let d = markIcons[name] {
                    let inner = size * 0.8
                    return AnyView(SVGMark(d: d).frame(width: inner, height: inner).frame(width: size, height: size))
                }
                if name == "progress-2-4" {
                    return AnyView(ProgressMark(size: size * 0.8).frame(width: size, height: size))
                }
                guard let symbol = symbolIcons[name] else { return nil }
                return AnyView(Image(systemName: symbol).font(.system(size: size * 0.8, weight: .medium)).frame(width: size, height: size))
            },
            actions: { e in print("[exponential-ui] action \(e.name) \(e.componentId) \(e.context) \(e.payload.map { "\($0)" } ?? "")") },
            inputs: { [weak self] e in
                Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(150))
                    guard let self else { return }
                    if let path = e.path { self.model.setData(path: path, value: e.value) }
                    self.echo = e.value.displayText
                }
            }
        )
        model.host = host
        var json = specimenJSON
            ?? (try! String(contentsOf: Bundle.main.url(forResource: "kitchen-sink", withExtension: "json")!, encoding: .utf8))
        if options.rtl {
            json = json.replacingOccurrences(of: "\"direction\": \"ltr\"", with: "\"direction\": \"rtl\"")
        }
        try! model.setNested(json: json)
        model.setData(path: "/draft", value: .object(["title": .string("")]))
        if case let .object(data)? = specimenData {
            for (key, value) in data { model.setData(path: "/\(key)", value: value) }
        }
    }

    /// `-shot <id>` naming a `fixtures/specimens.json` entry (the
    /// ui.exponential.at component pages): that entry's nested `node`; an
    /// `exponential-ui-eval-<corpus id>` names a recorded eval surface of
    /// `eval/eval-specimens.json` (VAPP-127), its data model included.
    /// Anything else (incl. `exponential-ui-kitchen-sink`) = nil.
    static func specimen(_ id: String?) -> (node: String, data: JSONValue?)? {
        guard let id else { return nil }
        let file = id.hasPrefix("exponential-ui-eval-") ? "eval-specimens" : "specimens"
        guard let url = Bundle.main.url(forResource: file, withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let specimens = root["specimens"] as? [[String: Any]],
              let entry = specimens.first(where: { $0["id"] as? String == id }),
              let node = entry["node"],
              let out = try? JSONSerialization.data(withJSONObject: node),
              let json = String(data: out, encoding: .utf8)
        else {
            print("[exponential-ui] specimen \(id): not in \(file).json")
            return nil
        }
        return (json, entry["data"].map { JSONValue(any: $0) })
    }

    /// `-frames` (VAPP-127): every visible node's frame, points in surface coordinates.
    func writeFrames(to path: String) {
        let frames: [[String: Any]] = model.nodes.indices.compactMap { i in
            guard i < model.frames.count, !model.nodes[i].hidden else { return nil }
            let f = model.frames[i]
            return ["id": model.nodes[i].id, "x": Double(f.minX), "y": Double(f.minY), "w": Double(f.width), "h": Double(f.height)]
        }
        let doc: [String: Any] = ["surface": options.shot ?? "kitchen-sink", "width": Double(model.width), "frames": frames]
        guard let data = try? JSONSerialization.data(withJSONObject: doc, options: [.sortedKeys]) else { return }
        do {
            try data.write(to: URL(fileURLWithPath: path))
            print("[exponential-ui] frames \(frames.count) → \(path)")
        } catch {
            print("[exponential-ui] frames \(path): \(error)")
        }
    }


    static func theme(_ id: String) -> ThemeHandle? {
        if id == "brand", let url = Bundle.main.url(forResource: "brand.theme", withExtension: "json"), let json = try? String(contentsOf: url, encoding: .utf8) {
            // An unusable theme file falls back to the default and says why.
            return ThemeHandle.loadOrDefault(json: json) { issues in
                for issue in issues { print("brand theme: \(issue.path): \(issue.message)") }
            }
        }
        return ThemeHandle.builtin(id)
    }

    func apply() {
        if let t = SinkState.theme(themeId) { model.setTheme(t) }
        model.setMode(mode)
    }
}

struct ContentView: View {
    @StateObject private var state = SinkState(options: LaunchOptions.parse())

    var body: some View {
        let options = state.options
        VStack(spacing: 0) {
            if options.shot == nil {
                HStack {
                    Picker("Theme", selection: $state.themeId) {
                        ForEach(ExponentialUI.builtinThemes + ["brand"], id: \.self) { Text($0).tag($0) }
                    }
                    .pickerStyle(.menu)
                    Spacer()
                    Picker("Mode", selection: $state.mode) {
                        Text("Dark").tag(Mode.dark)
                        Text("Light").tag(Mode.light)
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 140)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .onChange(of: state.themeId) { _, _ in state.apply() }
                .onChange(of: state.mode) { _, _ in state.apply() }
            }
            ScrollView(.vertical) {
                VStack(alignment: .leading, spacing: 0) {
                    ExponentialSurface(model: state.model)
                        .frame(width: options.width)
                    if !state.isSpecimen {
                        Text("host: \(state.echo)")
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .accessibilityIdentifier("host-echo")
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(state.model.color("background") ?? .clear)
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { h in
                state.model.setViewport(width: state.model.width, height: h)
            }
        }
        .background(state.model.color("background") ?? .clear)
        .ignoresSafeArea(.keyboard)
        .preferredColorScheme(state.mode == .dark ? .dark : .light)
        .task {
            if let path = options.a11yDump {
                try? await Task.sleep(for: .seconds(2))
                let labels = AccessibilityWalk.labels()
                try? (labels.joined(separator: "\n") + "\n---\n" + AccessibilityWalk.trace.joined(separator: "\n") + "\n").write(toFile: path, atomically: true, encoding: .utf8)
            }
            if let path = options.dump {
                try? await Task.sleep(for: .seconds(2))
                await FullSurfaceDump.write(model: state.model, to: path)
            }
            if let path = options.frames {
                try? await Task.sleep(for: .seconds(2))
                state.writeFrames(to: path)
            }
        }
    }
}
