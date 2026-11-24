

    // #EXP-2097 (agent-settings.json `assistant.chatTeams`): the Assistant's
    // chat sits in a personal team that never syncs; its preview cards read
    // the entity's own team, a member's role the chat teams.
    private val stamp = "2026-01-01 00:00:00+00"
    private val assistantChat = EntityRefResolver(
        db = flowOf(null),
        teamId = "p-me",
        userId = "u-me",
        chatTeamIds = listOf("t-a", "t-b"),
    )
    private val teamChat = EntityRefResolver(db = flowOf(null), teamId = "t-b", userId = "u-me")

    private fun board(id: String, teamId: String) = BoardEntity(
        id = id, teamId = teamId, name = id, slug = id, prefix = "EXP", color = "#6366f1",
        sortOrder = 0.0, createdAt = stamp, updatedAt = stamp,
    )

    private fun statusRow(id: String, teamId: String, name: String, sortOrder: Double) = IssueStatusEntity(
        id = id, teamId = teamId, category = IssueStatusCategory.Started.wire, name = name,
        sortOrder = sortOrder, createdAt = stamp, updatedAt = stamp,
    )

    private fun membership(teamId: String, userId: String, role: String) = TeamMemberEntity(
        id = "$teamId-$userId", teamId = teamId, userId = userId, role = role, createdAt = stamp, updatedAt = stamp,
    )

    @Test
    fun `a board, prompt and label card resolve by the entity's own team in the Assistant's chat`() {
        val boards = listOf(board("ba", "t-a"), board("bb", "t-b"))
        val prompts = listOf(PromptEntity(id = "pa", teamId = "t-a", name = "Review"), PromptEntity(id = "pb", teamId = "t-b", name = "Triage"))
        val labels = listOf(
            LabelEntity(id = "la", teamId = "t-a", name = "bug", color = "#ef4444", sortOrder = 0.0, createdAt = stamp, updatedAt = stamp),
            LabelEntity(id = "lb", teamId = "t-b", name = "ux", color = "#22c55e", sortOrder = 0.0, createdAt = stamp, updatedAt = stamp),
        )
        assertEquals("t-b", assistantChat.board(boards, "bb")?.teamId)
        assertEquals("Review", assistantChat.prompt(prompts, "pa")?.name)
        assertEquals("ux", assistantChat.label(labels, "lb")?.name)
        // A team chat naming another synced team's row: that row too.
        assertEquals("t-a", teamChat.board(boards, "ba")?.teamId)
        assertNull(assistantChat.board(boards, "gone"))
    }

    @Test
    fun `a status card resolves among its own team's rows`() {
        val rows = listOf(
            statusRow("sa1", "t-a", "Doing", 1.0),
            statusRow("sb1", "t-b", "Started", 1.0),
            statusRow("sb2", "t-b", "Review", 2.0),
        )
        assertEquals("Review", assistantChat.status(rows, "sb2")?.name)
        assertEquals("Doing", teamChat.status(rows, "sa1")?.name)
        assertNull(assistantChat.status(rows, "gone"))
    }

    @Test
    fun `a member card's role is the first chat team's that has them`() {
        val rows = listOf(membership("t-b", "u-x", "owner"), membership("t-a", "u-x", "member"), membership("t-c", "u-x", "owner"))
        assertEquals("member", assistantChat.memberRole(rows, "u-x"))
        assertEquals("owner", assistantChat.memberRole(rows.filter { it.teamId != "t-a" }, "u-x"))
        // A team outside the chat teams never names the role.
        assertNull(assistantChat.memberRole(rows.filter { it.teamId == "t-c" }, "u-x"))
        // Every team chat: unchanged, its own team's.
        assertEquals("owner", teamChat.memberRole(rows, "u-x"))
    }
}
