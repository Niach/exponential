package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class AdminUser(
    val id: String,
    val name: String? = null,
    val email: String,
    val image: String? = null,
    val isAdmin: Boolean,
    val createdAt: String? = null,
    val workspaceCount: Int = 0,
    val providers: List<String> = emptyList(),
)

@Serializable
data class AdminOwner(
    val id: String,
    val name: String? = null,
    val email: String,
)

@Serializable
data class AdminWorkspace(
    val id: String,
    val name: String,
    val slug: String,
    val createdAt: String? = null,
    val memberCount: Int = 0,
    val projectCount: Int = 0,
    val owners: List<AdminOwner> = emptyList(),
)

@Serializable
data class SetAdminInput(val userId: String, val isAdmin: Boolean)

@Serializable
data class DeleteUserInput(val userId: String)

@Serializable
data class DeleteWorkspaceInput(val workspaceId: String)

@Serializable
private object AdminEmptyInput

@Singleton
class AdminApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun listUsers(accountId: String): List<AdminUser> =
        trpc.query(
            accountId,
            path = "admin.listUsers",
            input = AdminEmptyInput,
            inputSerializer = AdminEmptyInput.serializer(),
            outputSerializer = kotlinx.serialization.builtins.ListSerializer(AdminUser.serializer()),
        )

    suspend fun setUserAdmin(accountId: String, userId: String, isAdmin: Boolean) {
        trpc.mutation(
            accountId,
            path = "admin.setUserAdmin",
            input = SetAdminInput(userId, isAdmin),
            inputSerializer = SetAdminInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }

    suspend fun deleteUser(accountId: String, userId: String) {
        trpc.mutation(
            accountId,
            path = "admin.deleteUser",
            input = DeleteUserInput(userId),
            inputSerializer = DeleteUserInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }

    suspend fun listWorkspaces(accountId: String): List<AdminWorkspace> =
        trpc.query(
            accountId,
            path = "admin.listWorkspaces",
            input = AdminEmptyInput,
            inputSerializer = AdminEmptyInput.serializer(),
            outputSerializer = kotlinx.serialization.builtins.ListSerializer(AdminWorkspace.serializer()),
        )

    suspend fun deleteWorkspace(accountId: String, workspaceId: String) {
        trpc.mutation(
            accountId,
            path = "admin.deleteWorkspace",
            input = DeleteWorkspaceInput(workspaceId),
            inputSerializer = DeleteWorkspaceInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }
}
