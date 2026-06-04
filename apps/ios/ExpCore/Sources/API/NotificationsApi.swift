import Foundation

// Mirrors apps/web/src/lib/trpc/notifications.ts — inbox mark-read.
private struct NotificationIdInput: Encodable {
    let id: String
}

private struct EmptyInput: Encodable {}

private struct EmptyResult: Decodable {}

public final class NotificationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func markRead(accountId: String, id: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "notifications.markRead",
            input: NotificationIdInput(id: id)
        )
    }

    public func markAllRead(accountId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "notifications.markAllRead",
            input: EmptyInput()
        )
    }
}
