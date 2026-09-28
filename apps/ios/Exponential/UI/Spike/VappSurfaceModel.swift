import Foundation
import Observation
import os
@preconcurrency import VappSpikeKit

/// One node of the fixture with its props parsed ONCE (LANES.md step 1).
struct VappNode: Identifiable, Sendable {
    let index: Int
    let id: String
    let kind: String
    let depth: Int
    let parent: Int?
    let isContainer: Bool
    let props: VappProps
}

/// The typed subset of `props` the painters read.
struct VappProps: Sendable {
    var text: String?
    var variant: String?
    var label: String?
    var placeholder: String?
    var echo = false
    var checked = false
    var options: [String] = []
    var value: String?
    var title: String?
    var meta: String?
    var count: Int?
    var selected = false
    var tone: String?
    var name: String?
    var size: Double?
    var alt: String?
    var progress: Double?

    init(json: String) {
        guard let data = json.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return }
        text = object["text"] as? String
        variant = object["variant"] as? String
        label = object["label"] as? String
        placeholder = object["placeholder"] as? String
        echo = object["echo"] as? Bool ?? false
        checked = object["checked"] as? Bool ?? false
        options = object["options"] as? [String] ?? []
        title = object["title"] as? String
        meta = object["meta"] as? String
        count = (object["count"] as? NSNumber)?.intValue
        selected = object["selected"] as? Bool ?? false
        tone = object["tone"] as? String
        name = object["name"] as? String
        size = (object["size"] as? NSNumber)?.doubleValue
        alt = object["alt"] as? String
        // `value` is a string on select, a number on progress.
        if let number = object["value"] as? NSNumber, !(object["value"] is String) {
            progress = number.doubleValue
        } else {
            value = object["value"] as? String
        }
        // image: `placeholder` is a colour, not a hint.
    }

    /// The accessibility label LANES.md asks for: text/label/title/placeholder.
    var accessibilityText: String? {
        text ?? label ?? title ?? value ?? placeholder ?? count.map(String.init)
    }
}

/// The key a cached pass answers for.
struct VappPassKey: Equatable {
    let width: Float
    let pressed: [String]
    let nonce: Int
}

/// NON-observed reference box: the layout writes here from inside
/// `sizeThatFits`/`placeSubviews` (never `@State`), the screen polls it.
final class VappLayoutBox: @unchecked Sendable {
    var key: VappPassKey?
    var result: LayoutResult?
    var framesByIndex: [Int: PlacedFrame] = [:]
    var lastSize: CGSize?
    var passes = 0
    var lastWallNs: UInt64 = 0
    var lastLayoutNs: UInt64 = 0
    var lastCalls: UInt32 = 0
    var bestWallNs: UInt64 = .max
    var bestLayoutNs: UInt64 = .max
    /// Distinct proposal shapes `sizeThatFits` received (for FINDINGS).
    var proposalLog: [String] = []
    var cacheHits = 0
}

@MainActor
@Observable
final class VappSurfaceModel {
    /// Rebuilt by `rebuild()` so the bench can time a COLD pass (a fresh
    /// taffy tree: every leaf measured again).
    @ObservationIgnored private(set) var surface: Surface
    @ObservationIgnored private let treeJson: String
    @ObservationIgnored let box = VappLayoutBox()
    let nodes: [VappNode]
    let bench: Int?
    let rtl: Bool
    @ObservationIgnored private(set) var buildNs: UInt64
    /// Pressed button ids → `set_pressed` → relayout.
    var pressed: Set<String> = []
    /// Bumping this forces a fresh FFI pass (the Re-layout button).
    var nonce = 0
    /// Per-index paint data (visual + font). Seeded from `layout_fixed` at 390,
    /// refreshed after a real pass only when it changed (pressed opacity,
    /// breakpoint), so it settles after one extra render.
    var paint: [Int: PlacedFrame] = [:]

    static let signposter = OSSignposter(subsystem: "at.exponential", category: "VappSpike")

    init(bench: Int?, rtl: Bool) {
        self.bench = bench
        self.rtl = rtl
        var json = bench.map { benchTreeJson(n: UInt32($0)) } ?? kitchenSinkJson()
        if rtl {
            json = json.replacingOccurrences(of: "\"direction\": \"ltr\"", with: "\"direction\": \"rtl\"")
            json = json.replacingOccurrences(of: "\"direction\":\"ltr\"", with: "\"direction\":\"rtl\"")
        }
        // swiftlint:disable:next force_try
        let surface = try! Surface(treeJson: json)
        self.surface = surface
        self.treeJson = json
        self.buildNs = surface.buildNs()
        self.nodes = surface.nodes().map { info in
            VappNode(
                index: Int(info.index),
                id: info.id,
                kind: info.kind,
                depth: Int(info.depth),
                parent: info.parent.map(Int.init),
                isContainer: info.isContainer,
                props: VappProps(json: info.propsJson)
            )
        }
        // Seed paint data from a THROWAWAY surface: `layout_fixed` on the live
        // one leaves taffy's per-node cache holding fixed-measure sizes, which
        // the next real pass then reuses (FINDINGS-ios.md, "stale cache").
        // swiftlint:disable:next force_try
        let seed = try! Surface(treeJson: json)
        _ = seed.setViewport(width: 390, height: 0)
        let fixed = seed.layoutFixed()
        var paint: [Int: PlacedFrame] = [:]
        for frame in fixed.frames { paint[Int(frame.index)] = frame }
        self.paint = paint
        print("VappSpike surface built nodes=\(nodes.count) buildNs=\(buildNs) rtl=\(rtl)")
    }

    /// Called async after a pass: publish paint changes (never inside layout).
    func publishPaintIfChanged() {
        let frames = box.framesByIndex
        var changed = false
        for (index, frame) in frames {
            let old = paint[index]
            if old?.visual != frame.visual || old?.fontSize != frame.fontSize
                || old?.fontWeight != frame.fontWeight || old?.lineHeight != frame.lineHeight {
                changed = true
                break
            }
        }
        if changed { paint = frames }
    }

    /// Cold re-layout: a fresh Surface (taffy cache empty) + a forced pass.
    func rebuild() {
        // swiftlint:disable:next force_try
        surface = try! Surface(treeJson: treeJson)
        buildNs = surface.buildNs()
        nonce += 1
    }

    /// A node's placed rect in the surface's (LTR) coordinate space, after
    /// the RTL un-flip the Layout applies.
    func placedRect(_ index: Int) -> CGRect? {
        guard let frame = box.framesByIndex[index] else { return nil }
        var x = CGFloat(frame.x)
        let w = CGFloat(frame.width)
        if rtl, VappSpikeLaunch.unflipsRTL, let width = box.lastSize?.width {
            x = width - x - w
        }
        return CGRect(x: x, y: CGFloat(frame.y), width: w, height: CGFloat(frame.height))
    }

    var caption: String {
        let b = box
        guard b.passes > 0 else { return "\(nodes.count) nodes · waiting for a pass" }
        return "\(nodes.count) nodes · \(b.lastCalls) calls · taffy \(b.lastLayoutNs / 1000) µs · wall \(b.lastWallNs / 1000) µs"
    }
}
