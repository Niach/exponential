-- 0073 (EXP-500) mistakenly created an update_updated_at trigger on
-- issue_labels — a table with no updated_at column (deliberately
-- timestamp-less). The trigger is unreachable today (every issue_labels
-- UPDATE changes board_id or a board-hide mirror, which its WHEN clause
-- excludes), but if it ever fired, update_updated_at()'s
-- `NEW.updated_at = now()` would throw and abort the enclosing
-- transaction. The custom trigger SQL never defines it; drop it wherever
-- 0073 already ran.
DROP TRIGGER IF EXISTS update_updated_at ON issue_labels;
