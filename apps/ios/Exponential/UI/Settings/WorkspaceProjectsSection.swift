import ExpCore
import SwiftUI

struct WorkspaceProjectsSection: View {
    let projects: [ProjectEntity]
    let onDelete: (ProjectEntity) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Projects")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(projects.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            if projects.isEmpty {
                Text("No projects in this workspace yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            } else {
                ForEach(projects, id: \.id) { project in
                    HStack(spacing: 10) {
                        // Color dot
                        Circle()
                            .fill(Color(hex: project.color ?? "#6366f1") ?? .gray)
                            .frame(width: 10, height: 10)

                        // Project name
                        Text(project.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)

                        Spacer()

                        // Prefix badge
                        Text(project.prefix)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Color.white.opacity(0.08))
                            .clipShape(RoundedRectangle(cornerRadius: 4))

                        // Delete button
                        Button {
                            onDelete(project)
                        } label: {
                            Image(systemName: "trash")
                                .font(.caption)
                                .foregroundStyle(.red.opacity(0.5))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                }
            }
        }
    }
}
