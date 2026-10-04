import ExpCore
import ExpUI
import SwiftUI

/// The Actions surface (EXP-253): the action prompts of EVERY member team
/// (EXP-1186: cross-team like the Inbox — one band per team, `TeamAvatar` +
/// name, once the caller is in more than one), each with a Run affordance
/// that opens the composer on the action's OWN team. EXP-825: Run, "New action" (EXP-431, in the web-parity
/// "Actions" section header since EXP-574) and a suggestion's tap are all
/// NAVIGATION into the Agent page composer, seeded with the action (or the
/// Create action builtin plus the suggestion's text and icon). SLOP-2: a row
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
    /// Actions · Suggestions (the MyWorkView segment pattern — the choice
    /// survives relaunch via AppStorage; a stored `automations` from before
    /// SLOP-2 reads as Actions).
    @AppStorage("actionsSegment") private var segmentRaw = Segment.actions.rawValue

    private enum Segment: String, CaseIterable {
        case actions
        case suggestions

        var label: String {
            switch self {
            case .actions: return "Actions"
            case .suggestions: return "Suggestions"
            }
        }
    }

    private var segment: Segment {
        Segment(rawValue: segmentRaw) ?? .actions
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                content(vm)
            }
        }
        .navigationTitle("Actions")
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

    /// The segmented pair — Actions (the run list) and Suggestions (seed
    /// ideas).
    @ViewBuilder
    private func content(_ vm: ActionsViewModel) -> some View {
        VStack(spacing: 0) {
            GlassSegmentedControl(
                options: Segment.allCases,
                selection: segment,
                label: { $0.label },
                identifier: { "actions-segment-\($0.rawValue)" },
                onSelect: { segmentRaw = $0.rawValue }
            )
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            switch segment {
            case .actions:
                actionsContent(vm)
            case .suggestions:
                suggestionsContent
            }
        }
    }

    @ViewBuilder
    private func actionsContent(_ vm: ActionsViewModel) -> some View {
        if vm.actions.isEmpty {
            Spacer()
            if vm.isLoading {
                ProgressView().tint(.white)
            } else if let error = vm.loadError {
                errorState(error)
            } else {
                emptyState
            }
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

    // MARK: - Suggestions (EXP-530)

    private var suggestionsContent: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                ForEach(ActionSuggestion.seeds) { suggestionCard($0) }
            }
            .padding()
        }
        .tabBarBottomInset()
    }

    /// EXP-694: the whole card IS the affordance — the "Use" pill was the only
    /// text button sitting where every sibling list puts a glyph, and a
    /// suggestion row has nothing else to tap.
    private func suggestionCard(_ suggestion: ActionSuggestion) -> some View {
        Button {
            useSuggestion(suggestion)
        } label: {
            HStack(spacing: 12) {
                AppIcon(suggestion.icon, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                VStack(alignment: .leading, spacing: 3) {
                    // SLOP-2: a seed that carries a trigger wears that
                    // trigger's glyph beside its title — what the tap will
                    // set up besides the action.
                    HStack(spacing: 6) {
                        Text(suggestion.title)
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        TriggerGlyphs(badges: TriggerBadges.of(suggested: suggestion.trigger))
                    }
                    Text(suggestion.description)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(3)
                }

                Spacer(minLength: 0)

                AppIcon(AppIcons.uiChevronRight, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
            .flatRow()
            .contentShape(Rectangle())
        }
        // `.plain` is what every tappable glass row in the app wears (the
        // ended-run rows, the session rows) — the tap feedback is the row's
        // own highlight, never a blue tint.
        .buttonStyle(.plain)
        .accessibilityIdentifier("suggestion-row")
    }

    /// Tapping a suggestion opens the composer on the Create action builtin
    /// with the suggestion's description as the request and its icon picked.
    /// A seed with a trigger appends the machine-readable block the creator
    /// agent passes to `exponential_actions_update` (byte-identical across
    /// the four clients — `TriggerNote.format`), bound to the caller's default
    /// trigger-capable machine, else the first one (offline included: a
    /// sleeping box still owns the trigger). No such machine = no block.
    private func useSuggestion(_ suggestion: ActionSuggestion) {
        var text = suggestion.description
        if let trigger = suggestion.trigger, let device = triggerDevice {
            text += TriggerNote.format(TriggerSpec(trigger: trigger, deviceId: device.deviceId))
        }
        openComposer(AgentComposerSeed(
            actionId: DomainContract.builtinCreateActionId,
            text: text,
            icon: suggestion.icon
        ))
    }

    /// The suggested trigger's runner (web `triggerDevices` + `defaultDeviceId`).
    private var triggerDevice: SteerDevice? {
        let candidates = (viewModel?.allDevices ?? []).filter(\.canRunTriggers)
        return candidates.first(where: \.isDefaultDevice) ?? candidates.first
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.actionDefault, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No actions yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Actions are reusable prompts your team runs on a desktop. Team owners create them on the web or desktop app.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    private func errorState(_ message: String) -> some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.uiWarning, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(message)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
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
