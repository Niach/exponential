//! The 35 synced shapes (masterplan-v3 §5.9) — the registry the `SyncManager`
//! iterates and the store builds its schema from. gpui-free.
//!
//! Each [`ShapeSpec`] carries the SQLite table name, the kebab-case proxy URL
//! path, the PK kind, and the **exact known-column allowlist** from its
//! `CREATE TABLE` (§5.4): `upsert_row` filters incoming snake_case keys to
//! this list and silently drops unknowns — the conformance fixtures carry a
//! column no build models (`some_future_column`) that must be tolerated and
//! dropped, and a server that adds a column before the desktop updates must
//! never wedge a shape in a rollback loop.
//!
//! The server-side `where`/`requireAuth`/`columns` scoping documented in §5.9
//! is proxy-enforced — the client sends none of it (§5.2).
//!
//! PII rule (§5.4/§5.9): `issue_subscribers` must NOT model an `email` column
//! — the proxy's columns allowlist excludes widget-reporter emails from sync,
//! and the missing local column is the client-side belt to that server-side
//! suspender. Same for `team_invites.token` (REV-4/14): the proxy's
//! allowlist excludes the invite bearer secret (accept is not recipient-bound,
//! so a synced owner-role token would let any member escalate to owner);
//! owners get the token once, from the create mutation. `users` is the
//! opposite: co-member-scoped but full rows including `email`.

/// Primary-key kind of a synced table (§5.9). `issue_labels` is the ONLY
/// composite-PK, id-less table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkKind {
    /// Single `id TEXT PRIMARY KEY` column.
    Id,
    /// Composite `PRIMARY KEY (issue_id, label_id)` — `issue_labels` only.
    IssueLabelPair,
}

/// One entry of the shape registry: display/table name, proxy URL path,
/// known-column allowlist, PK kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeSpec {
    /// SQLite table name == Electric shape name (snake_case).
    pub name: &'static str,
    /// Proxy URL path (kebab-case segments, §5.9).
    pub path: &'static str,
    /// The exact column set of the local `CREATE TABLE`, PK columns first.
    /// This is the §5.4 known-column allowlist.
    pub columns: &'static [&'static str],
    pub pk: PkKind,
}

impl ShapeSpec {
    /// The primary-key column names.
    pub fn pk_columns(&self) -> &'static [&'static str] {
        match self.pk {
            PkKind::Id => &["id"],
            PkKind::IssueLabelPair => &["issue_id", "label_id"],
        }
    }

    /// #FEED-219: a shape newer than some servers this build talks to. Its
    /// 404 means "not served yet", never an outage: the thread parks
    /// without a `PollFailed` (the offline banner stays out of it) and
    /// re-probes every [`crate::client::NOT_SERVED_PARK`].
    pub fn may_be_absent(&self) -> bool {
        MAY_BE_ABSENT.contains(&self.name)
    }

    /// Whether Electric row keys for this shape carry a composite
    /// (trailing-2-segment) primary key.
    pub fn composite_keys(&self) -> bool {
        self.pk == PkKind::IssueLabelPair
    }

    /// The `CREATE TABLE IF NOT EXISTS` DDL (§5.4): every column is `TEXT` —
    /// ONE canonical storage form, normalized at bind time; coercion to native
    /// types happens at hydrate time only (§5.5).
    pub fn ddl(&self) -> String {
        let pk_cols = self.pk_columns();
        let mut sql = format!("CREATE TABLE IF NOT EXISTS \"{}\" (\n", self.name);
        for col in self.columns {
            let not_null = if pk_cols.contains(col) { " NOT NULL" } else { "" };
            sql.push_str(&format!("  \"{col}\" TEXT{not_null},\n"));
        }
        let pk_list = pk_cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&format!("  PRIMARY KEY ({pk_list})\n)"));
        sql
    }
}

/// The 35 shapes, in §5.9 order (#EXP-1656 appended `skills`; #FEED-219
/// added `composer_drafts` after `issue_drafts`). Column sets mirror `packages/db-schema`
/// (minus the §5.4 exclusions: no `email` on `issue_subscribers`, web-only
/// billing fields dropped from `users`, no `body` on `actions`, no scoping
/// mirrors on `device_worktrees`).
pub const SHAPES: [ShapeSpec; 35] = [
    ShapeSpec {
        name: "teams",
        path: "/api/shapes/teams",
        // Teams are always private — no is_public/public_write_policy.
        // A pre-fix install keeps those as orphaned local TEXT columns
        // (heal_missing_columns is additive-only); the allowlist drops the
        // keys on upsert — the same fate as `helpdesk_enabled` since SLOP-4
        // (the helpdesk is gone; the server shape dropped the column). The
        // EXP-319 pr_* columns (PR automation targets: NULL status id =
        // builtin default, automation=false = "do nothing") heal via
        // heal_missing_columns, which ALTERs them onto existing store tables
        // and stamps a refetch so old rows get real values, not NULLs.
        columns: &[
            "id",
            "name",
            "slug",
            "icon_url",
            "pr_opened_status_id",
            "pr_opened_automation",
            "pr_merged_status_id",
            "pr_merged_automation",
            // EXP-711: the merge-ends-sessions toggle.
            "end_sessions_on_merge",
            // EXP-1105: yolo mode (hides Files/Source Control).
            "yolo_mode",
            // #EXP-1709: the merge train's mode (auto | approval).
            "merge_mode",
            // EXP-630: the estimate scale (Settings → Issues).
            "estimation_type",
            // VAPP-201: the team time zone (app hosts read dates in it).
            "time_zone",
            // #EXP-1735: the team's lead Assistant (Settings → Agents row).
            "lead_assistant_id",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "boards",
        path: "/api/shapes/boards",
        columns: &[
            "id",
            "team_id",
            "name",
            "slug",
            "prefix",
            "color",
            // Nullable repo + icon. `heal_missing_columns` ALTERs these onto
            // existing tables on the next open (all TEXT). The dropped
            // public-board columns (`is_public`/`public_show_*`) and the
            // dropped `is_protected` (EXP-364: protected boards are gone —
            // nothing stamped the flag, so the column left the server) linger
            // as orphaned local TEXT columns on pre-drop installs; the
            // allowlist drops the keys on upsert.
            //
            // `archived_at` is absent ON PURPOSE (EXP-500): board archiving
            // came back, but it is enforced entirely server-side — the shape's
            // where clause excludes archived boards, and the archive fan-out
            // pulls their issues out of the issue-child shapes too, so an
            // archived board simply stops arriving here. That is the whole
            // point: the FIRST archiving attempt synced this column and asked
            // all four clients to filter on it, leaked, and was deleted
            // (REV2-103). Do not re-add it — the archived-boards list in
            // settings reads `boards.listArchived` over tRPC instead.
            "icon",
            "repository_id",
            // EXP-712: the board's own branch pin (nullable) — the base every
            // coding session on this board cuts from. `heal_missing_columns`
            // ALTERs it onto an existing local table.
            "default_branch",
            "sort_order",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issues",
        path: "/api/shapes/issues",
        // §5.4 verbatim — no `archived_at` (REV2-103 deleted archiving; the
        // column lingers as an orphaned local TEXT column on pre-drop
        // installs).
        columns: &[
            "id",
            "board_id",
            "number",
            "identifier",
            "title",
            "description",
            // EXP-314: `status` stays the dual-written enum ANCHOR;
            // `status_id` is the precise per-team `issue_statuses` row.
            // `heal_missing_columns` ALTERs it onto existing store tables and
            // the shape-identity rotation's refetch backfills real values.
            "status",
            "status_id",
            "priority",
            "assignee_id",
            "creator_id",
            "source",
            "due_date",
            "sort_order",
            "completed_at",
            "duplicate_of_id",
            // EXP-630: story points (always a point number; the team's
            // `estimation_type` decides how it reads). `heal_missing_columns`
            // ALTERs it onto existing store tables.
            "estimate",
            // EXP-1231: the draft this issue was created from — an open draft
            // page on another client replaces itself with the issue.
            // `heal_missing_columns` ALTERs it onto existing store tables.
            "draft_id",
            // #EXP-1383: an agent assignee (exclusive with `assignee_id`) and
            // the agent whose session filed it. `heal_missing_columns` ALTERs
            // them onto existing store tables.
            "assignee_agent_id",
            "creator_agent_id",
            // #VAPP-229: the app that created it ("via <app>", via-app.json).
            // `heal_missing_columns` ALTERs it onto existing store tables.
            "via_app_id",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "labels",
        path: "/api/shapes/labels",
        columns: &[
            "id",
            "team_id",
            "name",
            "color",
            "sort_order",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issue_labels",
        path: "/api/shapes/issue-labels",
        columns: &["issue_id", "label_id", "team_id", "created_at"],
        pk: PkKind::IssueLabelPair,
    },
    ShapeSpec {
        name: "users",
        path: "/api/shapes/users",
        // The server proxy pins the 6-column contract list — everything else
        // (email_verified, is_admin, billing/onboarding fields) is web- or
        // server-only and NEVER syncs; isAdmin comes from the session
        // (api::accounts), never this shape.
        columns: &[
            "id",
            "name",
            "email",
            "image",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "team_members",
        path: "/api/shapes/team-members",
        columns: &[
            "id",
            "team_id",
            "user_id",
            "role",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "team_invites",
        path: "/api/shapes/team-invites",
        // No `token`: the proxy's columns allowlist excludes the invite
        // bearer secret from sync (see the module header). Pre-fix installs
        // keep an orphaned local `token` column (heal_missing_columns is
        // additive-only) — harmless; the allowlist drops the key on upsert.
        columns: &[
            "id",
            "team_id",
            "invited_by_id",
            "role",
            "email",
            // EXP-630: the placeholder member the invite created — the
            // Members list badges that roster row "invited, not joined" while
            // the invite is unaccepted. `heal_missing_columns` ALTERs it onto
            // existing store tables and refetches the (tiny) table.
            "placeholder_user_id",
            "accepted_at",
            // EXP-1076: NULL = a roster row nobody was ever invited (the
            // Linear import) — the Members list reads "Not invited" and
            // offers a first send, never "Invite expired".
            "sent_at",
            "expires_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "comments",
        path: "/api/shapes/comments",
        columns: &[
            "id",
            "issue_id",
            "team_id",
            // SLOP-4: NULL on a widget reporter's own comment (source
            // `reporter`).
            "author_id",
            "parent_id",
            "source",
            // SLOP-4: `team` | `reporter` — a member comment emailed to the
            // reporter. `heal_missing_columns` ALTERs it onto existing store
            // tables and stamps a refetch (like EXP-741's `source`).
            "audience",
            // #EXP-1383: the agent whose session wrote it.
            "author_agent_id",
            // #VAPP-186: the app whose MCP tool wrote it ("via <app>").
            // `heal_missing_columns` ALTERs it onto existing store tables.
            "via_app_id",
            "body",
            "edited_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "attachments",
        path: "/api/shapes/attachments",
        columns: &[
            "id",
            "team_id",
            "issue_id",
            "board_id",
            "comment_id",
            "uploader_id",
            "filename",
            "content_type",
            "size_bytes",
            "storage_key",
            "url",
            "width",
            "height",
            "duration_ms",
            "poster_storage_key",
            "as_file",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "notifications",
        path: "/api/shapes/notifications",
        columns: &[
            "id",
            "user_id",
            "issue_id",
            // Nullable — set on issue-less `agent_message` / `session_blocked`
            // rows so the inbox row can name the team; NULL on issue-anchored
            // rows (SLOP-4: `reporter_reply` is issue-scoped, the helpdesk's
            // issue-less `support_reply` rows are gone). `heal_missing_columns`
            // ALTERs it onto existing store tables and stamps a refetch so
            // old rows get real values, not NULLs.
            "team_id",
            // EXP-980: nullable — set on issue-less `session_blocked` rows
            // (the run that hit a rate limit) so the inbox row can open it.
            "session_id",
            // #EXP-1534: nullable — set on team-less `agent_signed_out` rows
            // (the devices row that lost a login) so the row opens its settings.
            "device_id",
            "type",
            "title",
            "body",
            "read_at",
            "pushed_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issue_events",
        path: "/api/shapes/issue-events",
        columns: &[
            "id",
            "issue_id",
            "team_id",
            "actor_user_id",
            // #EXP-1383: the agent whose session acted.
            "actor_agent_id",
            "type",
            "payload",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issue_subscribers",
        path: "/api/shapes/issue-subscribers",
        // MUST NOT declare an `email` column (§5.4 — widget-reporter PII is
        // excluded from sync server-side; no local column may exist to leak).
        columns: &[
            "id",
            "issue_id",
            "user_id",
            "team_id",
            "source",
            "unsubscribed",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "sessions",
        path: "/api/shapes/sessions",
        // EXP-1294: THE session tree (roots `thread` | `agent`,
        // children under a root). Byte-matches the proxy's allowlist; the
        // server-only `host_user_id`, `mcp_server_ids` and `instructions`
        // never sync (NEW-2: agent instructions come from `agents.get`).
        // NEW-8: the legacy columns (`kind`, `dispatch`, `caption`, `aside`,
        // `move_to_device_id`, `action_*`, the agent mirrors, …) are never
        // read: a pre-NEW-8 local table keeps them as orphaned TEXT columns
        // (heal_missing_columns is additive-only), nothing hydrates them.
        columns: &[
            "id",
            "team_id",
            "parent_id",
            "root_id",
            "depth",
            // NEW-2: the agent definition a conversation session runs (the
            // kind derives from it).
            "agent_id",
            // VAPP-110: the app whose editor session this root is.
            "app_id",
            "user_id",
            "device_id",
            "cli",
            "account",
            "model",
            "effort",
            "plan_mode",
            "computer_use",
            "ultracode",
            // EXP-1583: the launch's subagent model.
            "subagent_model",
            "repository_id",
            "branch",
            "status",
            "busy",
            "needs_input",
            "blocked",
            "started_at",
            "last_turn_at",
            "ended_at",
            // #EXP-1383: the brief stamp (the instructions hint) and the
            // agent whose session started this root.
            "agent_instructions_at",
            "started_by_agent_id",
            "shared",
            "title",
            // EXP-1458: the tree key (root rows) and the sealed title.
            "seal_public_key",
            "seal_epoch",
            // #FEED-177: a private turn's rotation flag.
            "seal_rotate_epoch",
            "sealed_title",
            "guide",
            // #EXP-1720: the Assistant conversation root's Teams pick (null =
            // every team). `heal_missing_columns` ALTERs it on.
            "scope_team_ids",
            // #VAPP-229: the app that started it ("via <app>", via-app.json;
            // NOT `app_id`, an editor session's app). `heal_missing_columns`
            // ALTERs it onto existing store tables.
            "via_app_id",
            // #EXP-1730: the lists' digest (mirrored from the session's
            // conversation server-side; `conversations` is never read).
            "digest",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "session_issues",
        path: "/api/shapes/session-issues",
        // EXP-1294: root session ↔ issue links. Board-scoped like every
        // issue child; the board mirrors / `shared` / `owner_id` scoping
        // columns stay server-side.
        columns: &[
            "id",
            "session_id",
            "issue_id",
            "team_id",
            "source",
            "added_by",
            "added_by_session_id",
            "sort_order",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "messages",
        path: "/api/shapes/messages",
        // EXP-1294: every session's conversation (person, agent and system
        // rows; `card` jsonb). `consumed_at` NULL on a person's row = queued
        // (the device's pickup reads this shape). Scoping columns
        // (`shared`, `owner_id`) stay server-side. #EXP-1730: a row lives
        // in its session (`session_id`, `domain::message_session::
        // message_session`); `conversation_id` is synced, never read.
        columns: &[
            "id",
            "conversation_id",
            // #EXP-1730: the row's session (THE key every reader uses).
            "session_id",
            "root_id",
            "team_id",
            "author_kind",
            "author_id",
            // EXP-1323: the session that posted it (a child's report or question).
            "author_session_id",
            "body",
            "card",
            // FEED-180: the clear attachments of a (possibly sealed) message.
            "attachment_ids",
            // #EXP-1584: the forwarded issues' section the agent reads.
            "issue_brief",
            "answered_at",
            "consumed_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "conversations",
        path: "/api/shapes/conversations",
        // NEW-8: the conversation every message lives in (kind `session`,
        // one per session, or `dm` / `channel` with no session). Byte-matches
        // the proxy's allowlist (apps/web routes/api/shapes/conversations.ts).
        // #EXP-1380: `digest` = the lists' digest of the messages.
        // #EXP-1730: no reader left (messages carry `session_id`, sessions
        // their `digest`); the table stays for a Local desktop's store, whose
        // fixture-locked `sessions.send` (local-parity.json) still writes it.
        // The CLI daemon and eDE no longer subscribe.
        columns: &["id", "team_id", "kind", "session_id", "title", "digest", "created_at", "updated_at"],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "pull_requests",
        path: "/api/shapes/pull-requests",
        // EXP-1294: ALL pull request state (it left `issues`).
        // `base_pull_request_id` = the explicit stack edge.
        columns: &[
            "id",
            "team_id",
            "root_session_id",
            "session_id",
            "repository_id",
            "number",
            "url",
            "title",
            "head",
            "base",
            "base_pull_request_id",
            "state",
            "merged_at",
            // #EXP-1610: the aggregate CI state (contract prChecksState).
            "checks_state",
            // #EXP-1709 (merge-train.json STATE): the train's state of the
            // PR; server-only: `merge_ready_at`, `merge_approved_at`.
            "merge_state",
            "merge_wait",
            "merge_attempts",
            "merge_hold",
            "merge_queued_since",
            "merge_now",
            "merge_position",
            "merge_train_id",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "pull_request_issues",
        path: "/api/shapes/pull-request-issues",
        // EXP-1294: PR ↔ issue links (a PR closing several issues carries
        // several rows). Board mirrors stay server-side.
        columns: &[
            "id",
            "pull_request_id",
            "issue_id",
            "team_id",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "pull_request_checks",
        path: "/api/shapes/pull-request-checks",
        // #EXP-1610: one CI check (by name) at a PR's head commit, team-
        // scoped like `pull_requests`. Byte-matches the proxy's allowlist
        // (`queued_flagged_at`, the watchdog's flag, stays server-side).
        columns: &[
            "id",
            "team_id",
            "pull_request_id",
            "head_sha",
            "name",
            "status",
            "conclusion",
            "required",
            "started_at",
            "completed_at",
            "details_url",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issue_statuses",
        path: "/api/shapes/issue-statuses",
        // EXP-314 — one team's status vocabulary. Team-scoped like `labels`
        // (statuses aren't board children, so no trash predicate).
        // `builtin_key` marks the 7 locked rows; `color` is only RENDERED for
        // custom rows (builtins render their theme token — see
        // `domain::statuses`).
        columns: &[
            "id",
            "team_id",
            "category",
            "name",
            "color",
            "sort_order",
            "builtin_key",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "devices",
        path: "/api/shapes/devices",
        // EXP-481: the per-user device registry (own rows + team-shared
        // server rows), server-authoritative launch defaults included.
        // `user_id` IS synced — the mine-vs-shared split and owner-name
        // resolution key on it (a sharing owner is always a co-member, so
        // the users shape covers the lookup). Byte-matches the proxy's
        // allowlist (apps/web routes/api/shapes/devices.ts).
        columns: &[
            "id",
            "user_id",
            "device_id",
            "label",
            "kind",
            "platform",
            // EXP-924: the owner-picked glyph (NULL = the kind default).
            "icon",
            "version",
            "agents",
            "caps",
            "unauthed_agents",
            // EXP-749: the ACP-ready subset of `agents` (NULL = assume all).
            "acp_agents",
            "launch_defaults",
            "launch_defaults_updated_at",
            // EXP-484: the read-only per-agent status the machine reports —
            // who is signed in, and how much of each rate-limit window is
            // spent. `agent_usage_at` is the server's write stamp and must
            // never become a sync-nudge trigger (it moves every few minutes;
            // the desktop's device watch keys on `launch_defaults_updated_at`
            // alone).
            "agent_accounts",
            "agent_usage",
            "agent_usage_at",
            // EXP-1196: the device-built readiness report (NULL = an older
            // build). It was missing from this list (the proxy always served
            // it), so `DeviceRow.doctor` hydrated None and the chat screen and
            // device settings drew no readiness block for shared machines.
            // `heal_missing_columns` ALTERs it onto existing store tables and
            // stamps a refetch so old rows get real values, not NULLs.
            "doctor",
            "active_sessions",
            "last_seen_at",
            // FEED-33: `uuid[]`, every team the machine is shared with
            // (`{}` = private); replaced the single `shared_team_id`.
            "shared_team_ids",
            // EXP-622: the owner's default machine. It was missing from this
            // list (the proxy always served it), so `DeviceRow.is_default`
            // hydrated None on the desktop and every picker fell back to
            // "no default" — fixed with the EXP-484 columns.
            "is_default",
            "update_requested_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "device_worktrees",
        path: "/api/shapes/device-worktrees",
        // EXP-481: per-device worktree inventory (resume offers + the
        // device-settings worktree list). The trigger-maintained scoping
        // mirrors (`user_id`/`shared_team_ids`) are proxy-excluded and MUST
        // NOT be modeled locally (the issue_subscribers email stance).
        columns: &[
            "id",
            "device_row_id",
            "repo_full_name",
            "branch",
            "issue_identifier",
            "agents",
            "dirty",
            "busy",
            "reported_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issue_relations",
        path: "/api/shapes/issue-relations",
        // EXP-736: the relation graph (blocks / parent / duplicate /
        // related), a board-scoped child of the SOURCE issue. Byte-matches
        // the proxy's allowlist (apps/web routes/api/shapes/
        // issue-relations.ts) — the board-hide mirrors are scoping columns
        // and stay server-side, like every other child shape.
        columns: &[
            "id",
            "issue_id",
            "related_issue_id",
            "type",
            "source",
            "team_id",
            "board_id",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "pins",
        path: "/api/shapes/pins",
        // EXP-778: personal pins (issue | session; NEW-8: `action_id` and
        // the legacy `action` kind are never read), a static
        // per-user shape (`user_id = me`) that is NOT team/trash scoped —
        // the rail filters to the active team and hides any row whose
        // target no longer resolves in the other collections. Byte-matches
        // the proxy's allowlist (apps/web routes/api/shapes/pins.ts).
        columns: &[
            "id",
            "user_id",
            "team_id",
            "kind",
            "issue_id",
            "session_id",
            "sort_order",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issue_drafts",
        path: "/api/shapes/issue-drafts",
        // EXP-878: the caller's unfiled create-issue dialogs. Like `pins` a
        // STATIC per-user shape (`user_id = me`), neither team- nor
        // trash-scoped: a row renders only once its board resolves from the
        // boards collection (which already carries membership + trash).
        // `label_ids` is a Postgres `uuid[]` on the wire (the
        // `devices.shared_team_ids` form). Byte-matches the proxy's
        // allowlist (apps/web routes/api/shapes/issue-drafts.ts).
        columns: &[
            "id",
            "user_id",
            "team_id",
            "board_id",
            "title",
            "description",
            "status_id",
            "priority",
            "assignee_id",
            // #EXP-1383: an agent assignee (exclusive with `assignee_id`).
            "assignee_agent_id",
            "label_ids",
            "due_date",
            // EXP-1294: the session root a draft files into.
            "session_id",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "composer_drafts",
        path: "/api/shapes/composer-drafts",
        // #FEED-219: every composer's synced draft (composer-draft.json). A
        // STATIC per-user shape (`user_id = me`) like `issue_drafts`; one
        // row per (user, key), `key` = `<kind>:<id>`. Byte-matches the
        // proxy's allowlist (apps/web routes/api/shapes/composer-drafts.ts).
        // A server that predates it 404s: the thread parks quietly
        // ([`ShapeSpec::may_be_absent`]).
        columns: &["id", "user_id", "team_id", "key", "text", "created_at", "updated_at"],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "session_reads",
        path: "/api/shapes/session-reads",
        // EXP-1323: the caller's read marks of sessions (`sessions.markRead`),
        // the Chats list's bold rows + nav dot. Static per user like
        // `pins`. Byte-matches the proxy's allowlist (apps/web
        // routes/api/shapes/session-reads.ts).
        // #EXP-1388: `archived_at` = the viewer's per-chat archive.
        // Still in the shape for old clients (#FEED-191); a later migration
        // removes it.
        columns: &[
            "id",
            "user_id",
            "session_id",
            "read_at",
            "archived_at",
            "story_seen_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "agents",
        path: "/api/shapes/agents",
        // NEW-2: the team's agent DEFINITIONS. Byte-matches the proxy's
        // allowlist (apps/web routes/api/shapes/agents.ts): `instructions`
        // and `trigger_state` stay server-only (tRPC `agents.get`).
        columns: &[
            "id",
            "team_id",
            "name",
            "icon",
            "color",
            "triggers",
            "device_id",
            "cli",
            "account",
            "model",
            // #EXP-1730: the runner defaults as one object (the ONLY one read:
            // `domain::rows::AgentDefRow`); the four columns above stay for
            // the proxy byte-match until phase B drops them.
            "defaults",
            // #EXP-1656: the skills every run of the agent starts with.
            "skill_ids",
            // #EXP-1793: the MCP servers every run of the agent starts with.
            "mcp_server_ids",
            "owner_id",
            "sort_order",
            // S-3: assistant | agent (an Assistant lists for its owner only).
            "role",
            // #EXP-1383: when the instructions text last changed.
            "instructions_updated_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "events",
        path: "/api/shapes/events",
        // NEW-2: the team's event stream (agent event triggers listen on
        // `name`; the server fires them). Byte-matches the proxy.
        columns: &[
            "id",
            "team_id",
            "name",
            "payload",
            "emitted_by",
            "session_id",
            "user_id",
            "created_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "prompts",
        path: "/api/shapes/prompts",
        // NEW-2: saved prompts (the composer's `/` menu). MUST NOT declare a
        // `body` column: the body stays server-only (tRPC `prompts.get`).
        columns: &["id", "team_id", "name", "icon", "sort_order", "created_at", "updated_at"],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "apps",
        path: "/api/shapes/apps",
        // VAPP-110: the team's apps. Byte-matches the proxy; the HEAD
        // version's `package` rides along (the editor's live preview).
        columns: &[
            "id",
            "team_id",
            "slug",
            "name",
            "icon",
            "color",
            "description",
            "package",
            "head_version",
            "published_version",
            "draft_package",
            "created_by",
            "granted_tools",
            // #EXP-1730: the app's agent (NULL = standalone).
            "agent_id",
            "deleted_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "app_versions",
        path: "/api/shapes/app-versions",
        // VAPP-110: an app's versions. MUST NOT declare `package`: an old
        // version's package loads on demand (tRPC `apps.get {version}`).
        columns: &["id", "app_id", "team_id", "number", "note", "session_id", "created_by", "created_at", "migration_state", "migration_total", "migration_done", "migration_error"],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "app_records",
        path: "/api/shapes/app-records",
        // VAPP-122: an app's records (its own data). Byte-matches the
        // proxy; `deleted_at` + the `app_deleted_at` mirror stay server-side
        // (a trashed record simply leaves the shape). VAPP-187: `author_id`
        // (whose record it is) + `updated_by` (the last writer). #VAPP-229:
        // `via_app_id` (the app whose tool created it), VAPP-241: `share_name`
        // (the public link that created it); both healed onto existing stores
        // by `heal_missing_columns`.
        columns: &["id", "app_id", "team_id", "kind", "data", "created_by", "author_id", "updated_by", "via_app_id", "share_name", "created_at", "updated_at"],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "skills",
        path: "/api/shapes/skills",
        // #EXP-1656: the team's skills (Settings › Skills, the "+" menu
        // pick, the agent page). Byte-matches the proxy. MUST NOT declare a
        // `body` column: the SKILL.md body stays server-only (tRPC
        // `skills.get`; the running device gets it from
        // `skills.resolveForLaunch`).
        columns: &["id", "team_id", "name", "description", "enabled_by_default", "builtin", "created_by_id", "created_at", "updated_at"],
        pk: PkKind::Id,
    },
];

/// Shapes a server older than this build may not serve (#FEED-219).
pub const MAY_BE_ABSENT: &[&str] = &["composer_drafts"];

/// Look a shape up by its table name.
pub fn shape_by_name(name: &str) -> Option<&'static ShapeSpec> {
    SHAPES.iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_34_shapes_with_kebab_paths() {
        assert_eq!(SHAPES.len(), 35);
        for spec in &SHAPES {
            assert!(spec.path.starts_with("/api/shapes/"), "{}", spec.name);
            assert!(!spec.path.contains('_'), "paths are kebab-case: {}", spec.path);
            // PK columns are part of the column allowlist.
            for pk in spec.pk_columns() {
                assert!(spec.columns.contains(pk), "{} missing pk {pk}", spec.name);
            }
            // No duplicate columns.
            let mut cols: Vec<_> = spec.columns.to_vec();
            cols.sort_unstable();
            cols.dedup();
            assert_eq!(cols.len(), spec.columns.len(), "{} dup column", spec.name);
        }
        assert_eq!(
            SHAPES.iter().filter(|s| s.composite_keys()).count(),
            1,
            "issue_labels is the only composite-PK shape"
        );
    }

    /// EXP-824: the attachments column list is the server's allowlist byte
    /// for byte — `duration_ms` and `poster_storage_key` drive the media
    /// tiles, `board_id` the trash mirror, EXP-1247 `as_file` the Files list.
    #[test]
    fn attachments_model_the_media_columns() {
        let spec = shape_by_name("attachments").unwrap();
        assert_eq!(
            spec.columns.to_vec(),
            vec![
                "id",
                "team_id",
                "issue_id",
                "board_id",
                "comment_id",
                "uploader_id",
                "filename",
                "content_type",
                "size_bytes",
                "storage_key",
                "url",
                "width",
                "height",
                "duration_ms",
                "poster_storage_key",
                "as_file",
                "created_at",
                "updated_at",
            ]
        );
    }

    #[test]
    fn issue_subscribers_never_models_email() {
        let spec = shape_by_name("issue_subscribers").unwrap();
        assert!(!spec.columns.contains(&"email"));
    }

    #[test]
    fn notifications_model_team_id_for_issueless_rows() {
        // EXP-801/EXP-980: issue-less `agent_message` / `session_blocked`
        // rows carry their team — the inbox row's only handle on its name.
        let spec = shape_by_name("notifications").unwrap();
        assert!(spec.columns.contains(&"team_id"));
    }

    #[test]
    fn teams_never_model_the_helpdesk_flag() {
        // SLOP-4: the helpdesk is gone; the server shape dropped the column.
        let spec = shape_by_name("teams").unwrap();
        assert!(!spec.columns.contains(&"helpdesk_enabled"));
    }

    #[test]
    fn comments_model_source_and_audience() {
        // EXP-741 `source` (`via MCP`) + SLOP-4 `audience` (`to reporter`).
        let spec = shape_by_name("comments").unwrap();
        assert!(spec.columns.contains(&"source"));
        assert!(spec.columns.contains(&"audience"));
    }

    #[test]
    fn notifications_model_session_id_for_blocked_runs() {
        // EXP-980: issue-less `session_blocked` rows carry the run they are
        // about — the inbox row's only handle on which run to open.
        let spec = shape_by_name("notifications").unwrap();
        assert!(spec.columns.contains(&"session_id"));
    }

    #[test]
    fn notifications_model_device_id_for_signed_out_agents() {
        // #EXP-1534: team-less `agent_signed_out` rows carry the device that
        // lost a login — the inbox row's only handle on which settings to open.
        let spec = shape_by_name("notifications").unwrap();
        assert!(spec.columns.contains(&"device_id"));
    }

    #[test]
    fn issues_never_models_deleted_columns() {
        let spec = shape_by_name("issues").unwrap();
        // Recurrence was removed repo-wide by EXP-107, and the time-of-day
        // `due_time`/`end_time` columns by REV2-49 (the due DATE stays).
        // Neither exists on the server anymore, so neither may be requested.
        assert!(!spec.columns.contains(&"recurrence_interval"));
        assert!(!spec.columns.contains(&"recurrence_unit"));
        assert!(!spec.columns.contains(&"due_time"));
        assert!(!spec.columns.contains(&"end_time"));
        assert!(spec.columns.contains(&"due_date"));
    }

    #[test]
    fn issues_sync_the_custom_status_row_pointer() {
        // EXP-314: dropping `status_id` from the allowlist silently degrades
        // every board to anchor-only grouping (custom statuses vanish).
        let spec = shape_by_name("issues").unwrap();
        assert!(spec.columns.contains(&"status"), "the anchor stays");
        assert!(spec.columns.contains(&"status_id"));
        let statuses = shape_by_name("issue_statuses").unwrap();
        assert!(statuses.columns.contains(&"builtin_key"));
        assert!(statuses.columns.contains(&"category"));
    }

    #[test]
    fn issues_sync_the_estimate_and_teams_the_scale() {
        // EXP-630: the estimate chip reads `issues.estimate` on the team's
        // `estimation_type` — dropping either silently hides every estimate.
        let spec = shape_by_name("issues").unwrap();
        assert!(spec.columns.contains(&"estimate"));
        let teams = shape_by_name("teams").unwrap();
        assert!(teams.columns.contains(&"estimation_type"));
    }

    #[test]
    fn issues_sync_the_draft_they_were_created_from() {
        // EXP-1231: `issues.draft_id` is how an open draft page learns its
        // draft was created elsewhere — dropping it strands the page.
        let spec = shape_by_name("issues").unwrap();
        assert!(spec.columns.contains(&"draft_id"));
    }

    #[test]
    fn composer_drafts_match_the_proxy_and_may_be_absent() {
        let spec = shape_by_name("composer_drafts").unwrap();
        assert_eq!(spec.path, "/api/shapes/composer-drafts");
        assert_eq!(
            spec.columns,
            &["id", "user_id", "team_id", "key", "text", "created_at", "updated_at"]
        );
        assert!(spec.may_be_absent());
        assert!(!shape_by_name("issue_drafts").unwrap().may_be_absent());
    }

    #[test]
    fn actions_left_the_registry() {
        // NEW-8: prompts replaced actions; the shape is never subscribed
        // (a 404 after the server drop would retry forever).
        assert!(shape_by_name("actions").is_none());
        assert!(shape_by_name("automations").is_none());
    }

    /// NEW-2: agents, events and prompts byte-match the proxies; the
    /// server-only columns never sync.
    #[test]
    fn the_new2_shapes_match_the_proxies() {
        assert_eq!(
            shape_by_name("agents").unwrap().columns,
            &["id", "team_id", "name", "icon", "color", "triggers", "device_id", "cli", "account", "model", "defaults", "skill_ids", "mcp_server_ids", "owner_id", "sort_order", "role", "instructions_updated_at", "created_at", "updated_at"]
        );
        assert_eq!(
            shape_by_name("events").unwrap().columns,
            &["id", "team_id", "name", "payload", "emitted_by", "session_id", "user_id", "created_at"]
        );
        assert_eq!(
            shape_by_name("prompts").unwrap().columns,
            &["id", "team_id", "name", "icon", "sort_order", "created_at", "updated_at"]
        );
        // #EXP-1656: skills sync body-less (apps/web routes/api/shapes/skills.ts).
        assert_eq!(
            shape_by_name("skills").unwrap().columns,
            &["id", "team_id", "name", "description", "enabled_by_default", "builtin", "created_by_id", "created_at", "updated_at"]
        );
        let sessions = shape_by_name("sessions").unwrap();
        assert!(sessions.columns.contains(&"agent_id"));
        assert!(!sessions.columns.contains(&"kind"), "NEW-8: the kind derives from agent_id");
    }

    /// VAPP-110: apps and app_versions byte-match the proxies; an old
    /// version's package never syncs; sessions carry the editor's app.
    #[test]
    fn the_apps_shapes_match_the_proxies() {
        assert_eq!(
            shape_by_name("apps").unwrap().columns,
            &["id", "team_id", "slug", "name", "icon", "color", "description", "package", "head_version", "published_version", "draft_package", "created_by", "granted_tools", "agent_id", "deleted_at", "created_at", "updated_at"]
        );
        let versions = shape_by_name("app_versions").unwrap();
        assert_eq!(
            versions.columns,
            &["id", "app_id", "team_id", "number", "note", "session_id", "created_by", "created_at", "migration_state", "migration_total", "migration_done", "migration_error"]
        );
        assert!(!versions.columns.contains(&"package"));
        // VAPP-122: records byte-match the proxy; the trash columns stay
        // behind the allowlist.
        let records = shape_by_name("app_records").unwrap();
        assert_eq!(records.path, "/api/shapes/app-records");
        assert_eq!(
            records.columns,
            &["id", "app_id", "team_id", "kind", "data", "created_by", "author_id", "updated_by", "via_app_id", "share_name", "created_at", "updated_at"]
        );
        assert!(!records.columns.contains(&"deleted_at"));
        assert!(!records.columns.contains(&"app_deleted_at"));
        let sessions = shape_by_name("sessions").unwrap();
        let agent = sessions.columns.iter().position(|c| *c == "agent_id").unwrap();
        assert_eq!(sessions.columns[agent + 1], "app_id");
    }

    #[test]
    fn teams_sync_the_pr_automation_columns() {
        // EXP-319: the settings PR-automation card reads these four —
        // dropping any silently shows every team as builtin-default.
        let spec = shape_by_name("teams").unwrap();
        assert!(spec.columns.contains(&"pr_opened_status_id"));
        assert!(spec.columns.contains(&"pr_opened_automation"));
        assert!(spec.columns.contains(&"pr_merged_status_id"));
        assert!(spec.columns.contains(&"pr_merged_automation"));
        // EXP-711: the same card's merge-ends-sessions switch.
        assert!(spec.columns.contains(&"end_sessions_on_merge"));
        assert!(spec.columns.contains(&"yolo_mode"));
        // EXP-630: the Issues pane's estimate scale.
        assert!(spec.columns.contains(&"estimation_type"));
    }

    #[test]
    fn device_worktrees_never_model_the_scoping_mirrors() {
        // EXP-481: `user_id`/`shared_team_ids` are server-side scoping
        // mirrors, proxy-excluded like issue_subscribers.email.
        let spec = shape_by_name("device_worktrees").unwrap();
        assert!(!spec.columns.contains(&"user_id"));
        assert!(!spec.columns.contains(&"shared_team_id"));
        // FEED-33 widened the devices column; the worktree mirror stays out.
        assert!(!spec.columns.contains(&"shared_team_ids"));
        assert!(spec.columns.contains(&"issue_identifier"));
        assert!(spec.columns.contains(&"busy"));
    }

    #[test]
    fn devices_sync_the_authoritative_launch_defaults() {
        // EXP-481: dropping either silently blanks the device-settings
        // dialog's defaults editor and the launch dialog's seeding.
        let spec = shape_by_name("devices").unwrap();
        assert!(spec.columns.contains(&"launch_defaults"));
        assert!(spec.columns.contains(&"launch_defaults_updated_at"));
        assert!(spec.columns.contains(&"last_seen_at"));
        assert!(spec.columns.contains(&"user_id"));
        assert!(spec.columns.contains(&"caps"));
    }

    #[test]
    fn devices_sync_the_agent_status_columns() {
        // EXP-484: dropping any of these silently blanks the Agents section
        // and every usage bar on this client.
        let spec = shape_by_name("devices").unwrap();
        for column in ["agent_accounts", "agent_usage", "agent_usage_at"] {
            assert!(spec.columns.contains(&column), "devices needs {column}");
        }
        // EXP-622's flag was never listed — a picker with no default is the
        // symptom.
        assert!(spec.columns.contains(&"is_default"));
        // EXP-924: the owner-picked glyph. Dropping it silently sends every
        // machine back to its kind default on this client alone.
        assert!(spec.columns.contains(&"icon"));
        // And the session row names the CLI whose windows those are.
        let sessions = shape_by_name("sessions").unwrap();
        assert!(sessions.columns.contains(&"cli"));
    }

    #[test]
    fn devices_sync_the_doctor_report() {
        // EXP-1196: the readiness block on the chat screen and in device
        // settings reads `DeviceRow.doctor`; without the column every shared
        // machine looks report-less on this client alone. The list byte-
        // matches the proxy allowlist (apps/web routes/api/shapes/devices.ts).
        let spec = shape_by_name("devices").unwrap();
        assert!(spec.columns.contains(&"doctor"));
        assert_eq!(
            spec.columns,
            &[
                "id",
                "user_id",
                "device_id",
                "label",
                "kind",
                "platform",
                "icon",
                "version",
                "agents",
                "caps",
                "unauthed_agents",
                "acp_agents",
                "launch_defaults",
                "launch_defaults_updated_at",
                "agent_accounts",
                "agent_usage",
                "agent_usage_at",
                "doctor",
                "active_sessions",
                "last_seen_at",
                "shared_team_ids",
                "is_default",
                "update_requested_at",
                "created_at",
                "updated_at",
            ]
        );
    }

    #[test]
    fn devices_sync_the_acp_agents_column() {
        // EXP-749/EXP-773: without it every remote picker loses its
        // "can't run a session" note and silently claims a start that the
        // target machine will refuse.
        let spec = shape_by_name("devices").unwrap();
        assert!(spec.columns.contains(&"acp_agents"));
        // It is a SUBSET of the runnable list, so both have to sync.
        assert!(spec.columns.contains(&"agents"));
        assert!(spec.columns.contains(&"unauthed_agents"));
    }

    #[test]
    fn team_invites_sync_the_placeholder_binding_but_never_the_token() {
        // EXP-630: the Members list badges a roster row "Invited" / "Invite
        // expired" from THIS column — dropping it silently shows every
        // not-yet-joined member as an ordinary one and hides "Resend invite".
        let spec = shape_by_name("team_invites").unwrap();
        assert!(spec.columns.contains(&"placeholder_user_id"));
        // EXP-1076: without it every imported (never-invited) seat would read
        // "Invite expired" — the wrong copy over the right action.
        assert!(spec.columns.contains(&"sent_at"));
        // REV-4/14: the bearer secret stays out (module header).
        assert!(!spec.columns.contains(&"token"), "bearer secret");
    }

    #[test]
    fn pins_model_every_target_column() {
        // EXP-778: the target id `kind` names is set per row; the rail
        // resolves it. Dropping one silently hides that pin kind. NEW-8: the
        // legacy `action_id` is never read.
        let spec = shape_by_name("pins").unwrap();
        for column in ["user_id", "team_id", "kind", "issue_id", "session_id", "sort_order"] {
            assert!(spec.columns.contains(&column), "pins needs {column}");
        }
        assert!(!spec.columns.contains(&"action_id"));
    }

    #[test]
    fn issue_drafts_model_every_composed_field() {
        // EXP-878: the create dialog restores itself from these columns —
        // dropping one silently reopens a draft with that field blank.
        let spec = shape_by_name("issue_drafts").unwrap();
        for column in [
            "user_id",
            "team_id",
            "board_id",
            "title",
            "description",
            "status_id",
            "priority",
            "assignee_id",
            "label_ids",
            "due_date",
            "session_id",
        ] {
            assert!(spec.columns.contains(&column), "issue_drafts needs {column}");
        }
        // A static per-user shape like `pins`: no board-trash mirror to
        // filter on (the board join does that job client-side).
        assert!(!spec.columns.contains(&"board_deleted_at"));
    }

    /// EXP-1294: the session tree's five shapes byte-match the proxies'
    /// allowlists; the server-only columns never sync.
    #[test]
    fn the_session_tree_shapes_match_the_proxies() {
        let sessions = shape_by_name("sessions").unwrap();
        assert_eq!(sessions.path, "/api/shapes/sessions");
        assert_eq!(sessions.columns.len(), 40);
        // #EXP-1720: the Assistant conversation root's Teams pick.
        for column in ["parent_id", "root_id", "agent_id", "status", "busy", "needs_input", "shared", "guide", "seal_public_key", "seal_epoch", "seal_rotate_epoch", "sealed_title", "scope_team_ids"] {
            assert!(sessions.columns.contains(&column), "sessions needs {column}");
        }
        // NEW-8: the legacy columns are never read (the server drops them).
        for legacy in [
            "kind", "dispatch", "move_to_device_id", "handoff_restored_at", "caption", "aside", "action_id",
            "action_name", "action_inputs", "spawned_by_session_id", "name", "icon", "color", "triggers",
            "sort_order",
        ] {
            assert!(!sessions.columns.contains(&legacy), "{legacy} is never read");
        }
        for server_only in ["host_user_id", "mcp_server_ids", "instructions", "handoff"] {
            assert!(!sessions.columns.contains(&server_only), "{server_only} is server-only");
        }
        assert_eq!(
            shape_by_name("session_issues").unwrap().columns,
            &["id", "session_id", "issue_id", "team_id", "source", "added_by", "added_by_session_id", "sort_order", "created_at", "updated_at"]
        );
        let messages = shape_by_name("messages").unwrap();
        assert_eq!(messages.path, "/api/shapes/messages");
        assert!(messages.columns.contains(&"consumed_at"));
        assert!(!messages.columns.contains(&"owner_id"));
        // #EXP-1730: a message is keyed by its own `session_id` (the legacy
        // `conversation_id` still syncs, never read).
        assert!(messages.columns.contains(&"conversation_id"));
        assert!(messages.columns.contains(&"session_id"));
        assert!(sessions.columns.contains(&"digest"));
        assert_eq!(
            shape_by_name("conversations").unwrap().columns,
            &["id", "team_id", "kind", "session_id", "title", "digest", "created_at", "updated_at"]
        );
        assert_eq!(shape_by_name("conversations").unwrap().path, "/api/shapes/conversations");
        let pulls = shape_by_name("pull_requests").unwrap();
        assert_eq!(pulls.path, "/api/shapes/pull-requests");
        assert!(pulls.columns.contains(&"base_pull_request_id"));
        assert_eq!(
            shape_by_name("pull_request_issues").unwrap().path,
            "/api/shapes/pull-request-issues"
        );
    }

    /// #EXP-1610/#EXP-1611: the CI check rows and the PR's aggregate sync,
    /// byte for byte the proxies' allowlists (apps/web routes/api/shapes/
    /// pull-request-checks.ts, pull-requests.ts).
    #[test]
    fn the_pr_checks_columns_match_the_proxies() {
        let pulls = shape_by_name("pull_requests").unwrap();
        assert!(pulls.columns.contains(&"checks_state"));
        // #EXP-1710: the merge train's columns (the status pill, the `…`,
        // Needs you), and the team's merge mode.
        for column in [
            "merge_state",
            "merge_wait",
            "merge_attempts",
            "merge_hold",
            "merge_queued_since",
            "merge_now",
            "merge_position",
            "merge_train_id",
        ] {
            assert!(pulls.columns.contains(&column), "{column}");
        }
        assert!(!pulls.columns.contains(&"merge_ready_at"));
        assert!(!pulls.columns.contains(&"merge_approved_at"));
        assert!(shape_by_name("teams").unwrap().columns.contains(&"merge_mode"));
        assert!(shape_by_name("teams").unwrap().columns.contains(&"lead_assistant_id"));
        let checks = shape_by_name("pull_request_checks").unwrap();
        assert_eq!(checks.path, "/api/shapes/pull-request-checks");
        assert_eq!(checks.pk, PkKind::Id);
        assert_eq!(
            checks.columns,
            &[
                "id",
                "team_id",
                "pull_request_id",
                "head_sha",
                "name",
                "status",
                "conclusion",
                "required",
                "started_at",
                "completed_at",
                "details_url",
                "created_at",
                "updated_at",
            ]
        );
        assert!(!checks.columns.contains(&"queued_flagged_at"));
    }

    /// #EXP-1383: the agent-actor / agent-assignee / instructions-hint
    /// columns sync (the proxies' allowlists carry them).
    #[test]
    fn the_exp_1383_agent_columns_sync() {
        for (shape, column) in [
            ("agents", "instructions_updated_at"),
            ("sessions", "agent_instructions_at"),
            ("sessions", "started_by_agent_id"),
            ("issues", "assignee_agent_id"),
            ("issues", "creator_agent_id"),
            ("issue_drafts", "assignee_agent_id"),
            ("issue_events", "actor_agent_id"),
            ("comments", "author_agent_id"),
            ("comments", "via_app_id"),
            ("issues", "via_app_id"),
            ("sessions", "via_app_id"),
            ("app_records", "via_app_id"),
            ("app_records", "share_name"),
        ] {
            assert!(shape_by_name(shape).unwrap().columns.contains(&column), "{shape} needs {column}");
        }
        assert!(!shape_by_name("agents").unwrap().columns.contains(&"instructions"));
    }

    /// EXP-1294: the old run and legacy-table shapes are gone, and PR state left
    /// `issues`.
    #[test]
    fn no_coding_session_or_chat_shapes() {
        for gone in ["coding_sessions", "threads", "thread_items"] {
            assert!(shape_by_name(gone).is_none(), "{gone} is gone");
        }
        let issues = shape_by_name("issues").unwrap();
        for gone in ["pr_url", "pr_number", "pr_state", "branch", "pr_merged_at", "pr_base_branch", "pr_stack_number"] {
            assert!(!issues.columns.contains(&gone), "issues.{gone} is gone");
        }
    }

    /// SLOP-3: the workflow shapes are gone from the registry.
    #[test]
    fn no_workflow_shapes() {
        for gone in ["workflows", "workflow_nodes", "workflow_events"] {
            assert!(shape_by_name(gone).is_none(), "{gone} is gone");
        }
    }

    #[test]
    fn ddl_is_all_text_with_pk() {
        let issues = shape_by_name("issues").unwrap().ddl();
        assert!(issues.contains("\"id\" TEXT NOT NULL"));
        assert!(issues.contains("PRIMARY KEY (\"id\")"));
        let labels = shape_by_name("issue_labels").unwrap().ddl();
        assert!(labels.contains("PRIMARY KEY (\"issue_id\", \"label_id\")"));
    }
}
