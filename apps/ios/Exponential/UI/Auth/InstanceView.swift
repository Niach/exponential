import SwiftUI

struct InstanceView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var input = "https://"
    @FocusState private var focused: Bool

    private var canSubmit: Bool {
        input.count > 8
    }

    var body: some View {
        ZStack {
            AppBackground()

            VStack(alignment: .leading, spacing: 0) {
                Text("Exponential")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(.white)

                Spacer().frame(height: 8)

                Text("Connect to your instance")
                    .font(.body)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Spacer().frame(height: 32)

                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Instance URL")
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
                }
                .padding(24)
                .glassCard()
            }
            .padding(.horizontal, 32)
        }
        .onAppear { focused = true }
    }
}
