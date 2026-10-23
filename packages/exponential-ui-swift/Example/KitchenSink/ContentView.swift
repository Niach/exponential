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
]

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
