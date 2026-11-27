package com.exponential.app.domain

// #EXP-2172: ci-page.json `scenario`, the CI page every platform renders for
// its shot (sg_ci; no server). Generated from the fixture; `CiPageTest`
// proves it equal to it.
object CiPageSpecimen {
    val INPUT: CiPage.Input = CiPage.Input(
        repos = listOf(CiPage.Repo("repo-engine", "acme/engine"), CiPage.Repo("repo-mobile", "acme/mobile"), CiPage.Repo("repo-site", "acme/site")),
        sessions = listOf(CiPage.Session("s-mine", "u-me"), CiPage.Session("s-other", "u-other")),
        pullRequests = listOf(
            CiPage.Pr("pr-211", 211, "Fix bisect on a red batch", "repo-engine", "s-other", "main", "testing", null, "train-91"),
            CiPage.Pr("pr-212", 212, "Runner labels", "repo-engine", "s-other", "main", "testing", null, "train-91"),
            CiPage.Pr("pr-213", 213, "Log tail", "repo-engine", "s-mine", "main", "testing", null, "train-91"),
            CiPage.Pr("pr-214", 214, "Reduce cold start below 400 ms", "repo-engine", "s-mine", "main", "checks", null, null),
            CiPage.Pr("pr-215", 215, "Cache the planner", "repo-engine", "s-other", "main", "queued", 1, null),
            CiPage.Pr("pr-40", 40, "Dark splash", "repo-mobile", null, "main", "held", null, null),
            CiPage.Pr("pr-41", 41, "Old", "repo-mobile", null, "main", "merged", null, null),
        ),
        runs = listOf(
            CiPage.Run("r483", "repo-engine", 483, "gate", "r483aaaaaaaaaaaaaaaa", "exp/APP-32", "pull_request", "pending", "pr-214", null, "exponential", "2026-11-27T10:26:48.000Z", null, "2026-11-27T10:26:48.000Z"),
            CiPage.Run("r91", "repo-engine", 91, "Merge train", "r91aaaaaaaaaaaaaaaa", "main", "merge_train", "pending", null, null, "exponential", "2026-11-27T10:28:20.000Z", null, "2026-11-27T10:28:20.000Z"),
            CiPage.Run("g1207", "repo-mobile", 1207, "build-ios", "g1207aaaaaaaaaaaaaaaa", "release/2.4", "tag", "pending", null, "u-me", "github", null, null, "2026-11-27T10:29:00.000Z"),
            CiPage.Run("r482", "repo-engine", 482, "gate", "r482aaaaaaaaaaaaaaaa", "exp/APP-33", "pull_request", "failure", "pr-213", null, "exponential", "2026-11-27T09:59:20.000Z", "2026-11-27T10:12:00.000Z", "2026-11-27T09:59:20.000Z"),
            CiPage.Run("g1206", "repo-mobile", 1206, "test", "g1206aaaaaaaaaaaaaaaa", "main", "push", "success", null, null, "github", "2026-11-27T09:22:54.000Z", "2026-11-27T09:30:00.000Z", "2026-11-27T09:22:54.000Z"),
            CiPage.Run("g57", "repo-site", 57, "deploy", "g57aaaaaaaaaaaaaaaa", "main", "push", "failure", null, "u-me", "github", "2026-11-26T08:27:57.000Z", "2026-11-26T08:30:00.000Z", "2026-11-26T08:27:57.000Z"),
            CiPage.Run("g58", "repo-site", 58, "deploy", "g58aaaaaaaaaaaaaaaa", "main", "push", "success", null, "u-me", "github", "2026-11-27T08:27:57.000Z", "2026-11-27T08:30:00.000Z", "2026-11-27T08:27:57.000Z"),
            CiPage.Run("r481", "repo-engine", 481, "gate", "r481aaaaaaaaaaaaaaaa", "exp/APP-32", "pull_request", "failure", "pr-214", null, "exponential", "2026-11-27T07:30:00.000Z", "2026-11-27T07:30:24.000Z", "2026-11-27T07:30:00.000Z"),
            CiPage.Run("r480", "repo-gone", 480, "nightly", "r480aaaaaaaaaaaaaaaa", null, "schedule", "success", null, null, "exponential", "2026-11-27T01:30:00.000Z", "2026-11-27T01:44:12.000Z", "2026-11-27T01:30:00.000Z"),
        ),
        jobs = listOf(
            CiPage.Job("r483", "in_progress", null, listOf("linux"), "dev-mint", "exponential"),
            CiPage.Job("r483", "queued", null, listOf("linux"), null, "exponential"),
            CiPage.Job("r483", "queued", null, listOf("mac"), null, "exponential"),
            CiPage.Job("r91", "in_progress", null, listOf("linux"), "dev-mint", "exponential"),
            CiPage.Job("g1207", "queued", null, listOf("macos-latest"), null, "github"),
            CiPage.Job("r482", "completed", "success", emptyList(), null, "exponential"),
            CiPage.Job("r482", "completed", "failure", emptyList(), null, "exponential"),
            CiPage.Job("g1206", "completed", "success", emptyList(), null, "github"),
            CiPage.Job("g57", "completed", "failure", emptyList(), null, "github"),
            CiPage.Job("g58", "completed", "success", emptyList(), null, "github"),
            CiPage.Job("r481", "completed", "cancelled", emptyList(), null, "exponential"),
            CiPage.Job("r480", "completed", "success", emptyList(), null, "exponential"),
        ),
        runners = listOf(
            CiPage.Runner("dev-mint", "mint", "linux", listOf("GPU"), 2, "2026-11-27T10:29:40.000Z"),
            CiPage.Runner("dev-mac", "Alex's MacBook Pro", "macos", emptyList(), 1, "2026-11-27T10:29:55.000Z"),
            CiPage.Runner("dev-old", "build-box", null, emptyList(), 4, "2026-11-26T22:00:00.000Z"),
        ),
        viewerId = "u-me",
        now = "2026-11-27T10:30:00.000Z",
    )
}
