CREATE TABLE "branch_pushes" (
	"repository_id" uuid NOT NULL,
	"branch" varchar(255) NOT NULL,
	"pushed_at" timestamp with time zone NOT NULL,
	"session_id" uuid,
	CONSTRAINT "branch_pushes_repository_id_branch_pk" PRIMARY KEY("repository_id","branch")
);
--> statement-breakpoint
ALTER TABLE "branch_pushes" ADD CONSTRAINT "branch_pushes_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "branch_pushes" ADD CONSTRAINT "branch_pushes_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;