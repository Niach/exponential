import Foundation

// Mirrors apps/web/src/lib/trpc/widgets.ts `submissionForIssue` (EXP-496) —
// the reporter/page/env metadata behind a widget- or agent-filed issue.
// Server-only data (never an Electric shape): the row carries reporter PII
// that is deliberately kept out of issue descriptions, so it is fetched on
// demand, member-gated, and callers render nothing on nil/error.

private struct SubmissionForIssueInput: Encodable {
    let issueId: String
}

/// The `widget_submissions` row behind an issue. The wire row is the full
/// drizzle row — this decodes only what the metadata card renders, and every
/// field is optional so new server columns never fail the decode.
public struct WidgetSubmissionRow: Decodable, Sendable {
    public let reporterEmail: String?
    public let reporterName: String?
    public let pageUrl: String?
    public let userAgent: String?
    public let viewportWidth: Int?
    public let viewportHeight: Int?
    public let screenWidth: Int?
    public let screenHeight: Int?
    public let devicePixelRatio: Double?
    /// Free-form blob (`identify()` custom data; `{"via":"mcp"}` for agent
    /// bug reports). Type-faithful so the card can pretty-print it.
    public let customData: [String: JSONWireValue]?
}

public final class WidgetsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// `nil` for issues that were not filed through the widget/MCP intake;
    /// throws for non-members — callers render nothing in both cases.
    public func submissionForIssue(accountId: String, issueId: String) async throws -> WidgetSubmissionRow? {
        try await trpc.query(
            accountId: accountId,
            path: "widgets.submissionForIssue",
            input: SubmissionForIssueInput(issueId: issueId)
        )
    }
}
