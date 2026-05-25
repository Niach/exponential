import SwiftUI

struct InstanceView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var input = "https://"
    @FocusState private var focused: Bool

    var showCancel: Bool = false
    var onCancel: (() -> Void)? = nil

    private var canSubmit: Bool {
        input.count > 8
    }

    // The cloud preset is offered only when there isn't already an account for
    // it. Re-tapping it from the add-server flow when the cloud account exists
    // re-activates the existing account through upsertAndActivate, which mid-
    // flight republishes a non-nil token and races SyncManager's DB swap —
    // hiding the button removes that path entirely. Users can still switch to
    // the existing cloud account from Settings.
    private var cloudAlreadyAdded: Bool {
        let normalized = AppConstants.publicCloudUrl
        return deps.auth.accounts.contains { $0.instanceUrl == normalized }
    }

    var body: some View {
        ZStack {
            AppBackground()

            VStack(alignment: .leading, spacing: 0) {
                Text("Exponential")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(.white)

                Spacer().frame(height: 8)

                Text("Connect to Exponential")
                    .font(.body)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Spacer().frame(height: 32)

                VStack(alignment: .leading, spacing: 16) {
                    if !cloudAlreadyAdded {
                        Button {
                            deps.auth.setInstanceUrl(AppConstants.publicCloudUrl)
                        } label: {
                            HStack(spacing: 8) {
                                Image(systemName: "cloud")
                                    .font(.body)
                                    .foregroundStyle(.white)
                                Text("Use Exponential Cloud")
                                    .font(.body.weight(.medium))
                                    .foregroundStyle(.white)
                                Spacer()
                                Image(systemName: "arrow.right")
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                            .padding(.horizontal, 14)
                        }
                        .background(Color.white.opacity(0.15))
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
                        )

                        HStack(spacing: 12) {
                            Rectangle()
                                .fill(Color.white.opacity(0.12))
                                .frame(height: 0.5)
                            Text("or self-host")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            Rectangle()
                                .fill(Color.white.opacity(0.12))
                                .frame(height: 0.5)
                        }
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("Server URL")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                        TextField("https://exp.example.com", text: $input)
                            .textFieldStyle(.plain)
                            .keyboardType(.URL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .padding(.horizontal, 14)
                            .padding(.vertical, 12)
                            .background(Color.white.opacity(0.06))
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                            .overlay(
                                RoundedRectangle(cornerRadius: 10)
                                    .stroke(Color.white.opacity(focused ? 0.3 : 0.1), lineWidth: 0.5)
                            )
                            .foregroundStyle(.white)
                            .focused($focused)
                            .onSubmit {
                                if canSubmit {
                                    deps.auth.setInstanceUrl(input)
                                }
                            }
                    }

                    Button {
                        deps.auth.setInstanceUrl(input)
                    } label: {
                        Text("Continue")
                            .font(.body.weight(.medium))
                            .foregroundStyle(.white)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                    }
                    .disabled(!canSubmit)
                    .background(canSubmit ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .overlay(
                        RoundedRectangle(cornerRadius: 10)
                            .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
                    )

                    Text("Self-hosted? Enter the full URL of your server.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                    if showCancel {
                        Button {
                            onCancel?()
                        } label: {
                            Text("Cancel")
                                .font(.body)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 12)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(24)
                .glassCard()
            }
            .padding(.horizontal, 32)
        }
        .onAppear { focused = false }
    }
}
