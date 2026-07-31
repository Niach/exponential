import ExpCore
import ExpUI
import SwiftUI

/// About → Third-party licenses (EXP-262): the bundled NOTICES.txt, split
/// into sections and rendered one `Text` per section inside a `LazyVStack` —
/// a single `Text` over the whole ~40 KB blob would re-run a full TextKit
/// layout on every geometry change. Plain text on purpose: licence bodies are
/// full of `*`/`#`/`---` a markdown renderer would mangle, and altering
/// reproduced text is exactly what these licences forbid.
struct ThirdPartyLicensesView: View {
    private enum LoadState {
        case loading
        case missing
        case ready([ThirdPartyNotices.Section])
    }

    @State private var state: LoadState = .loading

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    Text("Exponential is built with open-source software. These licenses cover the components bundled in this build.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 4)

                    switch state {
                    case .loading:
                        EmptyView()
                    case .missing:
                        Text("The bundled notices file is missing from this build.")
                            .font(.caption)
                            .foregroundStyle(.orange.opacity(0.85))
                            .padding(.horizontal, 4)
                    case .ready(let sections):
                        ForEach(Array(sections.enumerated()), id: \.offset) { _, section in
                            sectionBlock(section)
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
        }
        .navigationTitle("Third-party licenses")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task {
            guard case .loading = state else { return }
            // File read + split off the main actor; the repo's first
            // `Bundle.main.url(forResource:)` call site.
            let sections = await Task.detached(priority: .utility) {
                ThirdPartyNotices.load().map(ThirdPartyNotices.parse)
            }.value
            state = sections.map(LoadState.ready) ?? .missing
        }
    }

    @ViewBuilder
    private func sectionBlock(_ section: ThirdPartyNotices.Section) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if !section.title.isEmpty {
                Text(section.title)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
            }
            Text(section.body)
                .font(.caption2.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }
}
