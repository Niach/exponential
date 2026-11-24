CREATE TABLE "install_approvals" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"kind" varchar(16) NOT NULL,
	"subject_install_id" uuid NOT NULL,
	"subject_endpoint_id" varchar(64) NOT NULL,
	"approver_install_id" uuid,
	"approver_endpoint_id" varchar(64) NOT NULL,
	"signature" varchar(86) NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "installs" ADD COLUMN "approval_requested_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "install_approvals" ADD CONSTRAINT "install_approvals_subject_install_id_installs_id_fk" FOREIGN KEY ("subject_install_id") REFERENCES "public"."installs"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "install_approvals" ADD CONSTRAINT "install_approvals_approver_install_id_installs_id_fk" FOREIGN KEY ("approver_install_id") REFERENCES "public"."installs"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_install_approvals_subject_approver_kind" ON "install_approvals" USING btree ("subject_install_id","approver_endpoint_id","kind");--> statement-breakpoint
CREATE INDEX "idx_install_approvals_approver" ON "install_approvals" USING btree ("approver_endpoint_id");