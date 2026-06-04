import ExpCore
import SwiftUI

struct InboxView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: InboxViewModel?
    @State private var tab = 0 // 0 = For me, 1 = Needs your review

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                VStack(spacing: 0) {
                    Picker("", selection: $tab) {
                        Text(vm.totalUnread > 0 ? "For me · \(vm.totalUnread)" : "For me").tag(0)
                        Text(vm.reviewIssues.isEmpty ? "Needs review" : "Needs review · \(vm.reviewIssues.count)").tag(1)
                    }
                    .pickerStyle(.segmented)
                    .padding()

                    if tab == 0 {
                        if vm.groups.isEmpty {
                            emptyState("You're all caught up.")
                        } else {
                            list {
                                ForEach(vm.groups) { group in
                                    NavigationLink(value: AppRoute.issue(accountId: accountId, id: group.issue.id)) {
                                        groupCard(group)
                                    }
                                    .buttonStyle(.plain)
                                    .simultaneousGesture(TapGesture().onEnded { vm.markGroupRead(group) })
                                }
                            }
                        }
                    } else {
                        if vm.reviewIssues.isEmpty {
                            emptyState("Nothing waiting on your review.")
                        } else {
                            list {
                                ForEach(vm.reviewIssues) { issue in
                                    NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                                        reviewCard(issue)
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Inbox")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            if let vm = viewModel, vm.totalUnread > 0 {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Mark all read") { vm.markAllRead() }
                }
            }
        }
        .onAppear {
            if viewModel == nil {
                let vm = InboxViewModel(
                    accountId: accountId,
                    db: deps.db,
                    auth: deps.auth,
                    notificationsApi: deps.notificationsApi
                )
                viewModel = vm
                vm.startObserving()
            }
        }
        .onDisappear { viewModel?.stopObserving() }
    }

    @ViewBuilder
    private func list<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        ScrollView {
            LazyVStack(spacing: 8) { content() }
                .padding()
        }
    }

    private func groupCard(_ group: InboxViewModel.Group) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                if group.unread > 0 {
                    Circle().fill(Color.accentColor).frame(width: 8, height: 8)
                }
                if let identifier = group.issue.identifier {
                    Text(identifier)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                Text(group.issue.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }
            ForEach(group.notifications.prefix(3), id: \.id) { n in
                Text(n.title)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .padding(.leading, 16)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private func reviewCard(_ issue: IssueEntity) -> some View {
        HStack(spacing: 8) {
            if let identifier = issue.identifier {
                Text(identifier)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Text(issue.title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            Spacer()
            Text(issue.agentPlanState == "awaiting_approval" ? "Plan" : "PR")
                .font(.caption2)
                .foregroundStyle(Color.accentColor)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private func emptyState(_ label: String) -> some View {
        VStack {
            Spacer()
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}
