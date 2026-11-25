import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1715 (composer-menu.json `teamsPick`): the viewer's Assistant's (and
/// #EXP-2122 a plain chat's) Teams pick, which teams a turn may touch. The "+" › Teams page = THE
/// multi picker rows over the viewer's teams (the team switcher's order:
/// the team's mark + name, the check trailing); nothing picked = every team
/// (nil). While the pick is a subset, the picked teams ride as the
/// composer's removable chips (`TeamsPickChips`). Web: `team-pick.tsx`.
struct ComposerTeamsBinding {
    /// The viewer's teams, in `ComposerMenu.pickTeams` order.
    let teams: [ComposerMenu.PickTeam]
    /// nil = every team.
    let picked: [String]?
    let onChange: ([String]?) -> Void
    /// The next send's `scopeTeamIds`; nil = left out (#EXP-2122: a plain
    /// chat's unchanged pick).
    let scope: TeamsScope?

    /// The Assistant's (and Home's) pick: every send carries it.
    init(teams: [ComposerMenu.PickTeam], picked: [String]?, onChange: @escaping ([String]?) -> Void) {
        self.teams = teams
        self.picked = picked
        self.onChange = onChange
        scope = TeamsScope(picked)
    }

    /// #EXP-2122 (`plainTeamsPick`): a plain chat's pick for `root`, its own
    /// team always in it; `onPick` takes the other teams. nil = not the
    /// viewer's plain root (an agent's, an app's, a child, another member's).
    init?(
        root: SessionEntity?, me: String?, draft: AgentSettings.TeamsPickDraft, teams: [ComposerMenu.PickTeam],
        onPick: @escaping ([String]) -> Void
    ) {
        guard let root, root.parentId == nil, root.agentId?.isEmpty ?? true, root.appId?.isEmpty ?? true,
              !root.teamId.isEmpty, root.userId == me, teams.contains(where: { $0.id == root.teamId })
        else { return nil }
        let own = root.teamId
        let pick = ComposerMenu.plainTeamsPick(
            own: own, stored: root.scopeTeamIds, picked: draft.picked(stored: root.scopeTeamIds), teams: teams
        )
        self.teams = teams
        picked = pick.shown
        scope = pick.scope
        // Every team checked again (nil) = each id stored.
        onChange = { next in onPick((next ?? teams.map(\.id)).filter { $0 != own }) }
    }

    /// The "+" Teams row's value (`teamsPickValue`).
    var value: String { ComposerMenu.teamsPickValue(picked, teams: teams) }

    /// The viewer's synced teams as pick rows.
    static func teams(_ rows: [TeamEntity]) -> [ComposerMenu.PickTeam] {
        ComposerMenu.pickTeams(rows.map { ComposerMenu.PickTeam(id: $0.id, name: $0.name, iconUrl: $0.iconUrl) })
    }
}

/// The "+" › Teams page: the multi picker's rows; the last checked team
/// cannot be unchecked (a turn always touches one); every team = nil.
struct TeamsPickPage: View {
    let binding: ComposerTeamsBinding

    var body: some View {
        let teams = binding.teams
        let byId = Dictionary(teams.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        GlassPickerContent(
            items: teams.map { team in
                PickerItem(value: team.id, label: team.name, keywords: [team.name])
            },
            mode: .multi,
            value: Set(ComposerMenu.pickedTeamIds(binding.picked, teams: teams)),
            onChange: { picked in
                let next = teams.map(\.id).filter { picked.contains($0) }
                guard !next.isEmpty else { return }
                binding.onChange(ComposerMenu.nextTeamsPick(next, teams: teams))
            },
            // The typed pickers' rule: more than six rows get the search field.
            search: teams.count > 6,
            searchPlaceholder: "Search teams…",
            renderMark: { item in
                AnyView(TeamAvatar(iconUrl: byId[item.value]?.iconUrl, size: 20).foregroundStyle(.white))
            }
        )
        .pickerCheckMarks()
        .accessibilityIdentifier("agent-composer-teams-picker")
    }
}

/// The picked teams as the composer's removable chips (a subset only): THE
/// pill chip the prompt subject wears, the team mark + name + ✕.
struct TeamsPickChips: View {
    let binding: ComposerTeamsBinding
    var disabled = false

    var body: some View {
        let chips = ComposerMenu.teamsPickChips(binding.picked, teams: binding.teams)
        if !chips.isEmpty {
            FlowLayout(spacing: 8, lineSpacing: 6) {
                ForEach(chips) { team in chip(team) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-composer-team-chips")
        }
    }

    private func chip(_ team: ComposerMenu.PickTeam) -> some View {
        GlassPill(
            team.name,
            mode: .readonly,
            leading: {
                TeamAvatar(iconUrl: team.iconUrl, size: 12)
                    .foregroundStyle(.white)
            },
            trailing: {
                AppIcon(AppIcons.uiClose, size: 10)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        )
        .overlay(alignment: .trailing) {
            // A readonly pill takes no hits: the ✕'s 28pt button sits over it.
            Button {
                binding.onChange(ComposerMenu.removingTeamChip(team.id, picked: binding.picked, teams: binding.teams))
            } label: {
                Color.clear
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(disabled)
            .accessibilityLabel("Remove \(team.name)")
            .accessibilityIdentifier("agent-composer-chip-team-\(team.id)-remove")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-chip-team-\(team.id)")
    }
}
