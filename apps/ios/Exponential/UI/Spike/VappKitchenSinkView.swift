import ExpUI
import SwiftUI

/// VAPP-4 spike screen: the shared fixture (or the 200-node bench tree) laid
/// out by the Rust taffy core and painted with ExpUI primitives.
struct VappKitchenSinkView: View {
    static let coordinateSpace = "vapp-surface"

    let bench: Int?
    @State private var model: VappSurfaceModel
    @State private var caption = ""
    @State private var a11yDump = ""

    init(bench: Int?) {
        self.bench = bench
        _model = State(initialValue: VappSurfaceModel(bench: bench, rtl: VappSpikeLaunch.rtl))
    }

    var body: some View {
        ScrollViewReader { reader in
        ScrollView {
            TaffyLayout(
                model: model,
                pressed: model.pressed.sorted(),
                nonce: model.nonce,
                unflipsRTL: VappSpikeLaunch.unflipsRTL
            ) {
                ForEach(model.nodes) { node in
                    VappNodeView(node: node, model: model)
                        .layoutValue(key: VappIndexKey.self, value: node.index)
                }
            }
            .coordinateSpace(name: Self.coordinateSpace)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("vapp-surface")
            Color.clear.frame(height: 1).id("vapp-bottom")
        }
        .task {
            // Capture aid: `-vappScrollToBottom` shows the lower half.
            if ProcessInfo.processInfo.arguments.contains("-vappScrollToBottom") {
                try? await Task.sleep(for: .milliseconds(600))
                reader.scrollTo("vapp-bottom", anchor: .bottom)
            }
        }
        }
        .environment(\.layoutDirection, model.rtl ? .rightToLeft : .leftToRight)
        .background(DesignTokens.Palette.background)
        .safeAreaInset(edge: .top, spacing: 0) {
            HStack { Spacer(); benchBar }
        }
        .navigationTitle(bench == nil ? "Kitchen sink" : "Bench \(bench ?? 0)")
        .navigationBarTitleDisplayMode(.inline)
        .onReceive(Timer.publish(every: 0.25, on: .main, in: .common).autoconnect()) { _ in
            let next = model.caption
            if next != caption { caption = next }
        }
    }

    private var benchBar: some View {
        HStack(spacing: 8) {
            Text(caption)
                .font(.caption.monospacedDigit())
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .accessibilityIdentifier("vapp-bench-caption")
                .accessibilityLabel(caption)
                // The Surface build time of the live surface, for the bench test.
                .accessibilityValue("build \(model.buildNs / 1000) µs")
            // Cold: a fresh Surface, every leaf measured again.
            // Dumps the VoiceOver-walk order (VappA11yDump) into the label
            // of this button, so a UI test can read it back.
            Button("A11y") {
                let labels = VappA11yDump.labels()
                let data = (try? JSONSerialization.data(withJSONObject: labels)) ?? Data()
                a11yDump = String(data: data, encoding: .utf8) ?? "[]"
                print("VappSpike a11y \(a11yDump)")
            }
            .font(.caption.weight(.medium))
            .accessibilityIdentifier("vapp-a11y-dump")
            .accessibilityValue(a11yDump)
            Button("Re-layout") { model.rebuild() }
                .font(.caption.weight(.medium))
                .accessibilityIdentifier("vapp-relayout")
            // Warm: same Surface, a forced pass (taffy's cache answers).
            Button("Warm") { model.nonce += 1 }
                .font(.caption.weight(.medium))
                .accessibilityIdentifier("vapp-relayout-warm")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .background(.ultraThinMaterial, in: Capsule())
        .padding(.trailing, 8)
        .padding(.bottom, 4)
        .environment(\.layoutDirection, .leftToRight)
    }
}
