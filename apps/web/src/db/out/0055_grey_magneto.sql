-- EXP-361: originally stamped 1785350393499 in the journal, which sorts BELOW
-- 0054's hand-picked future stamp (1785650000000) — drizzle only applies
-- migrations newer than the DB's max created_at, so every DB that had already
-- applied 0054 skipped this one and never gained the 'merged' enum value
-- ("Merge and close" then failed with invalid input value for enum
-- coding_session_status). Re-stamped to 1785650000001 so those DBs pick it
-- up; IF NOT EXISTS keeps the re-run a no-op on DBs that did apply it.
ALTER TYPE "public"."coding_session_status" ADD VALUE IF NOT EXISTS 'merged' BEFORE 'ended';
