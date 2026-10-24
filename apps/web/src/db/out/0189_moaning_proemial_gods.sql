ALTER TABLE "sessions" ADD COLUMN IF NOT EXISTS "started_by_agent_id" uuid;--> statement-breakpoint
DO $$
BEGIN
  -- #EXP-1383: idempotent on purpose (see 0188): renumbered later, it must
  -- be a no-op on a database that already ran it.
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'sessions_started_by_agent_id_agents_id_fk') THEN
    ALTER TABLE "sessions" ADD CONSTRAINT "sessions_started_by_agent_id_agents_id_fk" FOREIGN KEY ("started_by_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;
  END IF;
END $$;
