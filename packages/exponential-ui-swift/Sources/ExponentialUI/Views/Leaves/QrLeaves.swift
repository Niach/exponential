import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

// VAPP-162 (docs/round-5-contract.md §6, §7): QrCode and QrScanner.

/// The core's QR matrix (`qrMatrixJson`, the port of Nayuki's generator;
/// `fixtures/qr-code.json`): `size` modules per side, `rows` of '1' (dark)
/// / '0' (light), no quiet zone. Never a platform QR library.
public struct QrMatrix: Equatable, Sendable {
    public var size: Int
    public var version: Int
    public var mask: Int
    public var rows: [String]

    /// The matrix of `value` at `level` (`L|M|Q|H`, anything else `M`); nil
    /// for an empty value or one version 40 cannot hold at that level.
    public static func of(_ value: String, level: String) -> QrMatrix? {
        parse(qrMatrixJson(value: value, level: level))
    }

    static func parse(_ json: String) -> QrMatrix? {
        let v = JSONValue.parse(json)
        guard let rows = v["rows"]?.array?.compactMap(\.string) else { return nil }
        return QrMatrix(size: Int(v["size"]?.number ?? Double(rows.count)), version: Int(v["version"]?.number ?? 0), mask: Int(v["mask"]?.number ?? 0), rows: rows)
    }
}

/// `catalog/style.json qrCode` / `qrScanner`.
enum QrScannerGeometry {
    static let quietZone = 4
    static let maxWidth: CGFloat = 280
    static let cornerLength: CGFloat = 24
    static let cornerStroke: CGFloat = 3
    static let feedAlpha = 0.06
    static let scanLineHeight: CGFloat = 2
    static let scanLineInset: CGFloat = 16
    static let sweepSeconds = 2.0
    static let placeholderIcon = "ui-scan"
    static let placeholderIconSize: CGFloat = 32

    /// The viewfinder edge in a scanner `width` wide.
    static func viewfinder(width: CGFloat) -> CGFloat { max(0, min(width, maxWidth)) }
}

extension Color {
    /// WCAG relative luminance of the resolved colour (its linear sRGB
    /// components); the plate swap only compares two of them.
    var relativeLuminance: Double {
        let r = resolve(in: EnvironmentValues())
        return 0.2126 * Double(r.linearRed) + 0.7152 * Double(r.linearGreen) + 0.0722 * Double(r.linearBlue)
    }
}

/// `QrCode`: the `size` pt plate centred at the top of the leaf with the
/// core's matrix inside a 4-module quiet zone, then the `label` caption
/// (muted, centred) `labelGap` below. DARK MODULES ON A LIGHT PLATE
/// ALWAYS: of the plate and module recipe colours the lighter fills the
/// plate. The img's name is the label (none = decorative, `A11yInfo`).
struct QrCodeLeaf: View {
    let cx: LeafContext

    /// The plate and module colours after the swap (lighter = plate).
    static func colours(plate: Color, module: Color) -> (plate: Color, module: Color) {
        plate.relativeLuminance >= module.relativeLuminance ? (plate, module) : (module, plate)
    }

    var body: some View {
        let size = max(0, cx.props.num("size") ?? 160)
        let platePart = cx.part("QrCode", "plate")
        let (plate, module) = Self.colours(plate: platePart.style.background ?? .white, module: cx.part("QrCode", "module").color ?? .black)
        let rows = QrMatrix.of(cx.props.str("value"), level: cx.props.str("level").isEmpty ? "M" : cx.props.str("level"))?.rows
        let label = cx.props.str("label")
        let labelStyle = SurfaceMeasurer.qrLabelStyle(cx.part("QrCode", "label"), base: cx.textStyle)
        let labelColor = cx.part("QrCode", "label").color ?? cx.themeColor("mutedForeground") ?? cx.ink
        VStack(spacing: cx.style.gap) {
            QrCodeView(rows: rows, size: size, plate: plate, module: module, radius: platePart.style.radius, quietZone: QrScannerGeometry.quietZone)
            if !label.isEmpty {
                VStack(spacing: 0) {
                    ForEach(Array(TextLeaf.shown(label, labelStyle, width: cx.inner.width, clamp: nil).enumerated()), id: \.offset) { _, line in
                        TextLabel(line, labelStyle, color: labelColor, align: "center", lines: 1)
                            .frame(width: cx.inner.width, height: labelStyle.lineHeight)
                    }
                }
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .top)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

// MARK: - the host camera

/// One running camera feed a host hands a `QrScanner`: the live view
/// (filling the viewfinder, clipped by it) and the decoded strings. The
/// renderer stops consuming `values` when the scanner goes away, is
/// disabled or its formats change: stop the camera in the stream's
/// `onTermination`.
public struct QrScannerFeed {
    public var view: AnyView
    public var values: AsyncStream<String>

    public init(view: AnyView, values: AsyncStream<String>) {
        self.view = view
        self.values = values
    }

    /// A feed whose decoded strings the host pushes through `report`
    /// (`AVCaptureMetadataOutput`'s delegate, a Vision request); `onStop`
    /// runs once the renderer stops listening.
    @MainActor
    public static func reporting<V: View>(_ view: V, onStop: @escaping @Sendable () -> Void = {}) -> (feed: QrScannerFeed, report: (String) -> Void) {
        let (stream, continuation) = AsyncStream<String>.makeStream()
        continuation.onTermination = { _ in onStop() }
        return (QrScannerFeed(view: AnyView(view), values: stream), { continuation.yield($0) })
    }
}

/// THE QrScanner host hook (round-5 contract §7; the camera is
/// HOST-OWNED, AVFoundation stays in the host): `start(formats)` opens a
/// camera decoding codes of `formats` (default `["qr"]`) and returns its
/// feed, or nil (no camera, no permission: the placeholder shows).
/// Install it with `.exponentialQrScanner(_:)` around `ExponentialSurface`.
public struct ExponentialQrScanner {
    public var start: @MainActor (_ formats: [String]) -> QrScannerFeed?

    public init(start: @escaping @MainActor (_ formats: [String]) -> QrScannerFeed?) {
        self.start = start
    }
}

private struct QrScannerKey: EnvironmentKey {
    static let defaultValue: ExponentialQrScanner? = nil
}

public extension EnvironmentValues {
    /// The host's camera for every `QrScanner` beneath (nil = none).
    var exponentialQrScanner: ExponentialQrScanner? {
        get { self[QrScannerKey.self] }
        set { self[QrScannerKey.self] = newValue }
    }
}

public extension View {
    /// The host camera the `QrScanner` nodes of the surfaces beneath use.
    func exponentialQrScanner(_ scanner: ExponentialQrScanner?) -> some View {
        environment(\.exponentialQrScanner, scanner)
    }
}

/// `QrScanner`: the viewfinder chrome around the HOST's feed. The square
/// viewfinder (as wide as the leaf, at most 280 pt, centred, `$radius.xl`,
/// clipping) holds the feed (the host's view; none = the `feed` colour at
/// 6 %), the four `corner` brackets and, while a feed shows, the sweeping
/// `scanLine` (static at the centre under reduced motion or disabled).
/// Without a camera: the `ui-scan` glyph (32 pt, muted) over
/// `cameraUnavailable`. The `hint` sits under the viewfinder. A decoded
/// value is dispatched as `scanned {value}` once per distinct value (until
/// it changes or the scanner remounts) and announced politely; `disabled`
/// stops the camera and dims the chrome (the root recipe's opacity).
struct QrScannerLeaf: View {
    let cx: LeafContext
    @Environment(\.exponentialQrScanner) private var host
    @Environment(\.xuiReducedMotion) private var surfaceReduced
    @Environment(\.accessibilityReduceMotion) private var platformReduced
    @State private var feed: QrScannerFeed?

    /// The `label` prop (default `$string.scanQrCode`).
    static func label(_ cx: LeafContext) -> String {
        let raw = cx.node.a11y.label ?? cx.props.str("label")
        if raw.isEmpty { return cx.model.builtinString("scanQrCode") }
        return raw.hasPrefix("$string.") ? cx.model.builtinString(String(raw.dropFirst("$string.".count))) : raw
    }

    static func formats(_ props: Props) -> [String] {
        let list = props["formats"]?.array?.compactMap(\.string).filter { !$0.isEmpty } ?? []
        return list.isEmpty ? ["qr"] : list
    }

    var body: some View {
        let disabled = cx.props.flag("disabled") || cx.model.isDisabled(cx.index)
        let formats = Self.formats(cx.props)
        let edge = QrScannerGeometry.viewfinder(width: cx.inner.width)
        let finder = cx.part("QrScanner", "viewfinder")
        let radius = finder.style.radius
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        let feedColor = cx.part("QrScanner", "feed").style.background ?? cx.themeColor("foreground") ?? cx.ink
        let corner = cx.part("QrScanner", "corner").color ?? cx.themeColor("primary") ?? cx.ink
        let line = cx.part("QrScanner", "scanLine").style.background ?? cx.themeColor("primary") ?? cx.ink
        let hint = cx.props.str("hint")
        let hintStyle = SurfaceMeasurer.qrHintStyle(cx.part("QrScanner", "hint"), base: cx.textStyle)
        let hintColor = cx.part("QrScanner", "hint").color ?? cx.themeColor("mutedForeground") ?? cx.ink
        let live = feed != nil && !disabled
        VStack(spacing: cx.style.gap) {
            ZStack(alignment: .topLeading) {
                feedColor.opacity(QrScannerGeometry.feedAlpha)
                if let feed, !disabled {
                    feed.view.frame(width: edge, height: edge).clipped()
                }
                QrViewfinderCorners(length: QrScannerGeometry.cornerLength, stroke: QrScannerGeometry.cornerStroke, radius: radius, color: corner)
                if live {
                    QrScanLine(color: line, height: QrScannerGeometry.scanLineHeight, inset: QrScannerGeometry.scanLineInset, period: QrScannerGeometry.sweepSeconds, moving: !(surfaceReduced || platformReduced))
                } else {
                    placeholder
                }
            }
            .frame(width: edge, height: edge)
            .clipShape(shape)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(live ? Self.label(cx) : "\(Self.label(cx)), \(cx.model.builtinString("cameraUnavailable"))")
            .accessibilityAddTraits(.isImage)
            if !hint.isEmpty {
                VStack(spacing: 0) {
                    ForEach(Array(TextLeaf.shown(hint, hintStyle, width: cx.inner.width, clamp: nil).enumerated()), id: \.offset) { _, l in
                        TextLabel(l, hintStyle, color: hintColor, align: "center", lines: 1)
                            .frame(width: cx.inner.width, height: hintStyle.lineHeight)
                    }
                }
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .top)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
        .task(id: ScanKey(disabled: disabled, formats: formats, hosted: host != nil)) {
            await scan(disabled: disabled, formats: formats)
        }
    }

    /// The placeholder: the scan glyph over `cameraUnavailable`, centred.
    private var placeholder: some View {
        let p = cx.part("QrScanner", "placeholder")
        let ts = TextStyle(fontSize: p.px("fontSize") ?? 12, fontWeight: Int(p.props.num("fontWeight") ?? 400), lineHeight: p.px("lineHeight") ?? 16, fontFamily: p.fontFamily ?? cx.textStyle.fontFamily)
        let ink = p.color ?? cx.themeColor("mutedForeground") ?? cx.ink
        let icon = cx.part("QrScanner", "placeholderIcon").color ?? ink
        let text = cx.model.builtinString("cameraUnavailable")
        return VStack(spacing: cx.spacing("xs")) {
            ConceptIcon(name: QrScannerGeometry.placeholderIcon, size: QrScannerGeometry.placeholderIconSize, color: icon, model: cx.model)
            TextLabel(text, ts, color: ink, align: "center", lines: 1)
                .frame(width: TextShaper.width(text, ts), height: ts.lineHeight)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private struct ScanKey: Equatable {
        let disabled: Bool
        let formats: [String]
        let hosted: Bool
    }

    /// Run the host camera while the scanner shows and is enabled: each
    /// distinct decoded value fires `scanned {value}` once.
    private func scan(disabled: Bool, formats: [String]) async {
        defer { feed = nil }
        guard !disabled, let host, let started = host.start(formats) else {
            feed = nil
            return
        }
        feed = started
        var last: String?
        for await value in started.values {
            if Task.isCancelled { break }
            guard !value.isEmpty, value != last else { continue }
            last = value
            cx.model.scanned(cx.index, value: value)
        }
    }
}

extension SurfaceModel {
    /// VAPP-162: a QrScanner's decoded value (the host camera): announced
    /// politely, then `scanned {value}` through the core.
    public func scanned(_ index: Int, value: String) {
        guard let n = node(index), n.component == "QrScanner", !n.props.flag("disabled") else { return }
        announce(value, live: "polite")
        fire(index, "scanned", payload: .object(["value": .string(value)]))
    }
}
