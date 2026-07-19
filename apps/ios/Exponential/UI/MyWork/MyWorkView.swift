import ExpCore
import ExpUI
import SwiftUI

/// "My Work" (EXP-58): Inbox and My Issues merged into one board-independent
/// bottom-bar destination — the web UI's inbox + my-issues pairing — behind a
/// glass-pill segmented control. The Inbox segment carries the unread count
/// and hosts Mark all read; the segment choice survives relaunch via
/// AppStorage. Search stays a pure search surface (its former embedded
/// "Assigned to you" list lives here now).
struct MyWorkView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var inboxViewModel: InboxViewModel?
    @AppStorage("myWorkSegment") private var segmentRaw = Segment.inbox.rawValue

    // Reviews (EXP-147) and Support (EXP-180) each moved out to their own
    // bottom-bar destinations; a persisted "reviews"/"support" rawValue falls
    // back to .inbox via the `segment` computed property.
    private enum Segment: String, CaseIterable {
        case inbox
        case myIssues

        var label: String {
            switch self {
            case .inbox: return "Inbox"
            case .myIssues: return "My Issues"
            }
        }
    }

    private var segment: Segment {
        Segment(rawValue: segmentRaw) ?? .inbox
    }

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                GlassSegmentedControl(
                    options: Segment.allCases,
                    selection: segment,
                    label: { $0.label },
                    badge: { $0 == .inbox ? (inboxViewModel?.totalUnread ?? 0) : 0 },
                    onSelect: { segmentRaw = $0.rawValue }
                )
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

                switch segment {
                case .inbox:
                    if let vm = inboxViewModel {
                        InboxListContent(viewModel: vm)
                    } else {
                        Color.clear
                    }
                case .myIssues:
                    MyIssuesListContent()
                }
            }
        }
        .navigationTitle("My Work")
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
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            inboxViewModel?.startObserving()
        }
        .onDisappear { inboxViewModel?.stopObserving() }
    }
}
