//! The 15 synced shapes (masterplan-v3 §5.9) — the registry the `SyncManager`
//! iterates and the store builds its schema from. gpui-free.
//!
//! Each [`ShapeSpec`] carries the SQLite table name, the kebab-case proxy URL
//! path, the PK kind, and the **exact known-column allowlist** from its
//! `CREATE TABLE` (§5.4): `upsert_row` filters incoming snake_case keys to
//! this list and silently drops unknowns — the conformance fixtures carry
//! stale pre-GFM columns (`due_time`, `end_time`) that must tolerate-and-drop,
//! and a server that adds a column before the desktop updates must never wedge
//! a shape in a rollback loop.
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

/// The 14 shapes, in §5.9 order. Column sets mirror `packages/db-schema`
/// (minus the §5.4 exclusions: no `due_time`/`end_time` on `issues`, no
/// `email` on `issue_subscribers`, and web-only billing fields dropped from
/// `users`).
pub const SHAPES: [ShapeSpec; 14] = [
    ShapeSpec {
        name: "teams",
        path: "/api/shapes/teams",
        // Teams are always private — no is_public/public_write_policy.
        // A pre-fix install keeps those as orphaned local TEXT columns
        // (heal_missing_columns is additive-only); the allowlist drops the
        // keys on upsert. `helpdesk_enabled` (EXP-180) gates the Support
        // inbox — heal_missing_columns ALTERs it onto existing store tables
        // and stamps a refetch so old rows get real values, not NULLs.
        columns: &[
            "id",
            "name",
            "slug",
            "icon_url",
            "helpdesk_enabled",
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
            // public-board columns (`is_public`/`public_show_*`) linger as
            // orphaned local TEXT columns on pre-drop installs; the allowlist
            // drops the keys on upsert.
            "icon",
            "repository_id",
            // Trash contract: the bootstrap dogfood board is protected —
            // clients disable delete/archive/retype from this synced flag.
            "is_protected",
            "sort_order",
            "archived_at",
            "created_at",
            "updated_at",
        ],
        pk: PkKind::Id,
    },
    ShapeSpec {
        name: "issues",
        path: "/api/shapes/issues",
        // §5.4 verbatim — deliberately NO due_time/end_time (stale pre-GFM
        // wire fields; tolerated-and-dropped by the allowlist, never modeled).
        columns: &[
            "id",
            "board_id",
            "number",
            "identifier",
            "title",
            "description",
            "status",
            "priority",
            "assignee_id",
            "creator_id",
            "due_date",
            "sort_order",
            "completed_at",
            "archived_at",
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
        // Full rows incl. email (co-member-scoped, §5.9). Billing fields
        // (creem_customer_id, had_trial, onboarding_completed_at) are
        // web-only and not modeled — the allowlist drops them.
        columns: &[
            "id",
            "name",
            "email",
            "email_verified",
            "image",
            "is_admin",
            "is_agent",
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
        // only `team_id`.
        columns: &[
            "id",
            "issue_id",
            "team_id",
            "user_id",
            "device_label",
            "status",
            "started_at",
            "ended_at",
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
    fn registry_has_14_shapes_with_kebab_paths() {
        assert_eq!(SHAPES.len(), 14);
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
    fn notifications_model_team_id_for_support_grouping() {
        // EXP-180: issue-less `support_reply` rows carry the ticket's team —
        // the inbox's only handle on which Support inbox to open.
        let spec = shape_by_name("notifications").unwrap();
        assert!(spec.columns.contains(&"team_id"));
    }

    #[test]
    fn issues_never_models_stale_pre_gfm_columns() {
        let spec = shape_by_name("issues").unwrap();
        assert!(!spec.columns.contains(&"due_time"));
        assert!(!spec.columns.contains(&"end_time"));
        // Recurrence was removed repo-wide (EXP-107); the columns no longer
        // exist and must never be requested.
        assert!(!spec.columns.contains(&"recurrence_interval"));
        assert!(!spec.columns.contains(&"recurrence_unit"));
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
