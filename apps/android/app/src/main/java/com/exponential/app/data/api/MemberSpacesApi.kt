package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject

// #EXP-2161 (member-space.json): mirrors apps/web/src/lib/trpc/member-spaces.ts
// for what a phone does: pick a team device for the Assistant (`request`),
// read the own space's state (`mine`), and, on a device the viewer owns (or
// one shared into a team they own), list and remove the members running there.

@Serializable
private data class RequestSpaceInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("teamId") val teamId: String,
)

/** `request`'s answer: the space's own device id (the Assistant's device now). */
@Serializable
data class RequestedSpace(
    @SerialName("spaceId") val spaceId: String,
    @SerialName("deviceId") val deviceId: String,
)

@Serializable
private data class SpaceRowIdInput(@SerialName("id") val id: String)

@Serializable
private data class SpaceIdInput(@SerialName("spaceId") val spaceId: String)

/** One member running their Assistant on a device (`forDevice`). */
@Serializable
data class SpaceMember(
    @SerialName("spaceId") val spaceId: String,
    @SerialName("userId") val userId: String,
    @SerialName("name") val name: String = "",
    @SerialName("teamId") val teamId: String? = null,
)

@Serializable
private data class SpaceMembers(@SerialName("members") val members: List<SpaceMember> = emptyList())

/** The caller's own space: `starting`, `waiting` (for their approval) or `ready`. */
@Serializable
data class MySpace(
    @SerialName("spaceId") val spaceId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("status") val status: String,
) {
    companion object {
        const val WAITING = "waiting"
        const val READY = "ready"
    }
}

@Serializable
data class MySpaceAnswer(
    @SerialName("space") val space: MySpace? = null,
    @SerialName("ended") val ended: Boolean = false,
)

@Singleton
class MemberSpacesApi @Inject constructor(private val trpc: TrpcClient) {
    /** `memberSpaces.request`: the Assistant runs on [deviceId] (the host's
     *  device id), picked through [teamId]; a new conversation starts. */
    suspend fun request(accountId: String, deviceId: String, teamId: String): RequestedSpace =
        trpc.mutation(
            accountId,
            path = "memberSpaces.request",
            input = RequestSpaceInput(deviceId, teamId),
            inputSerializer = RequestSpaceInput.serializer(),
            outputSerializer = RequestedSpace.serializer(),
        )

    suspend fun mine(accountId: String): MySpaceAnswer =
        trpc.query(
            accountId,
            path = "memberSpaces.mine",
            input = buildJsonObject { },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = MySpaceAnswer.serializer(),
        )

    /** The members with a space on the device row [rowId]. */
    suspend fun forDevice(accountId: String, rowId: String): List<SpaceMember> =
        trpc.query(
            accountId,
            path = "memberSpaces.forDevice",
            input = SpaceRowIdInput(rowId),
            inputSerializer = SpaceRowIdInput.serializer(),
            outputSerializer = SpaceMembers.serializer(),
        ).members

    /** The device owner's, or a team owner's, Remove. */
    suspend fun remove(accountId: String, spaceId: String) {
        trpc.mutationUnit(
            accountId,
            path = "memberSpaces.remove",
            input = SpaceIdInput(spaceId),
            inputSerializer = SpaceIdInput.serializer(),
        )
    }
}
