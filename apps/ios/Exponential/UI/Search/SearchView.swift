import ExpUI
import ExpCore
import SwiftUI

/// The Search tab: cross-board issue search over the active account's local
/// data, augmented by a debounced server full-text search (description +
/// comments). The glass field mirrors the inline search that used to live in
/// the issue list. Pure search (EXP-58): the empty-query state is a hint —
/// the "Assigned to you" list that used to live here moved to My Work.
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
                        searchHint
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
                viewModel = SearchViewModel(accountId: accountId, db: deps.db, issuesApi: deps.issuesApi)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it. Same for
            // the server search — onDisappear cancels any in-flight request.
            viewModel?.startObserving()
            viewModel?.queryChanged(query)
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        .onChange(of: query) { _, newValue in
            // Debounced + cancelled-on-keystroke inside the VM — never blocks
            // typing; local substring results stay instant.
            viewModel?.queryChanged(newValue)
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

    // MARK: - Empty-query state (search hint)

    private var searchHint: some View {
        VStack(spacing: 12) {
            Spacer()
            Image(systemName: "magnifyingglass")
                .font(.title2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Search issues across all boards")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Titles match instantly; descriptions and comments search the server.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            Spacer()
        }
        .frame(maxWidth: .infinity)
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
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    boardHeader(group.board, count: group.issues.count)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.plain)
        // Same compact-list treatment as the board IssueListView (EXP-80):
        // zero the List's own content margins, kill the implicit 44pt row
        // floor, and flow sections without the inter-section band, so search
        // results match the issue list's row rhythm.
        .contentMargins(.horizontal, 0, for: .scrollContent)
        .contentMargins(.top, 0, for: .scrollContent)
        .environment(\.defaultMinListRowHeight, 0)
        .listSectionSpacing(0)
        .scrollContentBackground(.hidden)
        .background(Color.clear)
        // Clearance for the floating tab bar (EXP-36).
        .tabBarBottomInset()
    }

    @ViewBuilder
    private func boardHeader(_ board: BoardEntity, count: Int) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(Color(hex: board.color ?? "#888888") ?? .gray)
                .frame(width: 8, height: 8)

            Text(board.name)
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
                // Priority icon (16pt column, IssueListView/Android parity)
                Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 16)

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
