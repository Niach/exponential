package com.exponential.app.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface WorkspaceDao {
    @Query("SELECT * FROM workspaces ORDER BY name")
    fun observeAll(): Flow<List<WorkspaceEntity>>

    @Query("SELECT * FROM workspaces WHERE slug = :slug LIMIT 1")
    fun observeBySlug(slug: String): Flow<WorkspaceEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: WorkspaceEntity)

    @Query("DELETE FROM workspaces WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM workspaces")
    suspend fun clear()
}

@Dao
interface ProjectDao {
    @Query("SELECT * FROM projects WHERE archived_at IS NULL ORDER BY sort_order, name")
    fun observeAll(): Flow<List<ProjectEntity>>

    @Query("SELECT * FROM projects WHERE workspace_id = :workspaceId AND archived_at IS NULL ORDER BY sort_order, name")
    fun observeByWorkspace(workspaceId: String): Flow<List<ProjectEntity>>

    @Query("SELECT * FROM projects WHERE workspace_id = :workspaceId AND slug = :slug LIMIT 1")
    fun observeBySlug(workspaceId: String, slug: String): Flow<ProjectEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ProjectEntity)

    @Query("DELETE FROM projects WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM projects")
    suspend fun clear()
}

@Dao
interface IssueDao {
    @Query("SELECT * FROM issues WHERE project_id = :projectId AND archived_at IS NULL ORDER BY sort_order, created_at")
    fun observeByProject(projectId: String): Flow<List<IssueEntity>>

    @Query("SELECT * FROM issues WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<IssueEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueEntity)

    @Query("DELETE FROM issues WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issues")
    suspend fun clear()
}

@Dao
interface LabelDao {
    @Query("SELECT * FROM labels WHERE workspace_id = :workspaceId ORDER BY sort_order, name")
    fun observeByWorkspace(workspaceId: String): Flow<List<LabelEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: LabelEntity)

    @Query("DELETE FROM labels WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM labels")
    suspend fun clear()
}

@Dao
interface IssueLabelDao {
    @Query("SELECT * FROM issue_labels WHERE issue_id = :issueId")
    fun observeByIssue(issueId: String): Flow<List<IssueLabelEntity>>

    @Query("SELECT * FROM issue_labels WHERE workspace_id = :workspaceId")
    fun observeByWorkspace(workspaceId: String): Flow<List<IssueLabelEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueLabelEntity)

    @Query("DELETE FROM issue_labels WHERE issue_id = :issueId AND label_id = :labelId")
    suspend fun delete(issueId: String, labelId: String)

    @Query("DELETE FROM issue_labels")
    suspend fun clear()
}

@Dao
interface ElectricOffsetDao {
    @Query("SELECT * FROM electric_offsets WHERE shape = :shape LIMIT 1")
    suspend fun get(shape: String): ElectricOffsetEntity?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ElectricOffsetEntity)

    @Query("DELETE FROM electric_offsets WHERE shape = :shape")
    suspend fun deleteShape(shape: String)

    @Query("DELETE FROM electric_offsets")
    suspend fun clear()
}
