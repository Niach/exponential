import os
import SwiftUI
import UIKit
@preconcurrency import VappSpikeKit

/// Tags every subview with its node index (pre-order, containers included).
struct VappIndexKey: LayoutValueKey {
    static let defaultValue: Int = -1
}

/// The measure callback: taffy asks for a LEAF's content size by node index,
/// we answer with the SwiftUI view's own `sizeThatFits` (LANES.md step 3).
/// Called synchronously on the main thread from inside `layout()`.
final class VappSubviewMeasure: Measure, @unchecked Sendable {
    private let subviews: LayoutSubviews
    private let positions: [Int: Int]
    private let nodes: [VappNode]

    static let logs = ProcessInfo.processInfo.arguments.contains("-vappLogMeasure")

    init(subviews: LayoutSubviews, positions: [Int: Int], nodes: [VappNode]) {
        self.subviews = subviews
        self.positions = positions
        self.nodes = nodes
    }

    func measure(call: MeasureCall) -> FfiSize {
        MainActor.assumeIsolated {
            let index = Int(call.index)
            guard let position = positions[index] else { return FfiSize(width: 0, height: 0) }
            let subview = subviews[position]
            let wrap: Float? = call.knownWidth ?? (
                call.availableWidthMode == .definite
                    ? call.availableWidth
                    : (call.availableWidthMode == .minContent ? 0 : nil)
            )
            // Text at a 0 proposal breaks INSIDE words; min-content is the
            // longest word, so measure the words one by one.
            if call.knownWidth == nil, call.availableWidthMode == .minContent,
               nodes[index].kind == "text", let text = nodes[index].props.text {
                let longest = text.split(separator: " ").map(String.init)
                    .map { Self.textWidth($0, subview: subview, full: text) }
                    .max() ?? 0
                let size = subview.sizeThatFits(ProposedViewSize(width: longest, height: nil))
                if Self.logs {
                    print("VappSpike measure \(nodes[index].id) MIN known=-x\(call.knownHeight.map { "\($0)" } ?? "-") avail=\(call.availableWidth.map { "\($0)" } ?? "-")/minContent -> \(longest)x\(size.height)")
                }
                return FfiSize(width: Float(longest), height: Float(ceil(size.height)))
            }
            let proposal = ProposedViewSize(
                width: wrap.map { CGFloat($0) },
                height: call.knownHeight.map { CGFloat($0) }
            )
            let size = subview.sizeThatFits(proposal)
            if Self.logs {
                print("VappSpike measure \(nodes[index].id) known=\(call.knownWidth.map { "\($0)" } ?? "-")x\(call.knownHeight.map { "\($0)" } ?? "-") avail=\(call.availableWidth.map { "\($0)" } ?? "-")/\(call.availableWidthMode) wrap=\(wrap.map { "\($0)" } ?? "nil") -> \(size.width)x\(size.height)")
            }
            // Ceil: taffy's rounding may otherwise hand back a frame a
            // fraction shorter than the text, and SwiftUI truncates it.
            return FfiSize(
                width: call.knownWidth ?? Float(ceil(size.width)),
                height: call.knownHeight ?? Float(ceil(size.height))
            )
        }
    }

    /// Unwrapped width of one word, scaled from the full text's max-content
    /// width (the subview is the whole Text; proportional by characters is
    /// close enough for min-content and costs no extra view).
    @MainActor
    private static func textWidth(_ word: String, subview: LayoutSubview, full: String) -> CGFloat {
        let fullWidth = subview.sizeThatFits(.unspecified).width
        guard !full.isEmpty else { return 0 }
        return ceil(fullWidth * CGFloat(word.count) / CGFloat(full.count))
    }
}

/// The ONE custom Layout: every node is a flat subview; taffy computes the
/// frames, SwiftUI only places them.
struct TaffyLayout: Layout {
    let model: VappSurfaceModel
    let pressed: [String]
    let nonce: Int
    let unflipsRTL: Bool

    /// Widths below this are SwiftUI probes, never a real surface.
    static let probeWidth: CGFloat = 64

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        MainActor.assumeIsolated {
            let box = model.box
            let tag = "w=\(proposal.width.map { $0.isInfinite ? "inf" : String(format: "%.1f", $0) } ?? "nil") h=\(proposal.height.map { $0.isInfinite ? "inf" : String(format: "%.1f", $0) } ?? "nil")"
            if !box.proposalLog.contains(tag) {
                box.proposalLog.append(tag)
                print("VappSpike proposal \(tag)")
            }
            // .zero / .infinity / .unspecified, and the transient 44 pt probe
            // NavigationStack sends before the real width: answer from the
            // cache instead of paying a full FFI pass.
            guard let width = proposal.width, width.isFinite, width >= Self.probeWidth else {
                if let last = box.lastSize {
                    return CGSize(width: proposal.width.map { $0.isFinite ? $0 : last.width } ?? last.width, height: last.height)
                }
                return CGSize(width: proposal.width.map { $0.isFinite ? $0 : 390 } ?? 390, height: box.lastSize?.height ?? 0)
            }
            let result = pass(width: width, subviews: subviews)
            let size = CGSize(width: CGFloat(result.surfaceWidth), height: CGFloat(result.surfaceHeight))
            box.lastSize = size
            return size
        }
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        MainActor.assumeIsolated {
            // A zero-width placement (SwiftUI probing) would cost a full pass.
            guard bounds.width >= Self.probeWidth else { return }
            let result = pass(width: bounds.width, subviews: subviews)
            let positions = Self.positions(subviews)
            let flip = unflipsRTL && model.rtl
            for frame in result.frames {
                guard let position = positions[Int(frame.index)] else { continue }
                let w = CGFloat(frame.width)
                let h = CGFloat(frame.height)
                var x = CGFloat(frame.x)
                if flip { x = bounds.width - x - w }
                subviews[position].place(
                    at: CGPoint(x: bounds.minX + x, y: bounds.minY + CGFloat(frame.y)),
                    anchor: .topLeading,
                    proposal: ProposedViewSize(width: w, height: h)
                )
            }
        }
    }

    @MainActor
    private static func positions(_ subviews: Subviews) -> [Int: Int] {
        var map: [Int: Int] = [:]
        map.reserveCapacity(subviews.count)
        for (position, subview) in subviews.enumerated() {
            map[subview[VappIndexKey.self]] = position
        }
        return map
    }

    /// ONE FFI pass per (width, pressed, nonce); `placeSubviews` reuses it.
    @MainActor
    private func pass(width: CGFloat, subviews: Subviews) -> LayoutResult {
        let box = model.box
        let key = VappPassKey(width: Float(width), pressed: pressed, nonce: nonce)
        if box.key == key, let result = box.result {
            box.cacheHits += 1
            return result
        }
        let surface = model.surface
        let signpostID = VappSurfaceModel.signposter.makeSignpostID()
        let state = VappSurfaceModel.signposter.beginInterval("layout", id: signpostID)
        let start = DispatchTime.now().uptimeNanoseconds
        _ = surface.setViewport(width: Float(width), height: 0)
        _ = surface.setPressed(ids: pressed)
        let measure = VappSubviewMeasure(
            subviews: subviews,
            positions: Self.positions(subviews),
            nodes: model.nodes
        )
        let result = surface.layout(measure: measure)
        let wall = DispatchTime.now().uptimeNanoseconds - start
        VappSurfaceModel.signposter.endInterval("layout", state)

        if VappSubviewMeasure.logs {
            for frame in result.frames {
                print("VappSpike frame \(model.nodes[Int(frame.index)].id) x=\(frame.x) y=\(frame.y) w=\(frame.width) h=\(frame.height)")
            }
        }
        box.key = key
        box.result = result
        var frames: [Int: PlacedFrame] = [:]
        frames.reserveCapacity(result.frames.count)
        for frame in result.frames { frames[Int(frame.index)] = frame }
        box.framesByIndex = frames
        box.passes += 1
        box.lastWallNs = wall
        box.lastLayoutNs = result.layoutNs
        box.lastCalls = result.measureCalls
        box.bestWallNs = min(box.bestWallNs, wall)
        box.bestLayoutNs = min(box.bestLayoutNs, result.layoutNs)
        print("VappSpike pass=\(box.passes) width=\(width) nodes=\(model.nodes.count) measureCalls=\(result.measureCalls) layoutNs=\(result.layoutNs) wallNs=\(wall) buildNs=\(model.buildNs) height=\(result.surfaceHeight) pressed=\(pressed)")
        let model = model
        Task { @MainActor in
            model.publishPaintIfChanged()
        }
        return result
    }
}
