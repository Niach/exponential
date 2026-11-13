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
}
