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
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON push_subscriptions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON fcm_tokens FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_members FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON workspace_invites FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_subscribers FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_events FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON coding_sessions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON repositories FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON project_repositories FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON run_configs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON user_notification_prefs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON email_deliveries FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON widget_configs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON widget_submissions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON github_installations FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- 2. Auto-generate issue number and identifier per project
CREATE OR REPLACE FUNCTION generate_issue_number()
RETURNS TRIGGER AS $$
DECLARE
  next_number integer;
  project_prefix text;
BEGIN
  SELECT COALESCE(MAX(number), 0) + 1 INTO next_number
  FROM issues
  WHERE project_id = NEW.project_id;

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
--    violation.
CREATE OR REPLACE FUNCTION populate_issue_child_workspace_id()
RETURNS TRIGGER AS $$
BEGIN
  SELECT p.workspace_id INTO NEW.workspace_id
  FROM issues i JOIN projects p ON p.id = i.project_id
  WHERE i.id = NEW.issue_id;
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
