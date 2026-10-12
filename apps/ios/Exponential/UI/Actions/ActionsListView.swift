import ExpCore
import ExpUI
import SwiftUI

/// The Actions surface (EXP-253): the action prompts of EVERY member team
/// (EXP-1186: cross-team like the Inbox — one band per team, `TeamAvatar` +
/// name, once the caller is in more than one), each with a Run affordance
/// that opens the composer on the action's OWN team. EXP-825: Run and "New
/// action" (EXP-431, in the web-parity "Actions" section header since
/// EXP-574) are NAVIGATION into the Agent page composer, seeded with the
/// action (or the Create action builtin). EXP-1270: the Suggestions segment
/// and its seed cards are gone ×4, so the surface is the list alone. SLOP-2: a row
/// tap (and the row menu's "Edit") pushes the ACTION PAGE (`ActionDetailView`:
/// Prompt · Triggers · Runs) — it replaced the edit sheet, and the Automations
/// segment with it: an action carries its triggers, shown here as the glyphs
/// beside its name.
struct ActionsListView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: ActionsViewModel?
    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                content(vm)
            }
        }
        .navigationTitle("Actions")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // Reload when the active team changes (and on first mount).
        .task(id: teamState.activeTeam?.id) {
            ensureViewModel()
            if let teamId = teamState.activeTeam?.id {
                await viewModel?.load(teamId: teamId)
            }
        }
        .onAppear {
            ensureViewModel()
        }
    }

    private func ensureViewModel() {
        if viewModel == nil {
            viewModel = ActionsViewModel(
                accountId: accountId,
                db: deps.db,
                auth: deps.auth
            )
        }
    }

    /// EXP-825: every launch here is a push into the Agent page composer.
    /// EXP-1186: a seed naming a team opens THAT team's composer.
    private func openComposer(_ seed: AgentComposerSeed) {
        guard seed.teamId != nil || teamState.activeTeam != nil else { return }
        pushRoute(.agent(accountId: accountId, seed: seed))
    }

    /// The action page, on its Prompt tab.
    private func openAction(_ action: ActionDto) {
        pushRoute(.action(accountId: accountId, id: action.id))
    }

    // MARK: - Content

    @ViewBuilder
    private func content(_ vm: ActionsViewModel) -> some View {
        VStack(spacing: 0) { actionsContent(vm) }
    }

    @ViewBuilder
    private func actionsContent(_ vm: ActionsViewModel) -> some View {
        if vm.actions.isEmpty, vm.isLoading {
            // EXP-1275: the first load draws skeleton rows in the list's own
            // place, not a centred spinner.
            SkeletonRows()
                .padding()
            Spacer()
        } else if vm.actions.isEmpty {
            Spacer()
            emptyState
            Spacer()
        } else {
            ScrollView {
                // EXP-818: a filled group BAND over flat rows — the list reads
                // as a table instead of a stack of cards (web `ListRow`,
                // desktop `surface::flat_row`).
                if TeamGroups.isMultiTeam(teamState.teams) {
                    // EXP-1186: one band per team — its avatar and name lead,
                    // "New action" creates in THAT team.
                    LazyVStack(alignment: .leading, spacing: 12) {
                        ForEach(
                            TeamGroups.group(vm.actions, teams: teamState.teams) { $0.teamId }
                        ) { group in
                            VStack(alignment: .leading, spacing: 0) {
                                GlassSectionBand(group.team.name) {
                                    TeamAvatar(team: group.team, size: 16)
                                } trailing: {
                                    newActionButton(teamId: group.team.id)
                                }
                                ForEach(group.items) { actionRow($0) }
                            }
                            .accessibilityElement(children: .contain)
                            .accessibilityIdentifier("actions-team-\(group.team.id)")
                        }
                    }
                    .padding()
                } else {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        // EXP-574 (web parity): the "Actions" band with the
                        // "New action" entry (EXP-431) as its trailing control.
                        GlassSectionBand("Actions") {
                            newActionButton(teamId: nil)
                        }
                        ForEach(vm.actions) { actionRow($0) }
                    }
                    .padding()
                }
            }
            // Actions is a tab of its own since EXP-686 — reserve the
            // floating bar's clearance (EXP-36).
            .tabBarBottomInset()
        }
    }

    /// EXP-431: creation left the list ("Create action" no longer poses as a
    /// row); EXP-825: it is the composer with the Create action builtin
    /// picked — describe it, and the creator run writes it.
    private func newActionButton(teamId: String?) -> some View {
        GlassPill(
            "New action",
            icon: AppIcons.actionCreate,
            mode: .action {
                openComposer(AgentComposerSeed(
                    actionId: DomainContract.builtinCreateActionId, teamId: teamId
                ))
            },
            enabled: teamId != nil || teamState.activeTeam != nil
        )
        .accessibilityLabel("New action")
    }

    private var emptyState: some View {
        EmptyState(.actions)
    }

    private func actionRow(_ action: ActionDto) -> some View {
        HStack(spacing: 12) {
            // SLOP-2: the row's body opens the action page. Its own Button,
            // BESIDE the play and menu controls — a control nested in a
            // button's label has its tap swallowed (the ×4 rule).
            Button {
                openAction(action)
            } label: {
                HStack(spacing: 12) {
                    // EXP-273: the action's own curated glyph, falling back to
                    // the generic action mark — also for a name only a newer
                    // build ships, which would otherwise draw nothing.
                    AppIcon(ActionIconDisplay.iconName(for: action.icon), size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))

                    VStack(alignment: .leading, spacing: 3) {
                        // EXP-697: no repo glyph beside the name — the row says
                        // what the action is, not where it runs. SLOP-2: its
                        // trigger glyphs sit there instead.
                        HStack(spacing: 6) {
                            Text(action.name)
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            TriggerGlyphs(badges: TriggerBadges.of(action.triggers))
                        }
                        if let description = action.description, !description.isEmpty {
                            Text(description)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(2)
                                .multilineTextAlignment(.leading)
                        }
                    }

                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("action-open")

            // EXP-615: the play glyph, not a "Run" pill — the same affordance
            // web and desktop wear on their action cards. EXP-825: it pushes
            // the composer with this action picked.
            CircleIconButton(AppIcons.actionRun, accessibilityLabel: "Run") {
                // EXP-1186: on the action's OWN team (the list is cross-team).
                openComposer(AgentComposerSeed(actionId: action.id, teamId: action.teamId))
            }

            // EXP-694: non-owners reach the page too — read-only there.
            GlassMenu {
                // EXP-858: no Pin row — the phone has no sidebar.
                GlassMenuItem("Edit", icon: AppIcons.uiEdit) {
                    openAction(action)
                }
            } label: {
                GhostIconLabel(AppIcons.uiMore)
            }
            .accessibilityLabel("Action actions")
            .accessibilityIdentifier("action-menu")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        // EXP-986: a container of its own, or SwiftUI hands the identifier to
        // every child and the store lane's pop rect (PopRects.swift, first
        // match) measured the icon instead of the row.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("action-row")
    }
}
