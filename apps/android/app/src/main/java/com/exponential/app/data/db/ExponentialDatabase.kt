package com.exponential.app.data.db

import androidx.room.Database
import androidx.room.RoomDatabase

@Database(
    entities = [
        TeamEntity::class,
        BoardEntity::class,
        IssueEntity::class,
        LabelEntity::class,
        IssueStatusEntity::class,
        IssueLabelEntity::class,
        IssueRelationEntity::class,
        UserEntity::class,
        TeamMemberEntity::class,
        TeamInviteEntity::class,
        CommentEntity::class,
        AttachmentEntity::class,
        NotificationEntity::class,
        IssueSubscriberEntity::class,
        IssueEventEntity::class,
        CodingSessionEntity::class,
        ActionEntity::class,
        AutomationEntity::class,
        DeviceEntity::class,
        DeviceWorktreeEntity::class,
        ElectricOffsetEntity::class,
    ],
    // v2: added attachments.width / attachments.height (parity with iOS).
    // v3: added 8 agent/PR fields on issues + notifications / issue_subscribers
    //     / issue_events tables (parity with web; the 11th/12th/13th shapes).
    // v4: added users.is_agent (widget helpdesk bot marker).
    // v5: agent_runs table (14th shape) — removed in v6.
    // v6: hard cut — dropped agent_runs + agent/google-calendar issue columns,
    //     added coding_sessions (the new 14th shape) + issues.duplicate_of_id,
    //     issue_subscribers.user_id nullable + email.
    // v7: dropped boards.github_repo — repos now live in the server-only
    //     repositories registry (tRPC `repositories` router, never synced).
    // v8: added boards.repository_id (masterplan v4 §3 — board = repository;
    //     additive column on the existing boards shape, no shape-count change).
    // v9: added electric_offsets.is_live (live-gating: only long-poll live=true
    //     after up-to-date; catch-up polls stay non-live).
    // v10: board types — boards gained type + public_show_* toggles and
    //      repository_id went nullable; teams dropped is_public /
    //      public_write_policy. The six issue-child tables gained a denormalized
    //      board_id column server-side; ignoreUnknownKeys absorbs it (we don't
    //      store it locally). Shapes rotate once on the deploy → destructive
    //      resync repopulates.
    // v11: added boards.is_protected (server-owned dogfood-board flag that
    //      hides delete/archive affordances). Additive column; destructive
    //      fallback wipes + resyncs — acceptable here because this release
    //      already forces one full resync per account via the per-user re-key.
    // v12: caught the local schema up to the current server shape contracts —
    //      boards.deleted_at (trash marker, EXP-26) plus the denormalized
    //      board_id on issue_labels / comments / attachments / issue_events /
    //      coding_sessions and team_invites.invited_by_id (stops the
    //      tolerant-apply "dropped columns" diagnostics noise).
    // v13: team_invites.token dropped from the shape server-side
    //      (bearer-secret leak fix, REV-4/14) — entity field now nullable;
    //      destructive fallback wipes + resyncs (also purging any previously
    //      leaked plaintext tokens from the local cache).
    // v14: releases (EXP-56, the 15th shape) + issues.release_id +
    //      coding_sessions.release_id (issue_id now nullable for
    //      release-scoped orchestrator sessions).
    // v15: boards.public_show_coding removed (EXP-90 — public feedback
    //      coding sessions are gone; the column left the synced shape).
    // v16: releases removed (EXP-106 — releases table + shape dropped, back to
    //      14 shapes) + issues.release_id + coding_sessions.release_id gone
    //      (coding_sessions.issue_id stays nullable for batch multi-issue runs).
    // v17: board-type collapse (EXP-121) — boards gained is_public + icon on
    //      the existing shape (repository stays optional). `type` still syncs
    //      (dual-written server-side) but no longer gates behavior.
    // v18: recurrence removed (EXP-107) — issues.recurrence_interval /
    //      recurrence_unit dropped from the entity + shape; in_review status
    //      (EXP-120) is a plain enum-value string, no schema change.
    //      boards.type dropped entirely (EXP-129 — the column, pg enum and
    //      shape column are gone server-side); the entity field is removed. The
    //      icon fallback now derives from is_public / repository_id instead.
    //      Destructive fallback wipes + resyncs.
    // v19: public feedback boards removed (EXP-180) — boards.is_public /
    //      public_show_comments / public_show_activity dropped from the entity
    //      (and the shape server-side); the icon fallback derives from
    //      repository_id alone. Destructive fallback wipes + resyncs.
    // v20: the great rename (EXP-180) — workspace→team / project→board: local
    //      tables (teams, boards, team_members, team_invites) and wire columns
    //      (team_id / board_id on every child table) renamed to the new server
    //      contract. Destructive fallback wipes + resyncs from the renamed
    //      shapes.
    // v21: teams.helpdesk_enabled (EXP-180 helpdesk) — the synced team-level
    //      flag gating the Support inbox. Additive column on the existing teams
    //      shape; destructive fallback wipes + resyncs.
    // v22: notifications.team_id (EXP-180 helpdesk) — nullable, set on
    //      issue-less support_reply rows so the inbox can group them per team.
    //      Additive column on the existing notifications shape; destructive
    //      fallback wipes + resyncs.
    // v23: team_invites.email (EXP-188 invite-by-email) — optional invited
    //      address, synced for the pending-invite list (the bearer token stays
    //      excluded). Additive column on the existing team-invites shape;
    //      destructive fallback wipes + resyncs.
    // v24: coding_sessions.needs_input (EXP-214) — desktop-written attention
    //      flag while the agent waits on a plan-approval / question picker.
    //      Additive column on the existing coding-sessions shape; destructive
    //      fallback wipes + resyncs.
    // v25: users.is_agent dropped (the synced users shape is now 6 columns);
    //      issues.creator_id went nullable and issues gained source
    //      ('user'|'widget') so widget-filed issues (null creator) decode +
    //      persist. Removing is_agent is decode-safe (ignoreUnknownKeys) and the
    //      dropped column / nullable creator_id / new source column all come back
    //      correctly on the destructive wipe + resync.
    // v26 (EXP-253): coding_sessions.action_id + action_name — action run
    //      linkage on the coding-sessions shape (an action run's row is
    //      batch-shaped with action_name labeling it); actions run labels.
    //      Additive columns; destructive fallback wipes + resyncs.
    // v27 (EXP-264): electric_offsets.needs_refetch — the must-refetch marker
    //      that replaced "wipe the table now, snapshot later" with an atomic
    //      swap (stale rows stay visible until the refetch batch replaces
    //      them). Local bookkeeping only, no shape change; destructive fallback
    //      wipes + resyncs.
    // v28 (EXP-268): actions table — team action prompts became the 15th
    //      Electric shape (columns WITHOUT the ≤64KB `body`, which stays
    //      tRPC-only). Consumers list actions from the local flow now instead
    //      of `actions.list`. Destructive fallback wipes + resyncs.
    // v29 (EXP-273): actions.icon — the action's curated registry glyph, the
    //      same set boards.icon draws from. Additive column on the existing
    //      actions shape; destructive fallback wipes + resyncs.
    // v30 (REV2-103): the SYNCED archiving columns deleted — boards.archived_at
    //      and issues.archived_at dropped from the entities (and the shapes
    //      server-side). Removing a column is decode-safe (ignoreUnknownKeys)
    //      and the destructive fallback wipes + resyncs. EXP-500 later brought
    //      BOARD archiving back WITHOUT a synced column: the server excludes
    //      archived boards from the boards shape and their issues from the
    //      issue-child shapes, so they simply stop arriving and Android needs
    //      no entity field, no DAO filter and no version bump. Do not re-add
    //      the column — per-client filtering is what leaked originally.
    // v31 (REV2-103): due times deleted from the product — issues.due_time and
    //      issues.end_time dropped from the entity (and the schema/shape
    //      server-side); the due DATE stays. Removing a column is decode-safe
    //      (ignoreUnknownKeys) and the destructive fallback wipes + resyncs.
    // v32 (EXP-314): custom issue statuses — the issue_statuses table (the
    //      16th Electric shape: team-scoped rows with category / name / color /
    //      sort_order / builtin_key) plus issues.status_id, the nullable FK to
    //      the issue's status ROW (issues.status stays as the dual-written
    //      builtin anchor). No Migration object: the destructive fallback wipes
    //      the local cache and re-syncs ALL 16 shapes on first launch after the
    //      update — expected, but user-visible as one full re-snapshot.
    // v33 (EXP-364): protected boards deleted from the product —
    //      boards.is_protected dropped from the entity (and the column/shape
    //      server-side). Nothing stamped it anymore after EXP-363, so the
    //      delete affordance is now plain owner-only. Removing a column is
    //      decode-safe (ignoreUnknownKeys) and the destructive fallback wipes +
    //      resyncs.
    // v34 (REV-7): attachments.uploader_id went nullable — the server column
    //      always was (widget screenshots have no human uploader; the FK is ON
    //      DELETE SET NULL) and the shape allowlist carries it, so the required
    //      field dropped widget-screenshot inserts on decode and a SET NULL
    //      partial update bound NULL into the NOT NULL column, failing the
    //      batch before the offset advanced and stalling the shape forever.
    //      Relaxing a constraint needs the destructive fallback's wipe + resync
    //      (Room can't ALTER it away), which also re-delivers the dropped rows.
    // v35 (EXP-481): devices + device_worktrees — the 17th/18th Electric
    //      shapes. The per-user machine registry became server-authoritative
    //      synced state (launch_defaults, worktree inventory, last_seen_at
    //      freshness = online), replacing the 15s `devices.list` polling on
    //      the Agents tab and powering the device-settings sheet + the remote
    //      resume offer. Destructive fallback wipes + resyncs all 18 shapes.
    // v36 (EXP-545): coding_sessions.branch — the batch↔PR linkage stamped by
    //      the server's pr_open batch flip, so a batch row's Merge shortcut
    //      targets its OWN PR instead of the team's sole open batch PR.
    // v37 (EXP-530): actions.trigger + coding_sessions.started_reason —
    //      additive columns; destructive fallback wipes + resyncs
    // v38 (EXP-549/550): coding_sessions.device_id — the host machine's steer
    //      deviceId, joining a session to its LIVE devices row (renamed label,
    //      offline → paused). Additive; destructive fallback wipes + resyncs.
    // v39 (EXP-583): automations became their own entity — the `automations`
    //      table (the 19th Electric shape: team-scoped rows carrying the
    //      action, the bound device, the when-part trigger and the optional
    //      agent/model/effort pins) plus coding_sessions.automation_id, the
    //      nullable link from an automated run back to the row that fired it.
    //      actions.trigger is dead server-side; the local column stays and is
    //      never read again. Destructive fallback wipes + resyncs all 19
    //      shapes on first launch after the update.
    // v40 (EXP-622): devices.is_default — the owner's default machine, the row
    //      every device picker prefills. Additive on an existing shape;
    //      destructive fallback wipes + resyncs.
    // v41 (EXP-637): coding_sessions.summary / outcome / ended_by /
    //      resumed_from_id — the agent's own close-out (`exponential_sessions_end`
    //      writes a summary), who ended the run, and the ended run a Resume
    //      continues. Additive on an existing shape; destructive fallback
    //      wipes + resyncs. (`outcome` left again in v43.)
    // v42 (EXP-484): devices.agent_accounts / agent_usage / agent_usage_at —
    //      the machine's read-only per-agent auth + usage status (jsonb kept as
    //      raw text) and the server stamp of the last usage write — plus
    //      coding_sessions.agent, the agent a run launched with (so a session
    //      view can find its host machine's usage). Additive on two existing
    //      shapes; destructive fallback wipes + resyncs.
    // v43 (EXP-686): coding_sessions.outcome is GONE — the self-reported
    //      done/blocked/no_changes close-out was dropped everywhere (server
    //      column, shape allowlist, every client's badge). A run row now shows
    //      "Running" or nothing at all. Removing a column, so the local cache
    //      has to go: destructive fallback wipes + resyncs on first launch.
    // v44 (EXP-712): boards.default_branch — the board's own branch (worktree
    //      base + PR target), NULL = follow the backing repo's default. New
    //      column on the boards shape allowlist; destructive fallback wipes +
    //      resyncs so every board row arrives carrying it.
    // v45 (EXP-734): coding_sessions.pr_url / pr_number / pr_state — a run's
    //      OWN chore PR (an action or chat run whose PR links no issue). New
    //      columns on the coding-sessions shape allowlist; destructive
    //      fallback wipes + resyncs so every session row arrives carrying them.
    // v46 (EXP-736): issue_relations table (the 20th shape) — the directed
    //      relation edges between issues (blocks / parent / duplicate /
    //      related, plus the auto-linked #IDENT references). New table, so the
    //      destructive fallback wipes + resyncs.
    // No Migration object— DatabaseHolder uses destructive fallback + resync,
    // so a shape column change just wipes and re-syncs from Electric.
    version = 46,
    exportSchema = false,
)
abstract class ExponentialDatabase : RoomDatabase() {
    abstract fun teamDao(): TeamDao
    abstract fun boardDao(): BoardDao
    abstract fun issueDao(): IssueDao
    abstract fun labelDao(): LabelDao
    abstract fun issueStatusDao(): IssueStatusDao
    abstract fun issueLabelDao(): IssueLabelDao
    abstract fun issueRelationDao(): IssueRelationDao
    abstract fun userDao(): UserDao
    abstract fun teamMemberDao(): TeamMemberDao
    abstract fun teamInviteDao(): TeamInviteDao
    abstract fun commentDao(): CommentDao
    abstract fun attachmentDao(): AttachmentDao
    abstract fun notificationDao(): NotificationDao
    abstract fun issueSubscriberDao(): IssueSubscriberDao
    abstract fun issueEventDao(): IssueEventDao
    abstract fun codingSessionDao(): CodingSessionDao
    abstract fun actionDao(): ActionDao
    abstract fun automationDao(): AutomationDao
    abstract fun deviceDao(): DeviceDao
    abstract fun deviceWorktreeDao(): DeviceWorktreeDao
    abstract fun electricOffsetDao(): ElectricOffsetDao
}
