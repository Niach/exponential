package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

/** `installs.register` (EXP-1457 / VAPP-37): idempotent, called on every node start. */
@Serializable
data class RegisterInstallInput(
    val kind: String,
    val label: String? = null,
    val platform: String? = null,
    val endpointId: String,
    val x25519Public: String,
    val signedAt: String,
    val signature: String,
)

@Serializable
data class RegisterInstallResult(val installId: String)

@Serializable
data class ListInstallsInput(val teamId: String)

/** One non-revoked install of a team member: its endpoint id is what the node allows in. */
@Serializable
data class TeamInstall(
    val installId: String,
    val userId: String,
    val kind: String,
    val label: String? = null,
    val platform: String? = null,
    val endpointId: String,
    val x25519Public: String,
)

/** One of the caller's own installs (`installs.mine`), revoked ones included. */
@Serializable
data class OwnInstall(
    val installId: String,
    val endpointId: String,
    val revokedAt: String? = null,
)

@Serializable
private object InstallsEmptyInput

@Serializable
data class RevokeInstallInput(val installId: String)

@Singleton
class InstallsApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun register(accountId: String, input: RegisterInstallInput): String =
        trpc.mutation(
            accountId,
            path = "installs.register",
            input = input,
            inputSerializer = RegisterInstallInput.serializer(),
            outputSerializer = RegisterInstallResult.serializer(),
        ).installId

    suspend fun list(accountId: String, teamId: String): List<TeamInstall> =
        trpc.query(
            accountId,
            path = "installs.list",
            input = ListInstallsInput(teamId),
            inputSerializer = ListInstallsInput.serializer(),
            outputSerializer = ListSerializer(TeamInstall.serializer()),
        )

    suspend fun mine(accountId: String): List<OwnInstall> =
        trpc.query(
            accountId,
            path = "installs.mine",
            input = InstallsEmptyInput,
            inputSerializer = InstallsEmptyInput.serializer(),
            outputSerializer = ListSerializer(OwnInstall.serializer()),
        )

    suspend fun revoke(accountId: String, installId: String) {
        trpc.mutationUnit(
            accountId,
            path = "installs.revoke",
            input = RevokeInstallInput(installId),
            inputSerializer = RevokeInstallInput.serializer(),
        )
    }
}
