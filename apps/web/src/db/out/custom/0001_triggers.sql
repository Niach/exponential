-- Custom triggers for Exponential
-- Apply after migrations: docker exec -i exponential-postgres-1 psql -U postgres -d exponential < src/db/out/custom/0001_triggers.sql

-- 1. Auto-update updated_at timestamp on all tables that carry it. Tables
--    with a board_deleted_at mirror column (REV2-5) guard the bump with a
--    WHEN clause: the board trash/restore fan-out (propagate_board_deleted_at)
--    only flips board_deleted_at, and bumping updated_at there would stamp a
--    whole board's history as freshly edited on restore. App writes never
--    touch board_deleted_at, so the guard is a no-op for them.
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
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON labels FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON fcm_tokens FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON team_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON team_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_subscribers FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_events FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON coding_sessions FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON repositories FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON run_configs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON user_notification_prefs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON email_deliveries FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON widget_configs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON widget_submissions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installations FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installation_links FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installation_repo_grants FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON mcp_grants FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_number_counters FOR EACH ROW EXECUTE FUNCTION update_updated_at();

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
--    trash/restore fan-out (propagate_board_deleted_at) also UPDATEs comment
--    rows — that is bookkeeping, not discussion, so it must not bump (and
--    would otherwise amplify a trash into one issues-UPDATE per comment).
CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()
RETURNS TRIGGER AS $$
DECLARE
  target_issue uuid;
BEGIN
  IF (TG_OP = 'UPDATE')
    AND (NEW.board_deleted_at IS DISTINCT FROM OLD.board_deleted_at) THEN
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

-- 7. Auto-populate board_id + board_deleted_at on every issue-child synced
--    table from the referenced issue. board_deleted_at mirrors the parent
--    board's deleted_at (REV2-5) so the member shapes stay trash-aware via the
--    STATIC predicate `board_deleted_at IS NULL` — the old per-user board-id
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
--    trash-vs-insert lock contention. issue_labels intentionally has BOTH
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
    SELECT i.board_id, b.deleted_at INTO NEW.board_id, NEW.board_deleted_at
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

-- 8. Auto-populate team_id + board_deleted_at on issues from the parent
--    board (REV2-5). team_id makes the issues shape TEAM-scoped (stable
--    across board create/trash); board_deleted_at is the shape's static
--    trash predicate. Writers pass the team_id they already resolved for
--    auth (the column is NOT NULL), but this trigger overwrites with
--    board-derived truth, and re-derives both columns when issues.move
--    re-points board_id (moves never cross teams, so team_id is effectively
--    invariant — deriving it anyway keeps the trigger the single source of
--    truth).
CREATE OR REPLACE FUNCTION populate_issue_board_context()
RETURNS TRIGGER AS $$
BEGIN
  SELECT b.team_id, b.deleted_at INTO NEW.team_id, NEW.board_deleted_at
  FROM boards b
  WHERE b.id = NEW.board_id;
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
