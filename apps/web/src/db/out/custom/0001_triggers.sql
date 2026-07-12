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

CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspaces FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON projects FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issues FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON labels FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON fcm_tokens FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();
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
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON releases FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- 2. Auto-generate issue number and identifier per project, allocated from the
--    per-project monotonic counter table issue_number_counters (migration
--    0013). The ON CONFLICT row lock serializes concurrent same-project
--    inserts — the old unlocked SELECT MAX(number)+1 raced under READ
--    COMMITTED and let two inserts commit the same identifier. The counter
--    only ever grows, so deleting the top-numbered issue can never recycle its
--    identifier (old #PREFIX-n mentions and exp/PREFIX-n branches stay
--    unambiguous). The GREATEST clamp self-heals a missing/stale counter row
--    (fresh project, or rows inserted by the pre-counter trigger between
--    `migrate` running and this file being re-applied at boot). The unique
--    index uniq_issues_project_number (migration 0013) is the loud backstop:
--    any residual race fails the insert instead of committing a duplicate.
--    Aborted inserts roll the counter back transactionally — a never-committed
--    number being reused is fine.
CREATE OR REPLACE FUNCTION generate_issue_number()
RETURNS TRIGGER AS $$
DECLARE
  next_number integer;
  current_max integer;
  project_prefix text;
BEGIN
  SELECT COALESCE(MAX(number), 0) INTO current_max
  FROM issues
  WHERE project_id = NEW.project_id;

  INSERT INTO issue_number_counters AS c (project_id, counter)
  VALUES (NEW.project_id, current_max + 1)
  ON CONFLICT (project_id) DO UPDATE
    SET counter = GREATEST(c.counter, current_max) + 1
  RETURNING counter INTO next_number;

  SELECT prefix INTO project_prefix
  FROM projects
  WHERE id = NEW.project_id;

  NEW.number := next_number;
  NEW.identifier := project_prefix || '-' || next_number;
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

-- 4. Auto-populate workspace_id on issue_labels from the referenced label,
--    so the Electric shape filter on issue_labels can be workspace-scoped
--    (stable) instead of label-scoped (rewritten on every label add → 409
--    churn → cascading 502s upstream).
CREATE OR REPLACE FUNCTION populate_issue_label_workspace_id()
RETURNS TRIGGER AS $$
BEGIN
  SELECT workspace_id INTO NEW.workspace_id FROM labels WHERE id = NEW.label_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_label_workspace_id
  BEFORE INSERT ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_label_workspace_id();

-- 5. Auto-populate workspace_id on issue-child tables from the referenced
--    issue's project, so their Electric shape filters can be workspace-scoped
--    (stable). issues have no direct workspace_id, so resolve it via
--    issues → projects (NOT the issue_labels template which reads
--    labels.workspace_id). A wrong source leaves workspace_id NULL → NOT NULL
--    violation. Guarded on issue_id: release-scoped coding_sessions rows
--    (EXP-56, issue_id NULL) carry an explicitly-written workspace_id that an
--    unguarded SELECT-with-no-row would overwrite with NULL. Every other
--    consumer has issue_id NOT NULL, so the guard is a no-op for them.
CREATE OR REPLACE FUNCTION populate_issue_child_workspace_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT p.workspace_id INTO NEW.workspace_id
    FROM issues i JOIN projects p ON p.id = i.project_id
    WHERE i.id = NEW.issue_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_issue_subscriber_workspace_id
  BEFORE INSERT ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_workspace_id();

CREATE OR REPLACE TRIGGER populate_issue_event_workspace_id
  BEFORE INSERT ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_workspace_id();

-- 6. coding_sessions (the live "coding now" record, the 14th synced shape):
--    workspace_id denormalized from issue→project via the shared
--    populate_issue_child_workspace_id, so its Electric shape filter stays
--    workspace-scoped and stable.
CREATE OR REPLACE TRIGGER populate_coding_session_workspace_id
  BEFORE INSERT ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_workspace_id();

-- 7. Auto-populate project_id on every issue-child synced table from the
--    referenced issue, so anonymous feedback-board shape filters can be
--    PROJECT-scoped (Electric where clauses are single-table; a public project
--    lives inside an otherwise-private workspace, so workspace scoping would
--    leak sibling projects). Covers every writer (tRPC, widget service,
--    attachment storage, MCP) without touching each insert site; overwrites
--    any explicitly-passed value with issue-derived truth, mirroring the
--    workspace_id pattern. Issues CAN move between projects (EXP-57,
--    issues.move): the move re-points these denormalized columns in its own
--    transaction, and the FOR KEY SHARE read below closes the race with a
--    concurrent child insert — it blocks against the move's FOR UPDATE row
--    lock, so the trigger always reads the committed post-move project_id
--    (or commits first, where the move's re-point UPDATEs then heal it).
--    issue_labels intentionally has BOTH triggers (workspace_id from the
--    label, project_id from the issue). Guarded on issue_id like the
--    workspace_id populate: release-scoped coding_sessions rows (issue_id
--    NULL) keep project_id NULL — they span projects and are never
--    anonymous-visible.
CREATE OR REPLACE FUNCTION populate_issue_child_project_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT project_id INTO NEW.project_id FROM issues WHERE id = NEW.issue_id
      FOR KEY SHARE;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER populate_comment_project_id
  BEFORE INSERT ON comments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_project_id();

CREATE OR REPLACE TRIGGER populate_attachment_project_id
  BEFORE INSERT ON attachments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_project_id();

CREATE OR REPLACE TRIGGER populate_issue_event_project_id
  BEFORE INSERT ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_project_id();

CREATE OR REPLACE TRIGGER populate_issue_subscriber_project_id
  BEFORE INSERT ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_project_id();

CREATE OR REPLACE TRIGGER populate_coding_session_project_id
  BEFORE INSERT ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_project_id();

CREATE OR REPLACE TRIGGER populate_issue_label_project_id
  BEFORE INSERT ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_project_id();
