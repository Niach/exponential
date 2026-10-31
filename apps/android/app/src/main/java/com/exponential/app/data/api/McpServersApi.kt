package com.exponential.app.data.api

import com.exponential.app.domain.McpServerOption
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonElement

/** EXP-792: one `mcpServers.list` row (#VAPP-185: + what Settings › MCP reads). */
@Serializable
data class McpServerListRow(
    val id: String,
    val name: String = "",
    val url: String? = null,
    val command: String? = null,
    val args: List<String>? = null,
    val transport: String = "http",
    val auth: String = "none",
    @SerialName("enabledByDefault") val enabledByDefault: Boolean = false,
    val connection: McpConnectionDto = McpConnectionDto(),
    /** #VAPP-185: the apps whose head package declares this server in `upstreams`. */
    val usedBy: List<String> = emptyList(),
)

@Serializable
data class McpConnectionDto(
    val status: String = "missing",
    val error: String? = null,
    val shared: Boolean = false,
)

@Serializable
private data class McpServersListInput(@SerialName("teamId") val teamId: String)

@Serializable
private data class McpServerIdInput(@SerialName("serverId") val serverId: String)

/**
 * EXP-1249: the team's MCP servers for the composer's `MCP servers ›` pick
 * (web `use-launch-options.ts` reads the same `mcpServers.list`). Members
 * only; the caller's own connection decides readiness. #VAPP-185: Settings ›
 * MCP reads the full [rows] and disconnects the caller's own credential.
 */
@Singleton
class McpServersApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun rows(accountId: String, teamId: String): List<McpServerListRow> =
        trpc.query(
            accountId,
            path = "mcpServers.list",
            input = McpServersListInput(teamId),
            inputSerializer = McpServersListInput.serializer(),
            outputSerializer = ListSerializer(McpServerListRow.serializer()),
        )

    suspend fun list(accountId: String, teamId: String): List<McpServerOption> =
        rows(accountId, teamId).map { row ->
            McpServerOption(
                id = row.id,
                name = row.name.ifBlank { row.id },
                connectionStatus = row.connection.status,
                enabledByDefault = row.enabledByDefault,
                url = row.url,
                command = row.command,
            )
        }

    suspend fun disconnect(accountId: String, serverId: String) {
        trpc.mutationUnit(accountId, "mcpServers.disconnect", McpServerIdInput(serverId), McpServerIdInput.serializer())
    }
}

// ── #VAPP-185: Settings › MCP › Clients · Access · Activity ──────────────────

@Serializable
data class McpClientRow(
    val clientId: String,
    val name: String = "",
    val icon: String? = null,
    val userId: String,
    val userName: String? = null,
    val userEmail: String? = null,
    val kinds: List<String> = emptyList(),
    val appsAccess: String? = null,
    val connectedAt: String? = null,
    val lastUsedAt: String? = null,
)

@Serializable
data class McpClientsList(val canSeeEveryone: Boolean = false, val clients: List<McpClientRow> = emptyList())

@Serializable
private data class McpClientsListInput(val teamId: String, val scope: String)

@Serializable
private data class McpClientRevokeInput(val clientId: String, val teamId: String, val userId: String? = null)

@Serializable
data class AppAccessApp(
    val id: String,
    val slug: String = "",
    val name: String = "",
    val icon: String? = null,
    val color: String? = null,
    val toolCount: Int = 0,
)

@Serializable
data class AppAccessMember(
    val userId: String,
    val name: String? = null,
    val email: String? = null,
    val image: String? = null,
    val role: String? = null,
    val levels: Map<String, String> = emptyMap(),
)

@Serializable
data class AppAccessList(
    val canEdit: Boolean = false,
    val apps: List<AppAccessApp> = emptyList(),
    val rows: List<AppAccessMember> = emptyList(),
)

@Serializable
private data class AppAccessSetInput(val appId: String, val userId: String, val level: String)

@Serializable
data class McpCallRow(
    val id: String,
    val tool: String = "",
    val status: String = "",
    val error: String? = null,
    val appName: String? = null,
    val userName: String? = null,
    val clientName: String? = null,
    val createdAt: String = "",
)

@Serializable
data class McpActivityList(val owner: Boolean = false, val calls: List<McpCallRow> = emptyList())

/** The MCP page's three server-only reads + its two writes (web `mcp-settings-page.tsx`). */
@Singleton
class McpSettingsApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun clients(accountId: String, teamId: String, scope: String): McpClientsList =
        trpc.query(accountId, "mcpClients.list", McpClientsListInput(teamId, scope), McpClientsListInput.serializer(), McpClientsList.serializer())

    /** [userId] only in Everyone (an owner revoking someone else's grant). */
    suspend fun revokeClient(accountId: String, teamId: String, clientId: String, userId: String?) {
        trpc.mutation(
            accountId,
            "mcpClients.revoke",
            McpClientRevokeInput(clientId, teamId, userId),
            McpClientRevokeInput.serializer(),
            JsonElement.serializer(),
        )
    }

    suspend fun appAccess(accountId: String, teamId: String): AppAccessList =
        trpc.query(accountId, "appAccess.list", McpServersListInput(teamId), McpServersListInput.serializer(), AppAccessList.serializer())

    suspend fun setAppAccess(accountId: String, appId: String, userId: String, level: String) {
        trpc.mutationUnit(accountId, "appAccess.set", AppAccessSetInput(appId, userId, level), AppAccessSetInput.serializer())
    }

    suspend fun activity(accountId: String, teamId: String): McpActivityList =
        trpc.query(accountId, "mcpActivity.list", McpServersListInput(teamId), McpServersListInput.serializer(), McpActivityList.serializer())
}
