ALTER TABLE "coding_sessions" ADD COLUMN "summary" text;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "outcome" varchar(16);--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "ended_by" varchar(16);--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "resumed_from_id" uuid;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "merged_own_pr" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_resumed_from_id_coding_sessions_id_fk" FOREIGN KEY ("resumed_from_id") REFERENCES "public"."coding_sessions"("id") ON DELETE set null ON UPDATE no action;