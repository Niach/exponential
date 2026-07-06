import ExpUI
import ExpCore
import SwiftUI

/// The Search tab: cross-project issue search over the active account's local
/// data. The glass field mirrors the inline search that used to live in the
/// issue list; while the query is empty the screen shows the cross-project
/// "Assigned to you" list (the former My Issues tab, folded in here).
struct SearchView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: SearchViewModel?
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                searchField
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)

                if let vm = viewModel {
                    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
                    if trimmed.isEmpty {
                        assignedToYou
                    } else {
                        let groups = vm.results(for: trimmed)
                        if groups.isEmpty {
                            noResults
                        } else {
                            resultsList(groups)
                        }
                    }
                }
            }
        }
        .navigationTitle("Search")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .onAppear {
            if viewModel == nil {
                viewModel = SearchViewModel(accountId: accountId, db: deps.db)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    // Custom glass search field. NOT system .searchable — on iOS 26+ the
    // navigationBarDrawer placement renders as a bottom-edge glass bar on
    // iPhone, colliding with the floating tab bar.
    private var searchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            TextField("Search issues", text: $query)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .focused($searchFocused)
                .submitLabel(.search)
            if !query.isEmpty {
                Button {
                    query = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
    }

    // MARK: - Empty-query state ("Assigned to you")

    private var assignedToYou: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "person.crop.circle")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("Assigned to you")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .padding(.horizontal, 20)
            .padding(.top, 6)
            .padding(.bottom, 2)

            MyIssuesListContent()
        }
    }

    // MARK: - Results

    private var noResults: some View {
        VStack(spacing: 12) {
            Spacer()
            Image(systemName: "magnifyingglass")
                .font(.title2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No matching issues")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func resultsList(_ groups: [SearchViewModel.ResultGroup]) -> some View {
        List {
            ForEach(groups) { group in
                Section {
                    ForEach(group.issues, id: \.id) { issue in
                        resultRow(issue)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 3, leading: 16, bottom: 3, trailing: 16))
                    }
                } header: {
                    projectHeader(group.project, count: group.issues.count)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color.clear)
        .safeAreaInset(edge: .bottom) {
            Color.clear.frame(height: 16)
        }
    }

    @ViewBuilder
    private func projectHeader(_ project: ProjectEntity, count: Int) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(Color(hex: project.color ?? "#888888") ?? .gray)
                .frame(width: 8, height: 8)

            Text(project.name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    @ViewBuilder
    private func resultRow(_ issue: IssueEntity) -> some View {
        NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
            HStack(spacing: 10) {
                Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 20)

                if let identifier = issue.identifier {
                    Text(identifier)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }

                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.from(issue.status).color)
                    .frame(width: 16)

                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)

                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }
}
