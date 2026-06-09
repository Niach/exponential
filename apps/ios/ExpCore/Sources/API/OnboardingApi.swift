import Foundation

private struct EmptyInput: Encodable {}

public final class OnboardingApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    // Marks onboarding complete for the current user (onboarding.complete in the
    // shared appRouter — sets users.onboardingCompletedAt = now). No input.
    public func complete(accountId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "onboarding.complete", input: EmptyInput())
    }
}
