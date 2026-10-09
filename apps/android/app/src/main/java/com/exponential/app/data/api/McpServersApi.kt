package com.exponential.app.data.api

import com.exponential.app.domain.McpServerOption
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

/** EXP-792: one `mcpServers.list` row — only what the composer's picker reads. */
@Serializable
internal data class McpServerListRow(
    val id: String,
    val name: String = "",
    val url: String? = null,
    val command: String? = null,
    @SerialName("enabledByDefault") val enabledByDefault: Boolean = false,
    val connection: McpConnectionDto = McpConnectionDto(),
)

@Serializable
internal data class McpConnectionDto(val status: String = "missing")

@Serializable
private data class McpServersListInput(@SerialName("teamId") val teamId: String)

/**
 * EXP-1249: the team's MCP servers for the composer's `MCP servers ›` pick
 * (web `use-launch-options.ts` reads the same `mcpServers.list`). Members
 * only; the caller's own connection decides readiness.
 */
@Singleton
class McpServersApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun list(accountId: String, teamId: String): List<McpServerOption> =
        trpc.query(
            accountId,
            path = "mcpServers.list",
            input = McpServersListInput(teamId),
            inputSerializer = McpServersListInput.serializer(),
            outputSerializer = ListSerializer(McpServerListRow.serializer()),
        ).map { row ->
            McpServerOption(
                id = row.id,
                name = row.name.ifBlank { row.id },
                connectionStatus = row.connection.status,
                enabledByDefault = row.enabledByDefault,
                url = row.url,
                command = row.command,
            )
        }
}
