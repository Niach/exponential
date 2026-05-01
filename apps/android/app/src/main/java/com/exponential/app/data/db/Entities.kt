package com.exponential.app.data.db

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Entity(tableName = "workspaces")
@Serializable
data class WorkspaceEntity(
    @PrimaryKey val id: String,
    val name: String,
    val slug: String,
    @ColumnInfo(name = "icon_url") @SerialName("icon_url") val iconUrl: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") val updatedAt: String,
)

@Entity(
    tableName = "projects",
    indices = [Index("workspace_id")],
)
@Serializable
data class ProjectEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") val workspaceId: String,
    val name: String,
    val slug: String,
    val prefix: String,
    val color: String,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") val sortOrder: Double,
    @ColumnInfo(name = "archived_at") @SerialName("archived_at") val archivedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") val updatedAt: String,
)

@Entity(
    tableName = "issues",
    indices = [Index("project_id"), Index("status"), Index("assignee_id"), Index("due_date")],
)
@Serializable
data class IssueEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "project_id") @SerialName("project_id") val projectId: String,
    val number: Int,
    val identifier: String,
    val title: String,
    val description: String? = null,
    val status: String,
    val priority: String,
    @ColumnInfo(name = "assignee_id") @SerialName("assignee_id") val assigneeId: String? = null,
    @ColumnInfo(name = "creator_id") @SerialName("creator_id") val creatorId: String,
    @ColumnInfo(name = "due_date") @SerialName("due_date") val dueDate: String? = null,
    @ColumnInfo(name = "due_time") @SerialName("due_time") val dueTime: String? = null,
    @ColumnInfo(name = "end_time") @SerialName("end_time") val endTime: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") val sortOrder: Double,
    @ColumnInfo(name = "completed_at") @SerialName("completed_at") val completedAt: String? = null,
    @ColumnInfo(name = "archived_at") @SerialName("archived_at") val archivedAt: String? = null,
    @ColumnInfo(name = "recurrence_interval") @SerialName("recurrence_interval") val recurrenceInterval: Int? = null,
    @ColumnInfo(name = "recurrence_unit") @SerialName("recurrence_unit") val recurrenceUnit: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") val updatedAt: String,
)

@Entity(
    tableName = "labels",
    indices = [Index("workspace_id")],
)
@Serializable
data class LabelEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") val workspaceId: String,
    val name: String,
    val color: String,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") val sortOrder: Double,
    @ColumnInfo(name = "created_at") @SerialName("created_at") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") val updatedAt: String,
)

@Entity(
    tableName = "issue_labels",
    primaryKeys = ["issue_id", "label_id"],
    indices = [Index("label_id"), Index("workspace_id")],
)
@Serializable
data class IssueLabelEntity(
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") val issueId: String,
    @ColumnInfo(name = "label_id") @SerialName("label_id") val labelId: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") val workspaceId: String,
)

@Entity(tableName = "electric_offsets")
data class ElectricOffsetEntity(
    @PrimaryKey @ColumnInfo(name = "shape") val shape: String,
    val handle: String,
    val offset: String,
)
