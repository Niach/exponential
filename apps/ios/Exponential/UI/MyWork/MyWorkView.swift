import ExpCore
import ExpUI
import SwiftUI

/// "My Work" (EXP-58): Inbox and My issues merged into one board-independent
/// bottom-bar destination — the web UI's inbox + my-issues pairing — behind a
/// glass-pill segmented control. The Inbox segment carries the unread count
/// and hosts Mark all read; the segment choice survives relaunch via
/// AppStorage. Search stays a pure search surface (its former embedded
/// "Assigned to you" list lives here now).
struct MyWorkView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var inboxViewModel: InboxViewModel?
    /// EXP-878: drafts are observed here, not inside the segment — the segment
    /// only exists while there is at least one resolvable draft.
    @State private var draftsViewModel: DraftsViewModel?
    @AppStorage("myWorkSegment") private var segmentRaw = Segment.inbox.rawValue

    // Reviews (EXP-147) and Support (EXP-180) each moved out to their own
    // bottom-bar destinations; a persisted "reviews"/"support" rawValue falls
    // back to .inbox via the `segment` computed property.
    private enum Segment: String, CaseIterable {
        case inbox
        case myIssues
        /// EXP-878: unfiled issues, account-wide. LABEL-ONLY, like its two
        /// siblings — the segmented control carries no icons.
        case drafts

        var label: String {
            switch self {
            case .inbox: return "Inbox"
            case .myIssues: return "My issues"
            case .drafts: return "Drafts"
            }
        }
    }

    private var hasDrafts: Bool {
        !(draftsViewModel?.rows.isEmpty ?? true)
    }

    /// The segment's plain trailing count: unread notifications for Inbox,
    /// resolvable drafts for Drafts, none for My issues.
    private func segmentCount(_ option: Segment) -> Int {
        switch option {
        case .inbox: return inboxViewModel?.totalUnread ?? 0
        case .myIssues: return 0
        case .drafts: return draftsViewModel?.rows.count ?? 0
        }
    }

    /// Drafts appear only once there is one to show.
    private var segments: [Segment] {
        hasDrafts ? Segment.allCases : [.inbox, .myIssues]
    }

    private var segment: Segment {
        let stored = Segment(rawValue: segmentRaw) ?? .inbox
        // A persisted "drafts" pick survives the last draft being filed or
        // deleted — fall back rather than render an empty segment.
        return segments.contains(stored) ? stored : .inbox
    }

    /// The pager's selection: a drag onto a page writes the stored segment.
    private var selection: Binding<Segment> {
        Binding(get: { segment }, set: { segmentRaw = $0.rawValue })
    }

    /// A segment tap slides the pages.
    private func selectSegment(_ next: Segment) {
        guard next != segment else { return }
        withAnimation { segmentRaw = next.rawValue }
    }

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                GlassSegmentedControl(
                    options: segments,
                    selection: segment,
                    label: { $0.label },
                    // Polish round (pinned ×3): label + a PLAIN trailing
                    // count — unread for Inbox, drafts for Drafts — never a
                    // filled badge, never an icon.
                    accessory: { option in
                        let count = segmentCount(option)
                        guard count > 0 else { return nil }
                        return AnyView(
                            Text("\(count)")
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .monospacedDigit()
                                .lineLimit(1)
                        )
                    },
                    onSelect: selectSegment
                )
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

                // EXP-1190: the segments are PAGES that follow the finger,
                // like the action page's tabs.
                FacePager(pages: segments, selection: selection) { page in
                    switch page {
                    case .inbox:
                        if let vm = inboxViewModel {
                            InboxListContent(viewModel: vm)
                        } else {
                            Color.clear
                        }
                    case .myIssues:
                        MyIssuesListContent()
                    case .drafts:
                        if let vm = draftsViewModel {
                            DraftsListContent(viewModel: vm)
                        } else {
                            Color.clear
                        }
                    }
                }
            }
        }
        .navigationTitle("Inbox")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            if segment == .inbox, let vm = inboxViewModel, vm.totalUnread > 0 {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Mark all read") { vm.markAllRead() }
                }
            }
        }
        .onAppear {
            if inboxViewModel == nil {
                inboxViewModel = InboxViewModel(
                    accountId: accountId,
                    db: deps.db,
                    auth: deps.auth,
                    notificationsApi: deps.notificationsApi
                )
            }
            if draftsViewModel == nil {
                draftsViewModel = DraftsViewModel(
                    accountId: accountId,
                    db: deps.db,
                    api: deps.issueDraftsApi
                )
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            inboxViewModel?.startObserving()
            draftsViewModel?.startObserving()
        }
        .onDisappear {
            inboxViewModel?.stopObserving()
            draftsViewModel?.stopObserving()
        }
    }
}
