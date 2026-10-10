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

            // Polish round ×4: the cloud chooser IS the LoginView layout —
            // the brand mark over "Continue to Exponential", no subtitle, no
            // card, the same Continue list as web, "Use a self-hosted
            // instance" below and the muted Privacy · Terms pair last.
            ScrollView {
                VStack(spacing: 24) {
                    AuthBrandHeading(title: "Continue to Exponential")

                    VStack(spacing: 16) {
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
                                .frame(maxWidth: .infinity, alignment: .center)
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
                                    // Full-width hit target — .plain hit-tests only opaque pixels.
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }

                    AuthLegalFooter()
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 48)
                .frame(maxWidth: .infinity)
            }
            .defaultScrollAnchor(.center)
            .scrollBounceBehavior(.basedOnSize)
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
        // The cloud login renders IMMEDIATELY (EXP-405): availability is
        // optimistic until the config probe lands and refines it. The old
        // probe-gated "Use Exponential Cloud" fallback flashed a chooser on
        // every fresh launch — it is gone; the welcome screen IS the login.
        // Apple leads (App Store guideline 4.8 / HIG prominence).
        if AppConstants.isStaging {
            // The deleted fallback button carried the "Use Staging Cloud"
            // label — keep a small marker so testers can tell builds apart.
            HStack(spacing: 6) {
                AppIcon(AppIcons.uiStaging, size: AppIcon.Size.small)
                    .foregroundStyle(.orange)
                Text("Staging · \(URL(string: AppConstants.defaultCloudUrl)?.host ?? AppConstants.defaultCloudUrl)")
                    .font(.caption)
                    .foregroundStyle(.orange)
            }
            .padding(.horizontal, 4)
        }
        if let vm = viewModel {
            if vm.appleAvailable {
                GlassOAuthButton("Continue with Apple", action: { vm.startCloudApple() }) {
                    // Apple's brand mark, not a registry glyph (see LoginView).
                    Image(systemName: "apple.logo")
                        .font(.body.weight(.medium))
                }
            }
            if vm.googleAvailable {
                GlassOAuthButton("Continue with Google", action: { vm.startCloudGoogle() }) {
                    // SF Symbols has no Google mark — the official multi-color G
                    // is drawn in GoogleLogoMark.
                    GoogleLogoMark()
                        .frame(width: 17, height: 17)
                }
            }
            // The rest of the Continue list (email, passkey, OIDC) signs in
            // through LoginView against the cloud: commit the cloud URL and
            // hand LoginView the picked branch. First-run only — the
            // add-server cover keeps its OAuth pair.
            if !showCancel {
                ForEach(vm.cloudConfig?.oidcProviders ?? []) { provider in
                    GlassOAuthButton("Continue with \(provider.name)", action: {
                        vm.continueOnCloud(.oidc(provider.id))
                    }) {
                        EmptyView()
                    }
                }
                if vm.emailAvailable {
                    GlassOAuthButton("Continue with email", action: {
                        vm.continueOnCloud(.email)
                    }) {
                        AppIcon(AppIcons.uiMail, size: AppIcon.Size.medium)
                    }
                    .accessibilityIdentifier("instance-continue-with-email-button")
                }
                if vm.passkeyAvailable {
                    GlassOAuthButton("Login with passkey", action: {
                        vm.continueOnCloud(.passkey)
                    }) {
                        AppIcon(AppIcons.authPasskey, size: AppIcon.Size.medium)
                    }
                    .accessibilityIdentifier("instance-passkey-button")
                }
            }
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
                    // Full-width hit target — .plain hit-tests only opaque pixels.
                    .contentShape(Rectangle())
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

            GlassTextField(
                "https://exp.example.com",
                text: $input,
                accessibilityIdentifier: "instance-url-field"
            )
            .keyboardType(.URL)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .focused($focused)
            .onSubmit {
                if canSubmit {
                    deps.auth.setInstanceUrl(input)
                }
            }
        }

        GlassSubmitButton("Continue", enabled: canSubmit) {
            deps.auth.setInstanceUrl(input)
        }
        .accessibilityIdentifier("instance-continue-button")

    }

}
