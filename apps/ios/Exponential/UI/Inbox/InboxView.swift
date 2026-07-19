import ExpCore
import ExpUI
import SwiftUI

/// Linear-style single activity stream: one row per issue group — plus one
/// synthetic Support group per team for issue-less support_reply rows
/// (EXP-180 helpdesk) — showing the latest notification's sentence (titles
/// are already full human sentences — no composition), a circular type-icon
/// badge leading, relative time + an unread dot trailing. Fully-read groups
/// render dimmed. Tapping a row marks its group read and opens the issue
/// (Support rows open the Support tab). Open-PR review triage moved to the
/// web/desktop Reviews surfaces.
///
/// EXP-58: no background or navigation chrome of its own — it renders
/// embedded as the My Work tab's Inbox segment; MyWorkView owns the view
/// model (segment control needs the unread count) and its observation
/// lifecycle.
struct InboxListContent: View {
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    let viewModel: InboxViewModel

    var body: some View {
        if viewModel.entries.isEmpty {
            emptyState("You're all caught up.")
        } else {
            ScrollView {
                LazyVStack(spacing: 8) {
                    ForEach(viewModel.entries) { entry in
                        switch entry {
                        case .issue(let group):
                            NavigationLink(value: AppRoute.issue(accountId: accountId, id: group.issue.id)) {
                                streamRow(group)
                            }
                            .buttonStyle(.plain)
                            .simultaneousGesture(TapGesture().onEnded { viewModel.markGroupRead(group) })
                        case .support(let group):
                            // Support groups open the Support tab (tickets are
                            // team-level, not per-issue). Point the active team
                            // at the group's team first — the tab renders the
                            // ACTIVE team's helpdesk inbox; a generic group
                            // (unknown team) just opens the current team's.
                            NavigationLink(value: AppRoute.support) {
                                supportRow(group)
                            }
                            .buttonStyle(.plain)
                            .simultaneousGesture(TapGesture().onEnded {
                                if let teamId = group.teamId {
                                    teamState.activeTeamId = teamId
                                }
                                viewModel.markSupportGroupRead(group)
                            })
                        }
                    }
                }
                .padding()
            }
            // Clearance for the floating tab bar (EXP-36) — the last
            // row was fully hidden under it before.
            .tabBarBottomInset()
        }
    }

    private func streamRow(_ group: InboxViewModel.Group) -> some View {
        let unread = group.unread > 0
        return HStack(alignment: .top, spacing: 10) {
            // Circular type-icon badge (no actor avatar — notifications carry
            // no actor column; the sentence names the actor).
            Image(systemName: typeIcon(group.latest?.type))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 28, height: 28)
                .background(Color.white.opacity(0.08), in: Circle())

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    if let identifier = group.issue.identifier {
                        Text(identifier)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    Text(group.issue.title)
                        .font(.subheadline.weight(unread ? .semibold : .regular))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                }
                if let latest = group.latest {
                    Text(latest.title)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 8)

            HStack(spacing: 6) {
                if let latest = group.latest {
                    Text(relativeDate(latest.createdAt))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                if unread {
                    Circle()
                        .fill(Accent.indigo)
                        .frame(width: 8, height: 8)
                }
            }
            .padding(.top, 3)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
        .opacity(unread ? 1 : 0.6)
    }

    /// Synthetic Support group row (EXP-180): same stream anatomy as the issue
    /// rows — lifepreserver badge, "Support" headline (plus the team name when
    /// the user is in more than one team, web parity), latest sentence,
    /// relative time + unread dot.
    private func supportRow(_ group: InboxViewModel.SupportGroup) -> some View {
        let unread = group.unread > 0
        return HStack(alignment: .top, spacing: 10) {
            Image(systemName: "lifepreserver")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 28, height: 28)
                .background(Color.white.opacity(0.08), in: Circle())

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text("Support")
                        .font(.subheadline.weight(unread ? .semibold : .regular))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if let teamName = group.teamName, viewModel.hasMultipleTeams {
                        Text(teamName)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                }
                if let latest = group.latest {
                    Text(latest.title)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 8)

            HStack(spacing: 6) {
                if let latest = group.latest {
                    Text(relativeDate(latest.createdAt))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                if unread {
                    Circle()
                        .fill(Accent.indigo)
                        .frame(width: 8, height: 8)
                }
            }
            .padding(.top, 3)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
        .opacity(unread ? 1 : 0.6)
    }

    /// Locked cross-platform type → icon mapping (SF Symbol column).
    private func typeIcon(_ type: String?) -> String {
        switch type {
        case DomainContract.notificationTypeIssueCreated:
            return "tray.and.arrow.down"
        case DomainContract.notificationTypeIssueAssigned:
            return "person.badge.plus"
        case DomainContract.notificationTypeIssueComment, DomainContract.notificationTypeIssueMention:
            return "text.bubble"
        case DomainContract.notificationTypeIssueStatusChanged:
            return "record.circle"
        case DomainContract.notificationTypePrOpened:
            return "arrow.triangle.branch"
        case DomainContract.notificationTypePrMerged:
            return "arrow.triangle.merge"
        default:
            return "bell"
        }
    }

    private func relativeDate(_ s: String) -> String {
        let isoFormatter = ISO8601DateFormatter()
        isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let date = isoFormatter.date(from: s) ?? ISO8601DateFormatter().date(from: s)
        guard let date else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
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
