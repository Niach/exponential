//! The 16 synced shapes (masterplan-v3 §5.9) — the registry the `SyncManager`
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

/// The 16 shapes, in §5.9 order. Column sets mirror `packages/db-schema`
/// (minus the §5.4 exclusions: no `email` on `issue_subscribers`, web-only
/// billing fields dropped from `users`, and no `body` on `actions`).
pub const SHAPES: [ShapeSpec; 16] = [
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
            // public-board columns (`is_public`/`public_show_*`), the dropped
            // `archived_at` (REV2-103: archiving is gone; board trash is the
            // unrelated `deleted_at` feature, server-side scoped) and the
            // dropped `is_protected` (EXP-364: protected boards are gone —
            // nothing stamped the flag, so the column left the server) linger
            // as orphaned local TEXT columns on pre-drop installs; the
            // allowlist drops the keys on upsert.
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
            "status",
            "needs_input",
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
];

/// Look a shape up by its table name.
pub fn shape_by_name(name: &str) -> Option<&'static ShapeSpec> {
    SHAPES.iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_16_shapes_with_kebab_paths() {
        assert_eq!(SHAPES.len(), 16);
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
    fn ddl_is_all_text_with_pk() {
        let issues = shape_by_name("issues").unwrap().ddl();
        assert!(issues.contains("\"id\" TEXT NOT NULL"));
        assert!(issues.contains("PRIMARY KEY (\"id\")"));
        let labels = shape_by_name("issue_labels").unwrap().ddl();
        assert!(labels.contains("PRIMARY KEY (\"issue_id\", \"label_id\")"));
    }
}
