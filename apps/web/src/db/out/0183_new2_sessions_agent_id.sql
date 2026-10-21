ALTER TABLE "sessions" ADD COLUMN "agent_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_agent_id_agents_id_fk" FOREIGN KEY ("agent_id") REFERENCES "public"."agents"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_sessions_agent_conversation" ON "sessions" USING btree ("agent_id","user_id") WHERE agent_id IS NOT NULL AND parent_id IS NULL;--> statement-breakpoint
CREATE INDEX "idx_sessions_agent" ON "sessions" USING btree ("agent_id");--> statement-breakpoint
-- NEW-2 data: the former agent root is now its owner's conversation session
-- with the agent of the same id (`kind = agent` stays as the read shim the
-- ≤0.14.74 devices key persistence on).
UPDATE "sessions" SET "agent_id" = "id" WHERE "kind" = 'agent' AND "parent_id" IS NULL;