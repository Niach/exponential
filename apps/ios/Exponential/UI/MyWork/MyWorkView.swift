import ExpCore
import ExpUI
import SwiftUI

/// "My Work" (EXP-58): Inbox and My Issues merged into one project-independent
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
                segmentControl
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

    // Glass-pill segmented control — same pill language as MobileTabBar.
    private var segmentControl: some View {
        HStack(spacing: 4) {
            ForEach(Segment.allCases, id: \.rawValue) { seg in
                segmentButton(seg)
            }
        }
        .padding(4)
        .background(.ultraThinMaterial, in: Capsule())
        .overlay(
            Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
        )
    }

    private func segmentButton(_ seg: Segment) -> some View {
        let active = segment == seg
        return Button {
            segmentRaw = seg.rawValue
        } label: {
            HStack(spacing: 6) {
                Text(seg.label)
                    .font(.subheadline.weight(active ? .semibold : .regular))
                    .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                if seg == .inbox, let unread = inboxViewModel?.totalUnread, unread > 0 {
                    Text("\(unread)")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Accent.indigo, in: Capsule())
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 7)
            .background(active ? Color.white.opacity(0.12) : .clear, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(seg.label)
    }
}
