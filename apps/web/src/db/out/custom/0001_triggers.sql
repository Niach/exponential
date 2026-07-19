-- Custom triggers for Exponential
-- Apply after migrations: docker exec -i exponential-postgres-1 psql -U postgres -d exponential < src/db/out/custom/0001_triggers.sql

-- 1. Auto-update updated_at timestamp on all tables that carry it.
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON teams FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON boards FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issues FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON labels FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON fcm_tokens FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON team_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON team_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_subscribers FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_events FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON coding_sessions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
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
--    "recently active" ordering honest on every client).
CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()
RETURNS TRIGGER AS $$
DECLARE
  target_issue uuid;
BEGIN
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

-- 7. Auto-populate board_id on every issue-child synced table from the
--    referenced issue, so the member shapes can be BOARD-scoped: a trashed
--    board's children drop out of member sync for the 48h trash window along
--    with the board itself (Electric where clauses are single-table).
--    Covers every writer (tRPC, widget service, attachment storage, MCP)
--    without touching each insert site; overwrites any explicitly-passed
--    value with issue-derived truth, mirroring the team_id pattern. Issues
--    CAN move between boards (EXP-57, issues.move): the move re-points these
--    denormalized columns in its own transaction, and the FOR KEY SHARE read
--    below closes the race with a concurrent child insert — it blocks
--    against the move's FOR UPDATE row lock, so the trigger always reads the
--    committed post-move board_id (or commits first, where the move's
--    re-point UPDATEs then heal it). issue_labels intentionally has BOTH
--    triggers (team_id from the label, board_id from the issue). Guarded on
--    issue_id like the team_id populate: batch-scoped coding_sessions rows
--    (issue_id NULL) keep board_id NULL — they span boards. notifications
--    also carries this trigger (REV-109): its board_id lets the member shape
--    hide notifications of trashed boards for the 48h trash window
--    (issue-less rows — e.g. helpdesk support_reply — keep board_id NULL).
CREATE OR REPLACE FUNCTION populate_issue_child_board_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT board_id INTO NEW.board_id FROM issues WHERE id = NEW.issue_id
      FOR KEY SHARE;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_comment_board_id
  BEFORE INSERT ON comments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_attachment_board_id
  BEFORE INSERT ON attachments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_issue_event_board_id
  BEFORE INSERT ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_issue_subscriber_board_id
  BEFORE INSERT ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_coding_session_board_id
  BEFORE INSERT ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_issue_label_board_id
  BEFORE INSERT ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();

CREATE OR REPLACE TRIGGER populate_notification_board_id
  BEFORE INSERT ON notifications
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();
