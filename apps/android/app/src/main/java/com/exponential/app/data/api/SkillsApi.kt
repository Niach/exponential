package com.exponential.app.data.api

import com.exponential.app.data.db.SkillEntity
import com.exponential.app.domain.SkillOption
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

// #EXP-1661: team SKILLS (apps/web/src/lib/trpc/skills.ts) on a phone: read
// only. The rows sync through the `skills` shape WITHOUT the body;
// `skills.get` is the only body path. Phones never write a skill.

/** One skill; [body] is empty off the synced shape. */
@Serializable
data class SkillDto(
    val id: String,
    val teamId: String = "",
    val name: String = "",
    val description: String = "",
    val enabledByDefault: Boolean = false,
    /** #VAPP-213: a built-in (read-only, served from the shipped set). */
    val builtin: Boolean = false,
    val body: String = "",
    val createdAt: String = "",
    val updatedAt: String = "",
)

@Serializable
data class SkillResult(val skill: SkillDto)

@Serializable
private data class SkillIdInput(val id: String)

@Singleton
class SkillsApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member read: the full row, body included (the ONLY body path). */
    suspend fun get(accountId: String, id: String): SkillDto = trpc.query(
        accountId,
        path = "skills.get",
        input = SkillIdInput(id),
        inputSerializer = SkillIdInput.serializer(),
        outputSerializer = SkillResult.serializer(),
    ).skill
}

/** A synced row as the pickers' [SkillOption]. */
fun SkillEntity.toSkillOption(): SkillOption =
    SkillOption(id = id, name = name, description = description, enabledByDefault = enabledByDefault, builtin = builtin)
