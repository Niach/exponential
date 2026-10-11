-- SLOP-17: the column names what it holds, the id of the action trigger that
-- fired the run (the automations table is gone since 0164).
ALTER TABLE "coding_sessions" RENAME COLUMN "automation_id" TO "trigger_id";
