import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2161 (member-space.json, web `MemberSpaceMembers`): Device settings ›
/// Sharing's `copy.members`: who runs their Assistant on this device, each
/// with `copy.remove`. Not synced to the phone: read on open, every 30 s and
/// whenever the opt-in moves (`version`); nothing while nobody does.
struct MemberSpaceMembersSection: View {
    let rowId: String?
    /// Anything that may end a space (the opt-in's teams).
    let version: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var members: [MemberSpaceMember] = []
    @State private var removing: String?

    private static let interval: Duration = .seconds(30)

    var body: some View {
        Group {
            if !members.isEmpty {
                Section {
                    ForEach(members) { member in
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
                                    if removing == member.spaceId {
                                        InlineSpinner()
                                    } else {
                                        Text(MemberSpace.Copy.remove)
                                    }
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white.opacity(removing == nil ? TextOpacity.primary : TextOpacity.quaternary))
                                .padding(.horizontal, 12)
                                .frame(minHeight: 32)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .disabled(removing != nil)
                            .accessibilityIdentifier("member-space-remove-\(member.spaceId)")
                        }
                        .accessibilityIdentifier("member-space-\(member.spaceId)")
                    }
                } header: {
                    GlassSectionHeader(MemberSpace.Copy.members)
                }
                .listRowBackground(glassFormRowFill)
            }
        }
        .task(id: "\(rowId ?? "")/\(version)") {
            while !Task.isCancelled {
                await load()
                try? await Task.sleep(for: Self.interval)
            }
        }
    }

    private func load() async {
        guard let rowId else { return }
        if let next = try? await deps.devicesApi.memberSpaces(accountId: accountId, rowId: rowId) { members = next }
    }

    private func remove(_ spaceId: String) async {
        guard removing == nil else { return }
        removing = spaceId
        defer { removing = nil }
        do {
            try await deps.devicesApi.removeMemberSpace(accountId: accountId, spaceId: spaceId)
            await load()
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "remove", thing: "the member"), description: error.userFacingMessage)
        }
    }
}
