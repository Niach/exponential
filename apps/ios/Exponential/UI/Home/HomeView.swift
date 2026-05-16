import SwiftUI
import GRDB

struct HomeView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var workspaces: [WorkspaceEntity] = []
    @State private var projects: [ProjectEntity] = []
    @State private var activeWorkspaceId: String?
    @State private var showSidebar = false
    @State private var observationTask: Task<Void, Never>?
    @State private var syncing = false

    private var activeWorkspace: WorkspaceEntity? {
        workspaces.first { $0.id == activeWorkspaceId } ?? workspaces.first
    }

    private var filteredProjects: [ProjectEntity] {
        guard let wsId = activeWorkspace?.id else { return [] }
        return projects
            .filter { $0.workspaceId == wsId && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if syncing && workspaces.isEmpty {
                VStack(spacing: 12) {
                    ProgressView()
                        .tint(.white)
                    Text("Syncing...")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        ForEach(filteredProjects) { project in
                            NavigationLink(value: AppRoute.project(id: project.id)) {
                                projectRow(project)
                            }
                            .buttonStyle(.plain)
                        }

                        if filteredProjects.isEmpty && !syncing {
                            VStack(spacing: 8) {
                                Image(systemName: "tray")
                                    .font(.title2)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                Text("No projects yet")
                                    .font(.subheadline)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.top, 60)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                }
            }
        }
        .navigationTitle(activeWorkspace?.name ?? "Exponential")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    showSidebar = true
                } label: {
                    Image(systemName: "sidebar.left")
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    deps.auth.clearToken()
                    Task { await deps.syncManager.signOut() }
                } label: {
                    Image(systemName: "rectangle.portrait.and.arrow.right")
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
        }
        .sheet(isPresented: $showSidebar) {
            SidebarView(
                workspaces: workspaces,
                activeWorkspaceId: activeWorkspaceId,
                projects: filteredProjects,
                onSelectWorkspace: { id in
                    activeWorkspaceId = id
                    showSidebar = false
                },
                onSignOut: {
                    showSidebar = false
                    deps.auth.clearToken()
                    Task { await deps.syncManager.signOut() }
                }
            )
            .presentationBackground(.ultraThinMaterial)
            .presentationDetents([.medium, .large])
        }
        .onAppear {
            startObserving()
            if workspaces.isEmpty {
                syncing = true
                Task {
                    try? deps.db.clearAllData()
                    await deps.syncManager.initialSync()
                    syncing = false
                }
            }
        }
        .onDisappear { stopObserving() }
    }

    @ViewBuilder
    private func projectRow(_ project: ProjectEntity) -> some View {
        HStack(spacing: 12) {
            Circle()
                .fill(Color(hex: project.color ?? "#888888") ?? .gray)
                .frame(width: 10, height: 10)

            Text(project.name)
                .font(.body)
                .foregroundStyle(.white)

            Spacer()

            Text(project.prefix)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow()
    }

    private func startObserving() {
        observationTask = Task {
            let wsObs = ValueObservation.tracking { db in
                try WorkspaceEntity.fetchAll(db)
            }
            let projObs = ValueObservation.tracking { db in
                try ProjectEntity.fetchAll(db)
            }
            Task {
                for try await ws in wsObs.values(in: deps.db.dbPool) {
                    await MainActor.run {
                        workspaces = ws
                        if activeWorkspaceId == nil, let first = ws.first {
                            activeWorkspaceId = first.id
                        }
                    }
                }
            }
            Task {
                for try await proj in projObs.values(in: deps.db.dbPool) {
                    await MainActor.run { projects = proj }
                }
            }
        }
    }

    private func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }
}
