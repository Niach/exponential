import ExpUI
import ExpCore
import SwiftUI

struct InstanceView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var input = "https://"
    @State private var viewModel: InstanceViewModel?
    // Self-hosting is demoted (EXP-14): the URL field stays hidden behind a
    // small link until the user opts in. When the cloud is unavailable (already
    // added) the field is the only path, so it's shown outright.
    @State private var showSelfHost = false
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
        let normalized = AppConstants.defaultCloudUrl
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
                        cloudSection
                    }

                    if showSelfHost || cloudAlreadyAdded {
                        selfHostSection
                    }

                    if let error = viewModel?.error {
                        Text(error)
                            .font(.callout)
                            .foregroundStyle(.red)
                            .padding(.horizontal, 4)
                    }

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
        .onAppear {
            focused = false
            if viewModel == nil {
                viewModel = InstanceViewModel(authApi: deps.authApi, auth: deps.auth)
            }
            Task { await viewModel?.loadCloudConfig() }
        }
    }

    // MARK: - Cloud (primary path)

    @ViewBuilder
    private var cloudSection: some View {
        if let vm = viewModel, vm.hasDirectOAuth {
            // Sign in with the cloud provider directly — no intermediate screen.
            // Apple leads (App Store guideline 4.8 / HIG prominence).
            if vm.appleAvailable {
                oauthButton(label: "Continue with Apple", action: { vm.startCloudApple() }) {
                    Image(systemName: "apple.logo")
                        .font(.body.weight(.medium))
                }
            }
            if vm.googleAvailable {
                oauthButton(label: "Continue with Google", action: { vm.startCloudGoogle() }) {
                    // SF Symbols has no Google mark — the official multi-color G
                    // is drawn in GoogleLogoMark.
                    GoogleLogoMark()
                        .frame(width: 17, height: 17)
                }
            }
        } else {
            // Offline / cloud config not yet loaded (or a password-only cloud):
            // fall back to the generic cloud button, which routes to the full
            // login screen and its own config fetch + retry.
            Button {
                deps.auth.setInstanceUrl(AppConstants.defaultCloudUrl)
            } label: {
                HStack(spacing: 8) {
                    if AppConstants.isStaging {
                        Image(systemName: "flask")
                            .font(.body)
                            .foregroundStyle(.orange)
                        Text("Use Staging Cloud")
                            .font(.body.weight(.medium))
                            .foregroundStyle(.white)
                    } else {
                        Image(systemName: "cloud")
                            .font(.body)
                            .foregroundStyle(.white)
                        Text("Use Exponential Cloud")
                            .font(.body.weight(.medium))
                            .foregroundStyle(.white)
                    }
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
        }

        if !showSelfHost {
            Button {
                withAnimation(.easeInOut(duration: 0.2)) { showSelfHost = true }
            } label: {
                Text("Use a self-hosted instance")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("instance-self-host-link")
        }
    }

    // MARK: - Self-hosted

    @ViewBuilder
    private var selfHostSection: some View {
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
                .accessibilityIdentifier("instance-url-field")
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
        .accessibilityIdentifier("instance-continue-button")
        .background(canSubmit ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )

        Text("Self-hosted? Enter the full URL of your server.")
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    // MARK: - OAuth button (mirrors LoginView.oauthButton)

    @ViewBuilder
    private func oauthButton(label: String, action: @escaping () -> Void, @ViewBuilder icon: () -> some View) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                icon()
                Text(label)
            }
            .font(.body.weight(.medium))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
        }
        .background(Color.white.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
        )
    }
}
