//! The 19 synced shapes (masterplan-v3 §5.9) — the registry the `SyncManager`
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

/// The 19 shapes, in §5.9 order. Column sets mirror `packages/db-schema`
/// (minus the §5.4 exclusions: no `email` on `issue_subscribers`, web-only
/// billing fields dropped from `users`, no `body` on `actions`, and no
/// scoping mirrors on `device_worktrees`).
pub const SHAPES: [ShapeSpec; 19] = [
    ShapeSpec {
        name: "teams",
        path: "/api/shapes/teams",
        // Teams are always private — no is_public/public_write_policy.
        // A pre-fix install keeps those as orphaned local TEXT columns
        // (heal_missing_columns is additive-only); the allowlist drops the
        // keys on upsert. `helpdesk_enabled` (EXP-180) gates the Support
        // inbox — heal_missing_columns ALTERs it onto existing store tables
        // and stamps a refetch so old rows get real values, not NULLs. The
        // EXP-319 pr_* columns (PR automation targets: NULL status id =
        // builtin default, automation=false = "do nothing") heal the same
        // way.
        columns: &[
            "id",
            "name",
            "slug",
            "icon_url",
            "helpdesk_enabled",
            "pr_opened_status_id",
            "pr_opened_automation",
            "pr_merged_status_id",
            "pr_merged_automation",
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
            "pr_url",
            "pr_number",
            "pr_state",
            "branch",
            "pr_merged_at",
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
            "accepted_at",
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
            "author_id",
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
            "comment_id",
            "uploader_id",
            "filename",
            "content_type",
            "size_bytes",
            "storage_key",
            "url",
            "width",
            "height",
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
            // EXP-180: nullable — set on issue-less `support_reply` rows (the
            // ticket's team) so the inbox can group helpdesk activity per
            // team; NULL on issue-anchored rows. `heal_missing_columns`
            // ALTERs it onto existing store tables and stamps a refetch so
            // old rows get real values, not NULLs.
            "team_id",
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
        name: "coding_sessions",
        path: "/api/shapes/coding-sessions",
        // `issue_id` is nullable — batch-scoped (multi-issue) sessions carry
        // only `team_id`. `needs_input` (EXP-214) is the attention flag the
        // desktop itself writes while the agent is parked on a picker —
        // heal_missing_columns ALTERs it onto existing store tables and
        // stamps a refetch so old rows get real values, not NULLs.
        columns: &[
            "id",
            "issue_id",
            "team_id",
            "user_id",
            "device_label",
            // EXP-549/550: the host machine's steer deviceId — joins the
            // synced `devices` row for its live label and online-ness
            // (heal_missing_columns ALTERs it onto existing stores).
            "device_id",
            "status",
            // EXP-484: which agent CLI runs it — the session views caption
            // the usage bar with it (heals onto existing store tables).
            "agent",
            "needs_input",
            // EXP-545/EXP-698: the head branch `pr_open` stamped on the row.
            // It is what ties a BATCH run (no issue linkage at all) to its
            // OWN pull request, which is how the steer viewer's Merge pill
            // finds a target. Heals onto existing store tables like the rest.
            "branch",
            // EXP-530 automation attribution: `action_id`/`action_name` scope
            // a run to its action (name snapshotted — outlives a deleted
            // row), `started_reason` (`schedule`/`event`, NULL = manual)
            // marks self-started runs. `heal_missing_columns` ALTERs them
            // onto existing store tables and stamps a refetch so old rows get
            // real values, not NULLs.
            "action_id",
            "action_name",
            // EXP-583: the `automations` row that fired the run (NULL on
            // manual starts) — heals onto existing store tables like the rest.
            "automation_id",
            "started_reason",
            // EXP-637 close-out: the agent's own `summary`, who ended the
            // run (`ended_by`) and the ended run a resume came from. The
            // server-only `merged_own_pr` flag is deliberately NOT here
            // (never in the shape allowlist), and EXP-686 dropped `outcome`
            // with the column itself.
            "summary",
            "ended_by",
            "resumed_from_id",
            "started_at",
            "ended_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "actions",
        path: "/api/shapes/actions",
        // MUST NOT declare a `body` column (EXP-268): the proxy's columns
        // allowlist excludes the ≤64KB prompt from sync — runs and editors
        // fetch it fresh via tRPC `actions.get`. `inputs` is the typed
        // input-schema JSON array (stored as TEXT like issue_events.payload).
        columns: &[
            "id",
            "team_id",
            "repository_id",
            "name",
            "description",
            // EXP-273: the action's curated registry glyph (same set as
            // boards.icon). `heal_missing_columns` ALTERs it onto an existing
            // local table, so no hand-written migration.
            "icon",
            "inputs",
            // NO `trigger` (EXP-583): automations are their own row/shape now
            // and the server dropped the column. A pre-drop install keeps an
            // orphaned local TEXT column (heal_missing_columns is
            // additive-only); the allowlist drops the key on upsert.
            "sort_order",
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
            "version",
            "agents",
            "caps",
            "unauthed_agents",
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
            "active_sessions",
            "last_seen_at",
            "shared_team_id",
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
        // mirrors (`user_id`/`shared_team_id`) are proxy-excluded and MUST
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
        name: "automations",
        path: "/api/shapes/automations",
        // EXP-583: one action + one device + one trigger, team-scoped like
        // `actions`. Byte-matches the proxy's allowlist (apps/web
        // routes/api/shapes/automations.ts) — every column is client-relevant,
        // so this is the full row; a future server-only column goes BEHIND it.
        columns: &[
            "id",
            "team_id",
            "action_id",
            "device_id",
            "enabled",
            "trigger",
            "agent",
            "model",
            "effort",
            "sort_order",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
];

/// Look a shape up by its table name.
pub fn shape_by_name(name: &str) -> Option<&'static ShapeSpec> {
    SHAPES.iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_19_shapes_with_kebab_paths() {
        assert_eq!(SHAPES.len(), 19);
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

    #[test]
    fn issue_subscribers_never_models_email() {
        let spec = shape_by_name("issue_subscribers").unwrap();
        assert!(!spec.columns.contains(&"email"));
    }

    #[test]
    fn actions_never_models_body() {
        // EXP-268: the ≤64KB prompt is excluded from sync server-side; no
        // local column may exist to hold a stale copy — runs fetch fresh.
        let spec = shape_by_name("actions").unwrap();
        assert!(!spec.columns.contains(&"body"));
    }

    #[test]
    fn notifications_model_team_id_for_support_grouping() {
        // EXP-180: issue-less `support_reply` rows carry the ticket's team —
        // the inbox's only handle on which Support inbox to open.
        let spec = shape_by_name("notifications").unwrap();
        assert!(spec.columns.contains(&"team_id"));
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
    fn coding_sessions_syncs_the_needs_input_flag() {
        // EXP-214/REV2-9: the amber "Needs input" badge reads this column —
        // dropping it from the allowlist silently kills the badge on desktop.
        let spec = shape_by_name("coding_sessions").unwrap();
        assert!(spec.columns.contains(&"needs_input"));
    }

    #[test]
    fn coding_sessions_syncs_action_attribution() {
        // EXP-530: the Automations tab's last-run/recent-runs read these —
        // dropping any silently shows every automated run as a manual one.
        let spec = shape_by_name("coding_sessions").unwrap();
        assert!(spec.columns.contains(&"action_id"));
        assert!(spec.columns.contains(&"action_name"));
        assert!(spec.columns.contains(&"started_reason"));
    }

    #[test]
    fn coding_sessions_sync_the_batch_pr_branch() {
        // EXP-545/EXP-698: a batch run carries no issue linkage, so the
        // steer viewer's Merge pill resolves its PR through the head branch
        // `pr_open` stamped here — dropping it silently kills Merge on every
        // batch session.
        let spec = shape_by_name("coding_sessions").unwrap();
        assert!(spec.columns.contains(&"branch"));
    }

    #[test]
    fn coding_sessions_sync_the_host_device_id() {
        // EXP-549/550: the coding-now pill joins this to the `devices` row
        // for the machine's renamed label and its online-ness — dropping it
        // silently reverts every session to the stale hostname snapshot and
        // shows lid-closed hosts as live.
        let spec = shape_by_name("coding_sessions").unwrap();
        assert!(spec.columns.contains(&"device_id"));
        assert!(spec.columns.contains(&"device_label"), "the snapshot stays");
    }

    #[test]
    fn coding_sessions_sync_the_run_close_out() {
        // EXP-637: the ended strip, the automation run rows and Resume all
        // read these — dropping any silently turns every finished run back
        // into a bare "Ended" with no summary and no resume target.
        // `merged_own_pr` is SERVER-ONLY and must never appear here; EXP-686
        // dropped `outcome` from the table, so requesting it would break the
        // whole shape.
        let spec = shape_by_name("coding_sessions").unwrap();
        for column in ["summary", "ended_by", "resumed_from_id"] {
            assert!(spec.columns.contains(&column), "coding_sessions needs {column}");
        }
        assert!(!spec.columns.contains(&"merged_own_pr"), "server-only");
        assert!(!spec.columns.contains(&"outcome"), "dropped by EXP-686");
    }

    #[test]
    fn automations_sync_the_whole_binding() {
        // EXP-583: the bound device fires off THIS shape now — dropping any
        // of the four binding columns silently disables every automation.
        let spec = shape_by_name("automations").unwrap();
        for column in ["action_id", "device_id", "enabled", "trigger"] {
            assert!(spec.columns.contains(&column), "automations needs {column}");
        }
        // The per-run overrides (NULL = the device's launch defaults).
        for column in ["agent", "model", "effort"] {
            assert!(spec.columns.contains(&column), "automations needs {column}");
        }
        // The trigger LEFT `actions` with EXP-583 — a client that still asks
        // for the dropped column wedges the whole shape.
        let actions = shape_by_name("actions").unwrap();
        assert!(!actions.columns.contains(&"trigger"));
        // And the run rows point back at the automation that fired them.
        let sessions = shape_by_name("coding_sessions").unwrap();
        assert!(sessions.columns.contains(&"automation_id"));
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
    }

    #[test]
    fn device_worktrees_never_model_the_scoping_mirrors() {
        // EXP-481: `user_id`/`shared_team_id` are server-side scoping
        // mirrors, proxy-excluded like issue_subscribers.email.
        let spec = shape_by_name("device_worktrees").unwrap();
        assert!(!spec.columns.contains(&"user_id"));
        assert!(!spec.columns.contains(&"shared_team_id"));
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
        // And the session row names the agent whose windows those are.
        let sessions = shape_by_name("coding_sessions").unwrap();
        assert!(sessions.columns.contains(&"agent"));
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
