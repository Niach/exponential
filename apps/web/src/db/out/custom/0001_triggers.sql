-- Custom triggers for Exponential
-- Applied automatically at every app boot (bootstrap-cloud applyCustomSql —
-- bundled via Vite ?raw, every statement idempotent). Manual application is
-- only needed where the app never boots, e.g. CI:
--   docker exec -i exponential-postgres-1 psql -U postgres -d exponential < src/db/out/custom/0001_triggers.sql

-- 1. Auto-update updated_at timestamp on all tables that carry it. Tables
--    with the board_deleted_at / board_archived_at mirror columns (REV2-5,
--    EXP-500) guard the bump with a WHEN clause: the board fan-outs
--    (propagate_board_deleted_at, propagate_board_archived_at) only flip those
--    mirrors, and bumping updated_at there would stamp a whole board's history
--    as freshly edited on restore or unarchive. The ISSUE-CHILD
--    tables among them additionally guard on board_id (REV-49): issues.move's
--    re-point UPDATEs only change board_id, which is bookkeeping too — a move
--    must not stamp every comment/attachment/... as freshly edited. issues
--    itself keeps only the board_deleted_at guard: the move's own UPDATE
--    (board_id + number + identifier) SHOULD bump the issue. App writes never
--    touch board_deleted_at or board_id outside the move re-point, so the
--    guards are no-ops for them.
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON teams FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON boards FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issues FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON labels FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_statuses FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON fcm_tokens FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON team_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON team_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_subscribers FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_events FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON coding_sessions FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON repositories FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON actions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON automations FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON user_notification_prefs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON email_deliveries FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON widget_configs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON widget_submissions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installations FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installation_links FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installation_repo_grants FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_user_identities FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON mcp_grants FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_number_counters FOR EACH ROW EXECUTE FUNCTION update_updated_at();
-- device_worktrees guards like the board_deleted_at mirrors: the share
-- fan-out (propagate_device_shared_team, #13) only flips shared_team_id, and
-- bumping updated_at there would stamp every worktree as freshly reported on
-- share/unshare. `devices` itself deliberately has NO trigger — the devices
-- router stamps updatedAt explicitly (heartbeat writes both timestamps).
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON device_worktrees FOR EACH ROW
  WHEN (NEW.shared_team_id IS NOT DISTINCT FROM OLD.shared_team_id)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON device_commands FOR EACH ROW EXECUTE FUNCTION update_updated_at();
-- creem_subscriptions is written from BOTH sides (REV2-70): the app's own seat/
-- plan/team-binding updates and the Better Auth Creem plugin's webhook
-- persistence. The plugin's model declares no updatedAt field, so better-auth's
-- adapter never applies an onUpdate hook to it — app-side stamping alone can
-- never cover the webhook path, which is exactly the path a billing dispute is
-- reconstructed from. The trigger covers both writers.
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON creem_subscriptions FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- 2. Auto-generate issue number and identifier per board, allocated from the
--    per-board monotonic counter table issue_number_counters (migration
--    0013). The ON CONFLICT row lock serializes concurrent same-board
--    inserts — the old unlocked SELECT MAX(number)+1 raced under READ
--    COMMITTED and let two inserts commit the same identifier. The counter
--    only ever grows, so deleting the top-numbered issue can never recycle its
--    identifier (old #PREFIX-n mentions and exp/PREFIX-n branches stay
--    unambiguous). The GREATEST clamp self-heals a missing/stale counter row
--    (fresh board, or rows inserted by the pre-counter trigger between
--    `migrate` running and this file being re-applied at boot). The unique
--    index uniq_issues_board_number (migration 0013, renamed in 0032) is the
--    loud backstop: any residual race fails the insert instead of committing
--    a duplicate. Aborted inserts roll the counter back transactionally — a
--    never-committed number being reused is fine.
CREATE OR REPLACE FUNCTION generate_issue_number()
RETURNS TRIGGER AS $$
DECLARE
  next_number integer;
  current_max integer;
  board_prefix text;
BEGIN
  SELECT COALESCE(MAX(number), 0) INTO current_max
  FROM issues
  WHERE board_id = NEW.board_id;

  INSERT INTO issue_number_counters AS c (board_id, counter)
  VALUES (NEW.board_id, current_max + 1)
  ON CONFLICT (board_id) DO UPDATE
    SET counter = GREATEST(c.counter, current_max) + 1
  RETURNING counter INTO next_number;

  SELECT prefix INTO board_prefix
  FROM boards
  WHERE id = NEW.board_id;

  NEW.number := next_number;
  NEW.identifier := board_prefix || '-' || next_number;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER generate_issue_number BEFORE INSERT ON issues FOR EACH ROW EXECUTE FUNCTION generate_issue_number();

-- 3. Bump issue.updated_at when a comment is created/edited/deleted so the
--    issues Electric shape fires an `updated` event on new discussion (keeps
--    "recently active" ordering honest on every client). The board
--    trash/restore and archive/unarchive fan-outs also UPDATE comment rows,
--    and issues.move re-points their board_id — all bookkeeping,
--    not discussion, so none must bump (each would otherwise amplify a
--    trash/archive/move into one issues-UPDATE per comment, REV-49; the move's
--    own issues UPDATE already bumps the issue exactly once).
CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()
RETURNS TRIGGER AS $$
DECLARE
  target_issue uuid;
BEGIN
  IF (TG_OP = 'UPDATE')
    AND (NEW.board_deleted_at IS DISTINCT FROM OLD.board_deleted_at
      OR NEW.board_archived_at IS DISTINCT FROM OLD.board_archived_at
      OR NEW.board_id IS DISTINCT FROM OLD.board_id) THEN
    RETURN NEW;
  END IF;
  IF (TG_OP = 'DELETE') THEN
    target_issue := OLD.issue_id;
  ELSE
    target_issue := NEW.issue_id;
  END IF;
  UPDATE issues SET updated_at = now() WHERE id = target_issue;
  IF (TG_OP = 'DELETE') THEN
    RETURN OLD;
  ELSE
    RETURN NEW;
  END IF;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER bump_issue_updated_at_from_comment
  AFTER INSERT OR UPDATE OR DELETE ON comments
  FOR EACH ROW EXECUTE FUNCTION bump_issue_updated_at_from_comment();

-- 4. Auto-populate team_id on issue_labels from the referenced label,
--    so the Electric shape filter on issue_labels can be team-scoped
--    (stable) instead of label-scoped (rewritten on every label add → 409
--    churn → cascading 502s upstream).
CREATE OR REPLACE FUNCTION populate_issue_label_team_id()
RETURNS TRIGGER AS $$
BEGIN
  SELECT team_id INTO NEW.team_id FROM labels WHERE id = NEW.label_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_label_team_id
  BEFORE INSERT ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_label_team_id();

-- 5. Auto-populate team_id on issue-child tables from the referenced
--    issue's board, so their Electric shape filters can be team-scoped
--    (stable). issues have no direct team_id, so resolve it via
--    issues → boards (NOT the issue_labels template which reads
--    labels.team_id). A wrong source leaves team_id NULL → NOT NULL
--    violation. Guarded on issue_id: batch-scoped coding_sessions rows
--    (issue_id NULL) carry an explicitly-written team_id that an
--    unguarded SELECT-with-no-row would overwrite with NULL. Every other
--    consumer has issue_id NOT NULL, so the guard is a no-op for them.
CREATE OR REPLACE FUNCTION populate_issue_child_team_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT b.team_id INTO NEW.team_id
    FROM issues i JOIN boards b ON b.id = i.board_id
    WHERE i.id = NEW.issue_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_subscriber_team_id
  BEFORE INSERT ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_team_id();

CREATE OR REPLACE TRIGGER populate_issue_event_team_id
  BEFORE INSERT ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_team_id();

-- 6. coding_sessions (the live "coding now" record, the 14th synced shape):
--    team_id denormalized from issue→board via the shared
--    populate_issue_child_team_id, so its Electric shape filter stays
--    team-scoped and stable.
CREATE OR REPLACE TRIGGER populate_coding_session_team_id
  BEFORE INSERT ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_team_id();

-- 7. Auto-populate board_id + board_deleted_at + board_archived_at on every
--    issue-child synced table from the referenced issue. The two mirrors track
--    the parent board's deleted_at/archived_at (REV2-5, EXP-500) so the member
--    shapes stay trash- and archive-aware via the STATIC predicates
--    `board_deleted_at IS NULL` + `board_archived_at IS NULL` — the old
--    per-user board-id
--    where clauses rotated every board-scoped shape identity on any board
--    create/trash (Electric where clauses are single-table AND part of the
--    shape identity). Covers every writer (tRPC, widget service, attachment
--    storage, MCP) without touching each insert site; overwrites any
--    explicitly-passed value with issue-derived truth, mirroring the team_id
--    pattern. Issues CAN move between boards (EXP-57, issues.move): the
--    triggers also fire on UPDATE OF board_id (the move's re-point UPDATEs),
--    re-deriving both columns from the already-moved issue row, and the
--    FOR KEY SHARE read below closes the race with a concurrent child insert
--    — it blocks against the move's FOR UPDATE row lock, so the trigger
--    always reads the committed post-move board_id (or commits first, where
--    the move's re-point UPDATEs then heal it). The boards row is
--    deliberately NOT locked: a child insert racing the board-trash fan-out
--    can commit with a stale NULL board_deleted_at, but such a row is
--    invisible on every client (its board/issue are out of sync scope) and
--    purge cascade-deletes it — a benign orphan, not worth the
--    trash-vs-insert lock contention. That reasoning is CHILD-ONLY: the
--    issues table's own mirror populate (#8) DOES lock the boards row,
--    because a stale-NULL issues row is squarely in sync scope (the issues
--    shape filters on board_deleted_at alone) — it would sync everywhere
--    while its board doesn't, then vanish in the 48h purge.
--    issue_labels intentionally has BOTH
--    triggers (team_id from the label, board_id from the issue). Guarded on
--    issue_id like the team_id populate: batch-scoped coding_sessions rows
--    (issue_id NULL) keep board_id + board_deleted_at NULL — they span
--    boards and always sync. notifications also carries this trigger
--    (REV-109): issue-less rows — e.g. helpdesk support_reply — keep both
--    NULL and always sync.
CREATE OR REPLACE FUNCTION populate_issue_child_board_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT i.board_id, b.deleted_at, b.archived_at
      INTO NEW.board_id, NEW.board_deleted_at, NEW.board_archived_at
    FROM issues i JOIN boards b ON b.id = i.board_id
    WHERE i.id = NEW.issue_id
    FOR KEY SHARE OF i;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_comment_board_id
  BEFORE INSERT OR UPDATE OF board_id ON comments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_attachment_board_id
  BEFORE INSERT OR UPDATE OF board_id ON attachments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_issue_event_board_id
  BEFORE INSERT OR UPDATE OF board_id ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_issue_subscriber_board_id
  BEFORE INSERT OR UPDATE OF board_id ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_coding_session_board_id
  BEFORE INSERT OR UPDATE OF board_id ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_issue_label_board_id
  BEFORE INSERT OR UPDATE OF board_id ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_notification_board_id
  BEFORE INSERT OR UPDATE OF board_id ON notifications
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

-- 8. Auto-populate team_id + board_deleted_at + board_archived_at on issues
--    from the parent board (REV2-5, EXP-500). team_id makes the issues shape
--    TEAM-scoped (stable across board create/trash/archive); the two mirrors
--    are the shape's static trash and archive predicates. Writers pass the
--    team_id they already resolved for
--    auth (the column is NOT NULL), but this trigger overwrites with
--    board-derived truth, and re-derives both columns when issues.move
--    re-points board_id (moves never cross teams, so team_id is effectively
--    invariant — deriving it anyway keeps the trigger the single source of
--    truth). The boards read takes FOR SHARE (REV-49) — unlike #7's
--    child-only unlocked read — so an issue insert/move serializes against a
--    concurrent board trash (a plain UPDATE's FOR NO KEY UPDATE lock; KEY
--    SHARE would NOT conflict with it): without the lock, a create racing a
--    trash could commit a stale-NULL board_deleted_at issue that syncs on
--    every client while its board doesn't, until the purge silently deletes
--    it. Accepted cost: ANY boards UPDATE (rename included) briefly blocks
--    concurrent inserts/moves on that board; concurrent inserts stay
--    share-compatible with each other.
CREATE OR REPLACE FUNCTION populate_issue_board_context()
RETURNS TRIGGER AS $$
BEGIN
  SELECT b.team_id, b.deleted_at, b.archived_at
    INTO NEW.team_id, NEW.board_deleted_at, NEW.board_archived_at
  FROM boards b
  WHERE b.id = NEW.board_id
  FOR SHARE OF b;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_board_context
  BEFORE INSERT OR UPDATE OF board_id ON issues
  FOR EACH ROW EXECUTE FUNCTION populate_issue_board_context();

-- 9. Fan the board's deleted_at out to every child row's board_deleted_at
--    mirror on trash/restore (REV2-5). This turns board trash/restore into
--    INCREMENTAL shape deltas (Electric emits move-out/move-in ops for just
--    the affected board's rows) instead of a where-clause change that rotated
--    all 8 board-scoped shape identities for every member and forced full
--    cross-team resyncs. The cost is one indexed UPDATE per child table,
--    proportional to the trashed board's own history. updated_at is
--    deliberately preserved (the WHEN guards on the update_updated_at
--    triggers above); purge needs no fan-out — it only hard-deletes boards
--    already stamped deleted_at, whose children are already out of scope.
CREATE OR REPLACE FUNCTION propagate_board_deleted_at()
RETURNS TRIGGER AS $$
BEGIN
  UPDATE issues SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE comments SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE attachments SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE issue_labels SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE issue_subscribers SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE issue_events SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE coding_sessions SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE notifications SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER propagate_board_deleted_at
  AFTER UPDATE ON boards
  FOR EACH ROW
  WHEN (OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
  EXECUTE FUNCTION propagate_board_deleted_at();

-- 9b. The same fan-out for archive/unarchive (EXP-500). Archiving is trash
--     WITHOUT the purge: an archived board and all of its history leave every
--     client's sync scope and every server read surface, and stay that way
--     until an owner unarchives it. Kept as its own function + WHEN clause
--     rather than widened into #9 so each fan-out only rewrites child rows
--     when its OWN column changed — the two markers are independent (a board
--     can be archived and then trashed). Same cost model and the same
--     updated_at preservation (the WHEN guards in #1 cover both mirrors).
CREATE OR REPLACE FUNCTION propagate_board_archived_at()
RETURNS TRIGGER AS $$
BEGIN
  UPDATE issues SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE comments SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE attachments SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE issue_labels SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE issue_subscribers SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE issue_events SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE coding_sessions SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE notifications SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER propagate_board_archived_at
  AFTER UPDATE ON boards
  FOR EACH ROW
  WHEN (OLD.archived_at IS DISTINCT FROM NEW.archived_at)
  EXECUTE FUNCTION propagate_board_archived_at();

-- 10. Seed the 7 locked builtin issue statuses for every NEW team (EXP-314).
--     A trigger rather than teams.create code because teams are inserted from
--     TWO places (trpc/teams.ts and bootstrap-cloud's feedback team) and any
--     future path must never produce a team without builtins — the
--     populate_issue_status_id derivation below depends on them. The VALUES
--     rows mirror contract.json's issueStatusDefaults byte-for-byte
--     (parity-locked by apps/web's domain-contract test); the migration 0052
--     backfill covers pre-existing teams with the same rows. ON CONFLICT via
--     uniq_issue_statuses_team_builtin makes re-application harmless.
CREATE OR REPLACE FUNCTION seed_builtin_issue_statuses()
RETURNS TRIGGER AS $$
BEGIN
  INSERT INTO issue_statuses (team_id, category, name, color, sort_order, builtin_key)
  VALUES
    (NEW.id, 'backlog', 'Backlog', '#A1A1AA', 1, 'backlog'),
    (NEW.id, 'unstarted', 'Todo', '#FAFAFA', 1, 'todo'),
    (NEW.id, 'started', 'In Progress', '#EAB308', 1, 'in_progress'),
    (NEW.id, 'started', 'In Review', '#22C55E', 2, 'in_review'),
    (NEW.id, 'completed', 'Done', '#3B82F6', 1, 'done'),
    (NEW.id, 'cancelled', 'Cancelled', '#A1A1AA', 1, 'cancelled'),
    (NEW.id, 'duplicate', 'Duplicate', '#A1A1AA', 1, 'duplicate')
  ON CONFLICT DO NOTHING;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER seed_builtin_issue_statuses
  AFTER INSERT ON teams
  FOR EACH ROW EXECUTE FUNCTION seed_builtin_issue_statuses();

-- Heal pass: teams inserted in the window between `migrate` (0052) and this
-- file's (re-)application never fired the trigger above — backfill them on
-- every boot (applyCustomSql re-runs this file), so a gap team can never
-- permanently lack its builtins. Idempotent via the partial unique index.
INSERT INTO issue_statuses (team_id, category, name, color, sort_order, builtin_key)
SELECT t.id, d.category::issue_status_category, d.name, d.color, d.sort_order, d.key::issue_status
FROM teams t
CROSS JOIN (VALUES
  ('backlog', 'backlog', 'Backlog', '#A1A1AA', 1),
  ('todo', 'unstarted', 'Todo', '#FAFAFA', 1),
  ('in_progress', 'started', 'In Progress', '#EAB308', 1),
  ('in_review', 'started', 'In Review', '#22C55E', 2),
  ('done', 'completed', 'Done', '#3B82F6', 1),
  ('cancelled', 'cancelled', 'Cancelled', '#A1A1AA', 1),
  ('duplicate', 'duplicate', 'Duplicate', '#A1A1AA', 1)
) AS d(key, category, name, color, sort_order)
WHERE NOT EXISTS (
  SELECT 1 FROM issue_statuses s
  WHERE s.team_id = t.id AND s.builtin_key = d.key::issue_status
)
ON CONFLICT DO NOTHING;

-- 11. Derive issues.status_id from the anchor enum for enum-only writers
--     (EXP-314). New clients dual-write {status_id, status}; old clients,
--     pr-sync, MCP tools, the widget service and the desktop launcher's
--     parking write only the `status` enum — this trigger re-anchors
--     status_id to the team's builtin row for them, so the pair can never
--     disagree. On UPDATE it only acts when the enum changed AND status_id
--     was NOT explicitly changed in the same write: an enum-only re-send of
--     the current anchor (old client "picking" the status it already sees)
--     deliberately leaves a custom status in place. Fires AFTER
--     populate_issue_board_context (BEFORE triggers run alphabetically:
--     'populate_issue_b…' < 'populate_issue_s…'), so NEW.team_id is settled;
--     issues never move across teams (issues.move rejects), so no team-change
--     clause is needed. A missing builtin row (pre-seed race) leaves
--     status_id NULL — clients render the anchor fallback; never an error.
CREATE OR REPLACE FUNCTION populate_issue_status_id()
RETURNS TRIGGER AS $$
BEGIN
  IF TG_OP = 'INSERT' THEN
    IF NEW.status_id IS NULL THEN
      SELECT id INTO NEW.status_id FROM issue_statuses
        WHERE team_id = NEW.team_id AND builtin_key = NEW.status;
    END IF;
  ELSE
    IF NEW.status IS DISTINCT FROM OLD.status
       AND NEW.status_id IS NOT DISTINCT FROM OLD.status_id THEN
      SELECT id INTO NEW.status_id FROM issue_statuses
        WHERE team_id = NEW.team_id AND builtin_key = NEW.status;
    END IF;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_status_id
  BEFORE INSERT OR UPDATE ON issues
  FOR EACH ROW EXECUTE FUNCTION populate_issue_status_id();

-- 12. (EXP-481) Mirror owner + share onto device_worktrees so the shape's
--     where clause stays single-table and its identity only rotates on
--     team-membership changes (REV2-5 stance — no device-id lists in where
--     clauses). The CASE guard is belt-and-braces: setShared is
--     router-enforced server-kind-only, but a non-server share must never
--     scope a shape.
CREATE OR REPLACE FUNCTION populate_device_worktree_owner()
RETURNS TRIGGER AS $$
BEGIN
  SELECT d.user_id,
         CASE WHEN d.kind = 'server' THEN d.shared_team_id END
    INTO NEW.user_id, NEW.shared_team_id
  FROM devices d WHERE d.id = NEW.device_row_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_device_worktree_owner
  BEFORE INSERT ON device_worktrees
  FOR EACH ROW EXECUTE FUNCTION populate_device_worktree_owner();

-- 13. (EXP-481) Fan a devices.shared_team_id change out to the worktree
--     mirrors (the board-trash fan-out pattern): share/unshare becomes
--     incremental move-in/move-out shape deltas, never a where-clause
--     rewrite. Also fires when the kind flips (a device re-registering as
--     desktop must not keep team-scoped mirrors).
CREATE OR REPLACE FUNCTION propagate_device_shared_team()
RETURNS TRIGGER AS $$
BEGIN
  UPDATE device_worktrees
    SET shared_team_id = (CASE WHEN NEW.kind = 'server' THEN NEW.shared_team_id END)
    WHERE device_row_id = NEW.id
      AND shared_team_id IS DISTINCT FROM
          (CASE WHEN NEW.kind = 'server' THEN NEW.shared_team_id END);
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER propagate_device_shared_team
  AFTER UPDATE ON devices
  FOR EACH ROW
  WHEN (OLD.shared_team_id IS DISTINCT FROM NEW.shared_team_id
     OR OLD.kind IS DISTINCT FROM NEW.kind)
  EXECUTE FUNCTION propagate_device_shared_team();

-- Heal pass (idempotent, every boot): worktree mirrors for rows written in
-- the migrate→boot gap or under a pre-trigger binary.
UPDATE device_worktrees w
  SET user_id = d.user_id,
      shared_team_id = CASE WHEN d.kind = 'server' THEN d.shared_team_id END
  FROM devices d
  WHERE d.id = w.device_row_id
    AND (w.user_id IS DISTINCT FROM d.user_id
      OR w.shared_team_id IS DISTINCT FROM
         CASE WHEN d.kind = 'server' THEN d.shared_team_id END);

-- 14. (REV-37) Mirror each user's sorted team-id set onto users.team_ids so
--     the users shape's where clause is `id = me OR team_ids && {my teams}` —
--     bounded by the CALLER's team count. The old clause enumerated every
--     readable co-member id, so its URL grew with instance size until it
--     tripped Electric's ~10KB request-line limit (414 → the users shape
--     died on every client at once). Sorted array => deterministic value;
--     the column is filter-only (excluded from the shape allowlist, the
--     device_worktrees mirror precedent), and a co-member joining/leaving a
--     team becomes an incremental move-in/move-out delta instead of a
--     where-clause rewrite. The FOR UPDATE pre-lock (its own statement, so
--     the recompute starts on a post-wait snapshot) serializes concurrent
--     recomputes for the same user: without it, the loser of two concurrent
--     membership writes would recompute from a stale team_members snapshot
--     and drop the winner's team until the next change (the boot heal pass
--     below is the backstop either way). users.updated_at is app-stamped
--     (better-auth; no update_updated_at trigger on users), so mirror writes
--     never restamp it. UPDATE OF user_id/team_id is belt-and-braces — no
--     writer re-points membership rows today. A deleted user's cascade fires
--     this per removed membership row; the users row is already gone, so the
--     lock+recompute match nothing (harmless no-op).
CREATE OR REPLACE FUNCTION sync_user_team_ids()
RETURNS TRIGGER AS $$
BEGIN
  IF TG_OP IN ('INSERT', 'UPDATE') THEN
    PERFORM 1 FROM users WHERE id = NEW.user_id FOR UPDATE;
    UPDATE users u SET team_ids = (
      SELECT COALESCE(array_agg(tm.team_id ORDER BY tm.team_id), '{}')
      FROM team_members tm WHERE tm.user_id = NEW.user_id
    ) WHERE u.id = NEW.user_id;
  END IF;
  IF TG_OP = 'DELETE'
     OR (TG_OP = 'UPDATE' AND NEW.user_id IS DISTINCT FROM OLD.user_id) THEN
    PERFORM 1 FROM users WHERE id = OLD.user_id FOR UPDATE;
    UPDATE users u SET team_ids = (
      SELECT COALESCE(array_agg(tm.team_id ORDER BY tm.team_id), '{}')
      FROM team_members tm WHERE tm.user_id = OLD.user_id
    ) WHERE u.id = OLD.user_id;
  END IF;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER sync_user_team_ids
  AFTER INSERT OR DELETE OR UPDATE OF user_id, team_id ON team_members
  FOR EACH ROW EXECUTE FUNCTION sync_user_team_ids();

-- Heal pass (idempotent, every boot): membership mirrors for rows written in
-- the migrate→boot gap or under a pre-trigger binary. The IS DISTINCT FROM
-- guard keeps a routine boot from rewriting (and Electric-churning) every
-- users row.
UPDATE users u
  SET team_ids = COALESCE(sub.ids, '{}')
  FROM (
    SELECT u2.id, (SELECT array_agg(tm.team_id ORDER BY tm.team_id)
                     FROM team_members tm WHERE tm.user_id = u2.id) AS ids
    FROM users u2
  ) sub
  WHERE u.id = sub.id AND u.team_ids IS DISTINCT FROM COALESCE(sub.ids, '{}');

-- 15. (REV-12) creem_subscriptions.creem_subscription_id is IMMUTABLE once
--     set. The Creem plugin's webhook persistence (updateSubscriptionFromEvent
--     in @creem_io/better-auth) falls back to matching by creem_customer_id
--     when its id lookup misses — exactly the out-of-order window where a
--     subscription.* event for a NEW subscription lands before its
--     checkout.completed — and then writes the new id onto whatever row it
--     found. That RE-KEYS the customer's existing subscription row: the old
--     subscription (still charging at Creem) loses its local row, so
--     getTeamPlan drops its team to Free, the REV2-30 duplicate guard sees one
--     merged row instead of two live ones, and the team-delete gate and admin
--     console go blind to the charge. Raising aborts only that misdirected
--     UPDATE (the plugin catches and logs it); the new subscription still gets
--     its own row immediately from the bind-path upsert keyed strictly by
--     creem_subscription_id (lib/billing/creem-binding.ts), and its
--     checkout.completed heals the full row data when it lands. Setting the
--     key on a row that never had one (NULL) stays allowed — that is creation,
--     not theft; uniq_creem_subscriptions_creem_subscription_id (migration
--     0071) backstops one-row-per-subscription.
CREATE OR REPLACE FUNCTION reject_creem_subscription_rekey()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'creem_subscriptions.creem_subscription_id is immutable once set (row %: % -> %)',
    OLD.id, OLD.creem_subscription_id, NEW.creem_subscription_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER reject_creem_subscription_rekey
  BEFORE UPDATE OF creem_subscription_id ON creem_subscriptions
  FOR EACH ROW
  WHEN (OLD.creem_subscription_id IS NOT NULL
    AND NEW.creem_subscription_id IS DISTINCT FROM OLD.creem_subscription_id)
  EXECUTE FUNCTION reject_creem_subscription_rekey();
