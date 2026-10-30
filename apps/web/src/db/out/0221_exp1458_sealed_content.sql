CREATE TABLE "session_keys" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"root_id" uuid NOT NULL,
	"epoch" integer NOT NULL,
	"install_id" uuid NOT NULL,
	"wrapped" text NOT NULL,
	"created_by_install_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "seal_public_key" varchar(43);--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "seal_epoch" integer;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "sealed_title" text;--> statement-breakpoint
ALTER TABLE "session_keys" ADD CONSTRAINT "session_keys_root_id_sessions_id_fk" FOREIGN KEY ("root_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_keys" ADD CONSTRAINT "session_keys_install_id_installs_id_fk" FOREIGN KEY ("install_id") REFERENCES "public"."installs"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_keys" ADD CONSTRAINT "session_keys_created_by_install_id_installs_id_fk" FOREIGN KEY ("created_by_install_id") REFERENCES "public"."installs"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_session_keys_root_epoch_install" ON "session_keys" USING btree ("root_id","epoch","install_id");--> statement-breakpoint
CREATE INDEX "idx_session_keys_install" ON "session_keys" USING btree ("install_id");