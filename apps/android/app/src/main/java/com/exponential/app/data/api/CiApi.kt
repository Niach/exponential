package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// #EXP-1617 / #EXP-1804: mirrors the people half of apps/web/src/lib/trpc/ci.ts
// (the run page and a device's CI runner section). Runs and jobs come from
// sync (the ci_runs / ci_jobs shapes); only the LOG is read on demand.

/** `ci.log`'s answer: the text from the asked offset, where to read next,
 *  and whether the job's log is whole (no further reads). */
@Serializable
data class CiLogChunk(
    @SerialName("text") val text: String = "",
    @SerialName("nextOffset") val nextOffset: Long = 0,
    @SerialName("complete") val complete: Boolean = false,
)

@Serializable
private data class CiLogInput(@SerialName("jobId") val jobId: String, @SerialName("offset") val offset: Long)

@Serializable
private data class CiRerunInput(@SerialName("runId") val runId: String, @SerialName("failedOnly") val failedOnly: Boolean)

@Serializable
private data class CiRunIdInput(@SerialName("runId") val runId: String)

@Serializable
data class CiRerunResult(@SerialName("runId") val runId: String)

@Serializable
internal data class CiSetRunnerInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("enabled") val enabled: Boolean,
    @SerialName("labels") val labels: List<String>,
    @SerialName("slots") val slots: Int,
    @SerialName("timeoutMin") val timeoutMin: Int,
)

/** #EXP-1617 (repo-ci.json): a repository's `ci_config`; absent keys = off / none. */
@Serializable
data class RepoCiConfig(
    @SerialName("requiredChecks") val requiredChecks: List<String>? = null,
    @SerialName("githubStatuses") val githubStatuses: Boolean? = null,
    @SerialName("actionsRunners") val actionsRunners: Boolean? = null,
)

/** One secret's NAME (values are never read back). */
@Serializable
data class RepoCiSecret(@SerialName("name") val name: String, @SerialName("updatedAt") val updatedAt: String = "")

/** `ci.repoSettings`'s answer. */
@Serializable
data class RepoCiSettings(
    @SerialName("config") val config: RepoCiConfig = RepoCiConfig(),
    @SerialName("secrets") val secrets: List<RepoCiSecret> = emptyList(),
)

@Serializable
private data class CiRepoInput(@SerialName("repositoryId") val repositoryId: String)

/** Nulls drop off the wire (the client's `explicitNulls = false`): only the
 *  given keys change. */
@Serializable
private data class CiSetRepoConfigInput(
    @SerialName("repositoryId") val repositoryId: String,
    @SerialName("requiredChecks") val requiredChecks: List<String>? = null,
    @SerialName("githubStatuses") val githubStatuses: Boolean? = null,
    @SerialName("actionsRunners") val actionsRunners: Boolean? = null,
)

@Serializable
private data class CiSetSecretInput(
    @SerialName("repositoryId") val repositoryId: String,
    @SerialName("name") val name: String,
    @SerialName("value") val value: String,
)

@Serializable
private data class CiDeleteSecretInput(@SerialName("repositoryId") val repositoryId: String, @SerialName("name") val name: String)

@Singleton
class CiApi @Inject constructor(private val trpc: TrpcClient) {

    /** `ci.log{jobId, offset}`: member-only; read from 0, then from `nextOffset`. */
    suspend fun log(accountId: String, jobId: String, offset: Long): CiLogChunk =
        trpc.query(
            accountId,
            path = "ci.log",
            input = CiLogInput(jobId, offset),
            inputSerializer = CiLogInput.serializer(),
            outputSerializer = CiLogChunk.serializer(),
        )

    /** `ci.rerun{runId, failedOnly: true}`: the new run's id (the page opens it). */
    suspend fun rerunFailed(accountId: String, runId: String): String =
        trpc.mutation(
            accountId,
            path = "ci.rerun",
            input = CiRerunInput(runId, failedOnly = true),
            inputSerializer = CiRerunInput.serializer(),
            outputSerializer = CiRerunResult.serializer(),
        ).runId

    /** `ci.cancel{runId}`. */
    suspend fun cancel(accountId: String, runId: String) =
        trpc.mutationUnit(
            accountId,
            path = "ci.cancel",
            input = CiRunIdInput(runId),
            inputSerializer = CiRunIdInput.serializer(),
        )

    /** `ci.setRunner{deviceId, enabled, labels, slots, timeoutMin}`: the WHOLE
     *  settings object, the device owner only ([deviceId] = the steer id). */
    suspend fun setRunner(accountId: String, deviceId: String, enabled: Boolean, labels: List<String>, slots: Int, timeoutMin: Int) =
        trpc.mutationUnit(
            accountId,
            path = "ci.setRunner",
            input = CiSetRunnerInput(deviceId, enabled, labels, slots, timeoutMin),
            inputSerializer = CiSetRunnerInput.serializer(),
        )

    /** `ci.repoSettings{repositoryId}`: the repo's CI config + secret names (members). */
    suspend fun repoSettings(accountId: String, repositoryId: String): RepoCiSettings =
        trpc.query(
            accountId,
            path = "ci.repoSettings",
            input = CiRepoInput(repositoryId),
            inputSerializer = CiRepoInput.serializer(),
            outputSerializer = RepoCiSettings.serializer(),
        )

    /** `ci.setRepoConfig`: only the non-null keys change (the repo's managers). */
    suspend fun setRepoConfig(
        accountId: String,
        repositoryId: String,
        requiredChecks: List<String>? = null,
        githubStatuses: Boolean? = null,
        actionsRunners: Boolean? = null,
    ) = trpc.mutationUnit(
        accountId,
        path = "ci.setRepoConfig",
        input = CiSetRepoConfigInput(repositoryId, requiredChecks, githubStatuses, actionsRunners),
        inputSerializer = CiSetRepoConfigInput.serializer(),
    )

    /** `ci.setSecret{repositoryId, name, value}` (creates or replaces). */
    suspend fun setSecret(accountId: String, repositoryId: String, name: String, value: String) =
        trpc.mutationUnit(
            accountId,
            path = "ci.setSecret",
            input = CiSetSecretInput(repositoryId, name, value),
            inputSerializer = CiSetSecretInput.serializer(),
        )

    /** `ci.deleteSecret{repositoryId, name}`. */
    suspend fun deleteSecret(accountId: String, repositoryId: String, name: String) =
        trpc.mutationUnit(
            accountId,
            path = "ci.deleteSecret",
            input = CiDeleteSecretInput(repositoryId, name),
            inputSerializer = CiDeleteSecretInput.serializer(),
        )
}
