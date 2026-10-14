import ExpCore
import ExpUI
import SwiftUI

/// EXP-1283: THE Guide picture viewer, phone shape (web `ImagePreviewDialog`
/// `panel`, desktop `image_preview`, Android `ResultPreviewDialog`): a
/// full-screen page — the picture on top fit to the width (a tall capture
/// scrolls), the panel under it (the topic, the picture's label pill, the
/// topic's GFM text), `index / count` at the foot, a swipe between every
/// picture of the Guide in the Guide's order, × top-right. The order and
/// the panel are `GuideViewer.state` (ExpCore, fixture-locked ×4).
struct GuidePictureViewer: View {
    /// The synced `sessions.guide` jsonb text.
    let guide: String?
    let startId: String
    var teamId: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var selection: String
    @State private var pageWidth: CGFloat = 0

    init(guide: String?, startId: String, teamId: String? = nil) {
        self.guide = guide
        self.startId = startId
        self.teamId = teamId
        _selection = State(initialValue: startId)
    }

    /// Every Guide picture id in viewer order (walked off `prevId`/`nextId`).
    private var ids: [String] {
        guard let start = GuideViewer.state(guide: guide, attachmentId: startId) else { return [startId] }
        var before: [String] = []
        var cursor = start.prevId
        while let id = cursor, before.count < 200 {
            before.insert(id, at: 0)
            cursor = GuideViewer.state(guide: guide, attachmentId: id)?.prevId
        }
        var after: [String] = []
        cursor = start.nextId
        while let id = cursor, after.count < 200 {
            after.append(id)
            cursor = GuideViewer.state(guide: guide, attachmentId: id)?.nextId
        }
        return before + [startId] + after
    }

    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            issueRefs: teamId.map { teamId in
                AgentIssueRefContext(
                    teamId: teamId,
                    db: deps.db,
                    onOpen: { [deps, accountId] issueId in
                        deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                    }
                )
            }
        )
    }

    var body: some View {
        let ids = ids
        ZStack {
            AppBackground()
            TabView(selection: $selection) {
                ForEach(ids, id: \.self) { id in
                    page(id)
                        .tag(id)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
                pageWidth = width
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            HStack {
                Spacer()
                FloatingBarCircle(accessibilityLabel: "Close", action: { dismiss() }) {
                    AppIcon(AppIcons.uiClose, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .accessibilityIdentifier("guide-picture-viewer-close")
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            footer(ids)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("guide-picture-viewer")
    }

    @ViewBuilder
    private func page(_ id: String) -> some View {
        let state = GuideViewer.state(guide: guide, attachmentId: id)
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if let entry = sessionResultPicture(guide, attachmentId: id) {
                    StripImageView(entry: entry, width: pageWidth)
                        .frame(maxWidth: .infinity)
                        .accessibilityLabel("\(entry.topic), \(entry.label)")
                }
                if let state {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(state.topic)
                            .font(.title2.weight(.semibold))
                            .foregroundStyle(.white)
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("guide-picture-viewer-topic")
                        Text(state.label)
                            .font(.callout.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 4)
                            .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                            .overlay(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .strokeBorder(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
                            )
                            .accessibilityIdentifier("guide-picture-viewer-label")
                        if let text = state.text {
                            AgentMarkdownText(
                                text: text,
                                context: markdownContext,
                                options: [.autolinkBareURLs],
                                overrides: AgentMarkdownText.guideReport
                            )
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.top, 4)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 24)
                    .padding(.bottom, 24)
                }
            }
        }
    }

    /// `index / count` with one dot per picture (the shown one lit).
    private func footer(_ ids: [String]) -> some View {
        let index = (ids.firstIndex(of: selection) ?? 0) + 1
        return VStack(spacing: 10) {
            Text("\(index) / \(ids.count)")
                .font(.body.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .accessibilityIdentifier("guide-picture-viewer-index")
            if ids.count > 1, ids.count <= 12 {
                HStack(spacing: 8) {
                    ForEach(ids, id: \.self) { id in
                        Circle()
                            .fill(.white.opacity(id == selection ? 0.9 : 0.25))
                            .frame(width: 7, height: 7)
                    }
                }
                .accessibilityHidden(true)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 12)
    }
}
