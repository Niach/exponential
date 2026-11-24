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
    // EXP-1459: the steer device id of a desktop / daemon install (addendum 1:
    // a new root wraps for the target device's install); absent = unknown.
    val deviceId: String? = null,
    // #EDE-114 (chat-store.json `catchUp` HOLDERS): newest first; absent = unknown.
    val lastSeenAt: String? = null,
)

/** One of the caller's own installs (`installs.mine`), revoked ones included. */
@Serializable
data class OwnInstall(
    val installId: String,
    val endpointId: String,
    val revokedAt: String? = null,
)

@Serializable
data class ForSessionInput(val sessionId: String)

/** VAPP-33: an install a viewer dials to steer a session over eNet (newest first). */
@Serializable
data class SessionInstall(val endpointId: String, val kind: String)

@Serializable
private object InstallsEmptyInput

@Serializable
data class RevokeInstallInput(val installId: String)

/** #EXP-1950 (fixtures/install-trust.json): an `approve` of an install that existed, carried by the root claim. */
@Serializable
data class RootClaimApproval(val subjectInstallId: String, val signature: String)

@Serializable
data class ClaimRootInput(val installId: String, val signature: String, val approvals: List<RootClaimApproval>? = null)

@Serializable
data class ApproveInstallInput(val installId: String, val subjectInstallId: String, val kind: String, val signature: String)

@Serializable
private data class InstallIdInput(val installId: String)

/**
 * #EXP-1950 (fixtures/install-trust.json): install approval. The server
 * stores and relays; this install judges every signature itself
 * ([com.exponential.app.data.sealed.InstallTrust.reconcile]). A fake (a lying server) in the tests.
 */
interface InstallTrustApi {
    suspend fun trust(accountId: String, installId: String): com.exponential.app.data.sealed.TrustAnswer
    suspend fun requestApproval(accountId: String, installId: String)
    suspend fun claimRoot(accountId: String, installId: String, signature: String, approvals: List<RootClaimApproval>)

    /** `approve` an own install, confirm (`reset`) or turn down (`decline`) a team-mate's new root. */
    suspend fun approve(accountId: String, installId: String, subjectInstallId: String, kind: com.exponential.app.data.sealed.ApprovalKind, signature: String)

    /** A New install row's Remove: its wraps go, its trees rotate. */
    suspend fun revoke(accountId: String, installId: String)
}

@Singleton
class InstallsApi @Inject constructor(private val trpc: TrpcClient) : InstallTrustApi {
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

    suspend fun forSession(accountId: String, sessionId: String): List<SessionInstall> =
        trpc.query(
            accountId,
            path = "installs.forSession",
            input = ForSessionInput(sessionId),
            inputSerializer = ForSessionInput.serializer(),
            outputSerializer = ListSerializer(SessionInstall.serializer()),
        )

    suspend fun mine(accountId: String): List<OwnInstall> =
        trpc.query(
            accountId,
            path = "installs.mine",
            input = InstallsEmptyInput,
            inputSerializer = InstallsEmptyInput.serializer(),
            outputSerializer = ListSerializer(OwnInstall.serializer()),
        )

    override suspend fun revoke(accountId: String, installId: String) {
        trpc.mutationUnit(
            accountId,
            path = "installs.revoke",
            input = RevokeInstallInput(installId),
            inputSerializer = RevokeInstallInput.serializer(),
        )
    }

    override suspend fun trust(accountId: String, installId: String): com.exponential.app.data.sealed.TrustAnswer =
        trpc.query(
            accountId,
            path = "installs.trust",
            input = InstallIdInput(installId),
            inputSerializer = InstallIdInput.serializer(),
            outputSerializer = com.exponential.app.data.sealed.TrustAnswer.serializer(),
        )

    override suspend fun requestApproval(accountId: String, installId: String) {
        trpc.mutationUnit(accountId, path = "installs.requestApproval", input = InstallIdInput(installId), inputSerializer = InstallIdInput.serializer())
    }

    override suspend fun claimRoot(accountId: String, installId: String, signature: String, approvals: List<RootClaimApproval>) {
        trpc.mutationUnit(
            accountId,
            path = "installs.claimRoot",
            input = ClaimRootInput(installId, signature, approvals.takeIf { it.isNotEmpty() }),
            inputSerializer = ClaimRootInput.serializer(),
        )
    }

    override suspend fun approve(
        accountId: String,
        installId: String,
        subjectInstallId: String,
        kind: com.exponential.app.data.sealed.ApprovalKind,
        signature: String,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "installs.approve",
            input = ApproveInstallInput(installId, subjectInstallId, kind.wire, signature),
            inputSerializer = ApproveInstallInput.serializer(),
        )
    }
}
