import ExpCore
import ExpUI
import SwiftUI

/// EXP-1284: the thread's CHILD STRIP as the phone's pager tabs (brief §1,
/// draft 3; it replaces the Work screen's Issue · Run · Guide): the
/// `ThreadViews.build` strip in order — `Conversation`, one tab per linked
/// issue (status glyph + mono identifier), `Guide` (a green dot while the
/// thread's Guide has entries) — scrolling sideways when it outgrows the
/// width, with the "+" as a trailing button outside the scroller. The shown
/// tab is the filled capsule. A long press on an issue tab opens that
/// child's menu (Link an issue · New issue in this thread · Start a session here
/// · Unlink). A child linked while the screen was up fades in with a ring.
struct ThreadStripTabs: View {
    let strip: [ThreadChild]
    let selection: ThreadPage
    /// The synced issue behind a tab (its status glyph); nil = anchor default.
    var issue: (String) -> IssueEntity? = { _ in nil }
    /// Whether an issue tab was just linked (the fade-in ring, once).
    var isFresh: (String) -> Bool = { _ in false }
    let onSelect: (ThreadPage) -> Void
    /// A long press on an issue tab: its id and the tab's global frame.
    var onIssueMenu: (String, CGRect) -> Void = { _, _ in }
    /// The "+" (nil = hidden: a reader who cannot add to the thread).
    var onAdd: ((CGRect) -> Void)?

    @State private var frames: [String: CGRect] = [:]
    @State private var addFrame: CGRect = .zero

    var body: some View {
        HStack(spacing: 4) {
            ScrollViewReader { scroller in
                ScrollView(.horizontal) {
                    HStack(spacing: 2) {
                        ForEach(tabs, id: \.self) { page in
                            tab(page)
                                .id(page)
                        }
                    }
                    .padding(.horizontal, 12)
                }
                .scrollIndicators(.hidden)
                .onChange(of: selection) { _, next in
                    scroller.scrollTo(next, anchor: .center)
                }
            }
            if let onAdd {
                Button {
                    onAdd(addFrame)
                } label: {
                    AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add to this thread")
                .accessibilityIdentifier("thread-strip-add")
                .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                    addFrame = frame
                }
                .padding(.trailing, 8)
            }
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-strip")
    }

    /// The pages in strip order (the "+" is a button, never a page).
    private var tabs: [ThreadPage] {
        strip.compactMap { child in
            switch child {
            case .conversation: .conversation
            case let .issue(issueId, _, _): .issue(issueId)
            case .guide: .guide
            case .add: nil
            }
        }
    }

    private func child(for page: ThreadPage) -> ThreadChild? {
        strip.first { child in
            switch (child, page) {
            case (.conversation, .conversation), (.guide, .guide): true
            case let (.issue(id, _, _), .issue(pageId)): id == pageId
            default: false
            }
        }
    }

    @ViewBuilder
    private func tab(_ page: ThreadPage) -> some View {
        let selected = page == selection
        switch child(for: page) {
        case let .issue(issueId, identifier, title):
            ThreadStripTab(selected: selected, fresh: isFresh(issueId)) {
                HStack(spacing: 6) {
                    let status = IssueStatus.from(issue(issueId)?.status)
                    AppIcon(status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(status.color)
                    Text(identifier)
                        .font(.system(.footnote, design: .monospaced))
                }
            }
            .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                frames[issueId] = frame
            }
            .onTapGesture { onSelect(page) }
            .onLongPressGesture(minimumDuration: 0.4) {
                onIssueMenu(issueId, frames[issueId] ?? .zero)
            }
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel("\(identifier) \(title)")
            .accessibilityAction(named: "More") {
                onIssueMenu(issueId, frames[issueId] ?? .zero)
            }
            .accessibilityIdentifier("thread-tab-\(identifier)")
        case let .guide(dot):
            ThreadStripTab(selected: selected, fresh: false) {
                HStack(spacing: 5) {
                    Text(ThreadViews.guideLabel)
                        .font(.footnote.weight(.medium))
                    if dot {
                        Circle()
                            .fill(DesignTokens.Semantic.green)
                            .frame(width: 6, height: 6)
                            .accessibilityHidden(true)
                    }
                }
            }
            .onTapGesture { onSelect(page) }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("thread-tab-guide")
        default:
            ThreadStripTab(selected: selected, fresh: false) {
                Text(ThreadViews.conversationLabel)
                    .font(.footnote.weight(.medium))
            }
            .onTapGesture { onSelect(page) }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("thread-tab-conversation")
        }
    }
}

/// One tab: the filled capsule when shown, plain text otherwise; a FRESH tab
/// fades in (motion tokens, decelerate) with a ring that settles away.
private struct ThreadStripTab<Content: View>: View {
    let selected: Bool
    let fresh: Bool
    @ViewBuilder let content: () -> Content

    @Environment(\.motion) private var motion
    @State private var appeared = false
    @State private var ringed = false

    var body: some View {
        content()
            .foregroundStyle(.white.opacity(selected ? TextOpacity.primary : TextOpacity.secondary))
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background {
                if selected {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(GlassTokens.fillCard)
                        .overlay(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                        )
                }
            }
            .overlay {
                if fresh {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .strokeBorder(DesignTokens.Semantic.blue, lineWidth: 1)
                        .opacity(ringed ? 0 : 1)
                }
            }
            .opacity(fresh && !appeared ? 0 : 1)
            .contentShape(Rectangle())
            .onAppear {
                guard fresh else { return }
                withAnimation(motion.decelerate(DesignTokens.Motion.Duration.slow)) { appeared = true }
                Task { @MainActor in
                    try? await Task.sleep(for: .seconds(2))
                    withAnimation(motion.standard) { ringed = true }
                }
            }
    }
}

/// EXP-1284: the thread header's title row (brief §1): the title with the
/// muted `thread` word after it, and the header caption (`2 runs · 2
/// devices`) under it on phones, where the strip takes the full width.
struct ThreadHeaderTitle: View {
    let title: String
    let caption: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(title)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.white)
                    .lineLimit(2)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("thread-title")
                Text(ThreadViews.threadWord)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize()
            }
            if !caption.isEmpty {
                Text(caption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .accessibilityIdentifier("thread-header-caption")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
    }
}
