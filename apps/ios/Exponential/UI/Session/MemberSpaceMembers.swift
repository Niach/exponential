import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2161: the members with a space on one device. Not synced to the
/// phone: read on open, every 30 s and whenever the opt-in moves (the host's
/// `.task(id:)` restarts `run`). Owned by the sheet, whose Form always
/// appears: an empty section never runs a task of its own.
@MainActor
@Observable
final class MemberSpaceMembersModel {
    private(set) var members: [MemberSpaceMember] = []
    var removing: String?

    private static let interval: Duration = .seconds(30)

    func run(deps: AppDependencies, accountId: String, rowId: String?) async {
        while !Task.isCancelled {
            await load(deps: deps, accountId: accountId, rowId: rowId)
            try? await Task.sleep(for: Self.interval)
        }
    }

    func load(deps: AppDependencies, accountId: String, rowId: String?) async {
        guard let rowId else { return }
        if let next = try? await deps.devicesApi.memberSpaces(accountId: accountId, rowId: rowId) { members = next }
    }
}

/// #EXP-2161 (member-space.json, web `MemberSpaceMembers`): Device settings ›
/// Sharing's `copy.members`: who runs their Assistant on this device, each
/// with `copy.remove`; nothing while nobody does.
struct MemberSpaceMembersSection: View {
    let model: MemberSpaceMembersModel
    let rowId: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    var body: some View {
        if !model.members.isEmpty {
            Section {
                ForEach(model.members) { member in
                    HStack(spacing: 12) {
                        Text(member.name)
                            .foregroundStyle(.white.opacity(TextOpacity.primary))
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 8)
                        Button {
                            Task { await remove(member.spaceId) }
                        } label: {
                            Group {
                                if model.removing == member.spaceId {
                                    InlineSpinner()
                                } else {
                                    Text(MemberSpace.Copy.remove)
                                }
                            }
                            .font(.expSubheadline.weight(.medium))
                            .foregroundStyle(.white.opacity(model.removing == nil ? TextOpacity.primary : TextOpacity.quaternary))
                            .padding(.horizontal, 12)
                            .frame(minHeight: 32)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(model.removing != nil)
                        .accessibilityIdentifier("member-space-remove-\(member.spaceId)")
                    }
                }
            } header: {
                GlassSectionHeader(MemberSpace.Copy.members)
            }
            .listRowBackground(glassFormRowFill)
        }
    }

    private func remove(_ spaceId: String) async {
        guard model.removing == nil else { return }
        model.removing = spaceId
        defer { model.removing = nil }
        do {
            try await deps.devicesApi.removeMemberSpace(accountId: accountId, spaceId: spaceId)
            await model.load(deps: deps, accountId: accountId, rowId: rowId)
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "remove", thing: "the member"), description: error.userFacingMessage)
        }
    }
}
