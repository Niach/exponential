package com.exponential.app.data.api

import com.exponential.app.domain.McpServerOption
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

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
private data class AppAccessMineInput(val appId: String)

/** eApps R3: the caller's own level for an app (`appAccess.mine`). */
@Serializable
data class AppAccessMine(val level: String)

/** #VAPP-220: one `appAccess.appTools` automation (lenient: `on` stays raw,
 *  [toRowInput] reads it; an unknown shape = no row). */
@Serializable
data class AppAutomationDto(
    val name: String = "",
    val on: JsonElement? = null,
    val tool: String = "",
    val paused: String? = null,
    val lastRunAt: String? = null,
    val lastStatus: String? = null,
    val lastError: String? = null,
    val lastFailureAt: String? = null,
) {
    fun toRowInput(): com.exponential.app.domain.Apps.AutomationRowInput? {
        val obj = on as? JsonObject ?: return null
        fun JsonElement?.text(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.content
        val trigger = when {
            obj["schedule"] is JsonObject -> (obj["schedule"] as JsonObject).let { schedule ->
                val cron = schedule["cron"].text() ?: return null
                com.exponential.app.domain.Apps.AutomationTrigger.Schedule(cron, schedule["tz"].text())
            }
            obj["record"] is JsonObject -> (obj["record"] as JsonObject).let { record ->
                com.exponential.app.domain.Apps.AutomationTrigger.Record(
                    record["kind"].text() ?: return null,
                    record["op"].text() ?: return null,
                )
            }
            obj["webhook"] is JsonObject -> com.exponential.app.domain.Apps.AutomationTrigger.Webhook(
                (obj["webhook"] as JsonObject)["name"].text() ?: return null,
            )
            else -> com.exponential.app.domain.Apps.AutomationTrigger.Event(obj["event"].text() ?: return null)
        }
        return com.exponential.app.domain.Apps.AutomationRowInput(name, trigger, paused, lastStatus)
    }
}

/** #VAPP-238: one `appAccess.appTools` webhook (the secret itself never comes back). */
@Serializable
data class AppHookDto(
    val name: String = "",
    val url: String = "",
    val hasSecret: Boolean = false,
    val rotatedAt: String? = null,
)

/** #VAPP-238: `appAccess.rotateHook`'s answer, the secret shown once. */
@Serializable
data class AppHookSecretDto(val name: String = "", val url: String = "", val secret: String = "")

/** #VAPP-220: `appAccess.appTools` (only the automations + webhooks are read on phones). */
@Serializable
data class AppToolsDto(
    val canToggle: Boolean = false,
    val automations: List<AppAutomationDto> = emptyList(),
    val hooks: List<AppHookDto> = emptyList(),
)

@Serializable
private data class AppToolsInput(val appId: String)

@Serializable
private data class AppHookRotateInput(val appId: String, val name: String)

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

    /** eApps R3: the caller's own level for one app (`none | read | write`). */
    suspend fun appAccessMine(accountId: String, appId: String): AppAccessMine =
        trpc.query(accountId, "appAccess.mine", AppAccessMineInput(appId), AppAccessMineInput.serializer(), AppAccessMine.serializer())

    /** #VAPP-220: the app's tools, upstreams and automations (`appAccess.appTools`). */
    suspend fun appTools(accountId: String, appId: String): AppToolsDto =
        trpc.query(accountId, "appAccess.appTools", AppToolsInput(appId), AppToolsInput.serializer(), AppToolsDto.serializer())

    /** #VAPP-238: mint or rotate a webhook's secret (owner); the answer carries it once. */
    suspend fun rotateHook(accountId: String, appId: String, name: String): AppHookSecretDto =
        trpc.mutation(
            accountId,
            "appAccess.rotateHook",
            AppHookRotateInput(appId, name),
            AppHookRotateInput.serializer(),
            AppHookSecretDto.serializer(),
        )

    suspend fun setAppAccess(accountId: String, appId: String, userId: String, level: String) {
        trpc.mutationUnit(accountId, "appAccess.set", AppAccessSetInput(appId, userId, level), AppAccessSetInput.serializer())
    }

    suspend fun activity(accountId: String, teamId: String): McpActivityList =
        trpc.query(accountId, "mcpActivity.list", McpServersListInput(teamId), McpServersListInput.serializer(), McpActivityList.serializer())
}
