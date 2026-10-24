import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// EXP-1373: the Team tab's rows land on one section of this page.
enum TeamSettingsSection: String, Hashable {
    case boards, agents, prompts, repositories, members, labels
}

struct TeamSettingsView: View {
    let teamId: String
    /// EXP-1373: the section a Team tab row opened it on (scrolled to).
    var focus: TeamSettingsSection? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var team: TeamEntity?
    @State private var members: [TeamMemberEntity] = []
    @State private var labels: [LabelEntity] = []
    @State private var boards: [BoardEntity] = []
    // NEW-2: the team's agent definitions, listed like the boards.
    @State private var agents: [AgentDefinitionEntity] = []
    @State private var agentTarget: AgentPageTarget?
    // NEW-2: the team's events (the band under the agents) and the titles of
    // the sessions that emitted them.
    @State private var events: [EventEntity] = []
    @State private var eventSessionTitles: [String: String] = [:]
    // NEW-7: Settings → Prompts (prompts replaced actions).
    @State private var prompts: [PromptEntity] = []
    @State private var promptTarget: PromptPageTarget?
    @State private var users: [UserEntity] = []
    // Invites of this team — the Members list badges the placeholder members
    // they created (EXP-630); minting/resending stays web-only.
    @State private var invites: [TeamInviteEntity] = []
    // Each observation loop is stored and cancelled individually — a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var showDeleteTeam = false
    @State private var deletingTeam = false
    @State private var deleteBoardTarget: BoardEntity?
    @State private var deletingBoard = false
    // FEED-79: a failed team/board delete is a toast, never a rose line.
    @Environment(\.toaster) private var toaster

    var body: some View {
        ZStack {
            AppBackground()

            ScrollViewReader { scroller in
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // Boards section
                    TeamBoardsSection(
                        boards: boards,
                        accountId: accountId,
                        teamId: teamId,
                        isOwner: isOwner,
                        boardsApi: deps.boardsApi,
                        repositoriesApi: deps.repositoriesApi,
                        onDelete: { board in deleteBoardTarget = board }
                    )
                    .id(TeamSettingsSection.boards)

                    // EXP-1291: Agents, managed exactly like boards — a row
                    // opens the agent page (read-only for members).
                    TeamAgentsSection(
                        agents: agents,
                        isOwner: isOwner,
                        onOpen: { agentTarget = AgentPageTarget(agentId: $0) }
                    )
                    .id(TeamSettingsSection.agents)

                    // NEW-2: the team's Events, newest first (web
                    // `AgentEventsBand`).
                    TeamEventsSection(events: events, sessionTitles: eventSessionTitles, users: users)

                    // NEW-7: Prompts — a row opens the prompt's page
                    // (read-only for members); "New prompt" is the owner's.
                    TeamPromptsSection(
                        prompts: prompts,
                        isOwner: isOwner,
                        onOpen: { promptTarget = PromptPageTarget(promptId: $0) }
                    )
                    .id(TeamSettingsSection.prompts)

                    // Repositories registry (server-only, read over tRPC —
                    // masterplan §6). A pure registry with "used by" chips;
                    // both the GitHub connect (App install / grant capture)
                    // and the grant-model "Reconnect GitHub" hop run in-app
                    // (EXP-45), web parity with repositories-section.tsx.
                    // Member-visible since EXP-557: any member connects/adds
                    // (sharing with the team); row removal is sharer-or-owner,
                    // so the section needs the viewer's id.
                    TeamRepositoriesSection(
                        accountId: accountId,
                        team: team,
                        currentUserId: deps.auth.userId,
                        isOwner: isOwner,
                        boards: boards,
                        repositoriesApi: deps.repositoriesApi,
                        integrationsApi: deps.integrationsApi,
                        instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
                    )
                    .id(TeamSettingsSection.repositories)

                    // Members section (view/manage only — inviting is web-only,
                    // EXP-216)
                    TeamMembersSection(
                        accountId: accountId,
                        teamId: teamId,
                        members: members,
                        users: users,
                        invites: invites,
                        currentUserId: deps.auth.userId,
                        membersApi: deps.teamMembersApi,
                        isOwner: isOwner,
                        teamName: team?.name
                    )
                    .id(TeamSettingsSection.members)

                    // Labels section
                    TeamLabelsSection(
                        accountId: accountId,
                        teamId: teamId,
                        labels: labels,
                        labelsApi: deps.labelsApi
                    )
                    .id(TeamSettingsSection.labels)

                    // Delete team — owner-only (hidden for non-owners, full
                    // web parity). No slug is exempt: EXP-364 killed every
                    // feedback-team special case, an owner may delete ANY team.
                    if team != nil, isOwner {
                        // ×4: the Danger zone heading is a filled band
                        // with its label in the danger red.
                        VStack(alignment: .leading, spacing: 8) {
                            GlassSectionBand("Danger zone", danger: true)

                            Button {
                                showDeleteTeam = true
                            } label: {
                                HStack {
                                    AppIcon(AppIcons.uiDelete, size: AppIcon.Size.medium)
                                    Text("Delete team")
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.red)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 10)
                                // Full-capsule hit target — .plain hit-tests only opaque pixels.
                                .contentShape(Rectangle())
                            }
                            // NOT a GlassPill (EXP-698): this is a full-width
                            // destructive action in the app's one red, and a
                            // pill is a fixed-height capsule with a white
                            // label. EXP-1176: a text button is the row-radius
                            // rectangle (GlassSubmitButton's chrome), not a capsule.
                            .background(
                                GlassTokens.fillCard,
                                in: RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                            )
                            .overlay(
                                RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                            )
                            .buttonStyle(.plain)
                        }
                    }
                }
                .padding(16)
            }
            .task(id: focus) {
                guard let focus else { return }
                // The sections size as their rows sync in: land once laid out.
                try? await Task.sleep(for: .milliseconds(150))
                scroller.scrollTo(focus, anchor: .top)
            }
            }
        }
        .navigationTitle(team?.name ?? "Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .navigationDestination(item: $agentTarget) { target in
            AgentSettingsPage(teamId: teamId, agentId: target.agentId, isOwner: isOwner)
        }
        .navigationDestination(item: $promptTarget) { target in
            PromptSettingsPage(teamId: teamId, promptId: target.promptId)
                .environment(\.accountId, accountId)
        }
        .onAppear { startObserving() }
        .onDisappear { stopObserving() }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4. Each on a
        // zero-size node of its own (EXP-240): two presentations stacked on
        // one node and SwiftUI drops the second.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showDeleteTeam) {
                    GlassAlert(
                        prompt: Prompts.DeleteTeam.copy(name: team?.name ?? "this team"),
                        enabled: ["delete": !deletingTeam],
                        handlers: ["delete": { Task { await deleteTeam() } }]
                    )
                }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $deleteBoardTarget) { board in
                    GlassAlert(
                        prompt: Prompts.TrashBoard.copy(name: board.name),
                        enabled: ["trash": !deletingBoard],
                        handlers: ["trash": { Task { await deleteBoard(board) } }]
                    )
                }
        }
    }

    /// Owner surface: boards/members management + the danger zone, and the
    /// owner half of the repos section's sharer-or-owner row gating (EXP-557 —
    /// connecting/adding repos is member-level now).
    private var isOwner: Bool {
        guard let me = deps.auth.userId else { return false }
        return members.contains { $0.userId == me && $0.role == DomainContract.teamRoleOwner }
    }

    private func deleteTeam() async {
        deletingTeam = true
        defer { deletingTeam = false }
        do {
            // An owner may delete ANY of their teams, including the last one
            // (EXP-188 dropped the last-team guard) — a team-less account
            // lands on the zero-team create-or-join empty state.
            try await deps.teamsApi.delete(accountId: accountId, teamId: teamId)
            // Membership changed, so every shape's server-derived where clause
            // rotated — relaunch the pipeline so all shapes re-scope
            // immediately instead of waiting out the in-flight live long-polls
            // (up to ~60s of "deleted team still there").
            await deps.syncManager.restartPipeline(accountId: accountId)
            await MainActor.run {
                // Pop the whole stack to root — parent views (server detail /
                // settings) may still target the deleted team.
                NotificationCenter.default.post(name: .teamDeleted, object: nil)
            }
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the team"),
                description: error.trpcUserMessage
            )
        }
    }

    private func deleteBoard(_ board: BoardEntity) async {
        deletingBoard = true
        defer { deletingBoard = false; deleteBoardTarget = nil }
        do {
            try await deps.teamsApi.deleteBoard(accountId: accountId, boardId: board.id)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the board"),
                description: error.trpcUserMessage
            )
        }
    }

    private func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId

        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try TeamEntity.fetchOne(db, key: teamId)
            }
            do {
                for try await item in obs.values(in: pool) {
                    await MainActor.run { team = item }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                // Members ×4 list in JOIN order.
                try TeamMemberEntity.filter(Column("team_id") == teamId)
                    .order(Column("created_at"), Column("id")).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { members = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { labels = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { boards = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try SessionQueries.agents(db: db, teamId: teamId)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { agents = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db -> ([EventEntity], [String: String]) in
                let events = try EventEntity.filter(Column("team_id") == teamId)
                    .order(Column("created_at").desc, Column("id").desc)
                    .limit(TeamEventsSection.cap)
                    .fetchAll(db)
                let sessionIds = Array(Set(events.compactMap(\.sessionId)))
                let sessions = sessionIds.isEmpty
                    ? []
                    : try SessionEntity.filter(sessionIds.contains(Column("id"))).fetchAll(db)
                return (events, Dictionary(
                    sessions.map { ($0.id, $0.title ?? "") }, uniquingKeysWith: { a, _ in a }
                ))
            }
            do {
                for try await (items, titles) in obs.values(in: pool) {
                    await MainActor.run {
                        events = items
                        eventSessionTitles = titles
                    }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try PromptEntity.filter(Column("team_id") == teamId)
                    .order(Column("sort_order"), Column("name"))
                    .fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { prompts = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try TeamInviteEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { invites = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { users = items }
                }
            } catch {}
        })
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}

/// The agent page's navigation item: an agent id, or nil for a new one.
private struct AgentPageTarget: Hashable {
    let agentId: String?
}

/// The prompt page's navigation item: a prompt id, or nil for a new one.
private struct PromptPageTarget: Hashable {
    let promptId: String?
}
