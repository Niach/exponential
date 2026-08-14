package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.json.JsonObject

// Mirrors apps/web/src/lib/trpc/widgets.ts `submissionForIssue` (EXP-496) —
// the reporter/page/env metadata behind a widget- or agent-filed issue.
// Server-only data (never an Electric shape): the row carries reporter PII
// kept out of issue descriptions, so it is fetched on demand, member-gated,
// and callers render nothing on null/error. The wire row is the full drizzle
// row; this decodes only what the metadata card renders (ignoreUnknownKeys
// drops the rest, defaults survive new nulls).
@Serializable
data class WidgetSubmissionResult(
    val reporterEmail: String? = null,
    val reporterName: String? = null,
    val pageUrl: String? = null,
    val userAgent: String? = null,
    val viewportWidth: Int? = null,
    val viewportHeight: Int? = null,
    val screenWidth: Int? = null,
    val screenHeight: Int? = null,
    val devicePixelRatio: Double? = null,
    // Free-form blob (`identify()` custom data; {"via":"mcp"} for agent bug
    // reports) — kept as a raw JSON tree for pretty-printing.
    val customData: JsonObject? = null,
)

@Serializable
private data class SubmissionForIssueInput(@SerialName("issueId") val issueId: String)

@Singleton
class WidgetsApi @Inject constructor(private val trpc: TrpcClient) {

    // Null for issues without a widget/MCP submission row; throws for
    // non-members — callers render nothing in both cases.
    suspend fun submissionForIssue(accountId: String, issueId: String): WidgetSubmissionResult? =
        trpc.query(
            accountId,
            path = "widgets.submissionForIssue",
            input = SubmissionForIssueInput(issueId),
            inputSerializer = SubmissionForIssueInput.serializer(),
            outputSerializer = WidgetSubmissionResult.serializer().nullable,
        )
}
