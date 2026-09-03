import XCTest

@testable import ExpCore

// EXP-724 — the steer slash-command catalog and its two matching rules.
// Mirrors the web `steer-commands.test.ts`, Android's `SlashCommandsTest` and
// desktop's `slash_commands.rs` tests: same catalog lock, same query rules,
// same insertion and send rules, same confirm copy.
final class SlashCommandsTests: XCTestCase {

    // MARK: - Catalog lock

    func testCatalogZipsEveryContractRow() {
        XCTAssertEqual(SlashCommands.all.count, DomainContract.steerCommandNames.count)
        XCTAssertEqual(SlashCommands.all.map(\.name), DomainContract.steerCommandNames)
        XCTAssertEqual(
            SlashCommands.all.map(\.description), DomainContract.steerCommandDescriptions
        )
        XCTAssertEqual(SlashCommands.all.map(\.argHint), DomainContract.steerCommandArgHints)
        XCTAssertEqual(SlashCommands.all.map(\.confirm), DomainContract.steerCommandConfirm)
    }

    func testEveryCommandRunsOnKnownAgentsOnly() {
        let known = Set(DomainContract.codingAgentValues)
        for command in SlashCommands.all {
            XCTAssertFalse(command.agents.isEmpty, "\(command.name) runs nowhere")
            for agent in command.agents {
                XCTAssertTrue(known.contains(agent), "\(command.name) names unknown agent \(agent)")
            }
        }
    }

    func testNamesAreUniqueAndTokenShaped() {
        let names = SlashCommands.all.map(\.name)
        XCTAssertEqual(Set(names).count, names.count)
        for name in names {
            XCTAssertFalse(name.isEmpty)
            XCTAssertFalse(name.contains(where: { $0.isWhitespace }), "\(name) carries whitespace")
            XCTAssertFalse(name.hasPrefix("/"), "\(name) carries its own slash")
        }
    }

    func testDefaultAgentIsTheFirstContractAgent() {
        XCTAssertEqual(SlashCommands.defaultAgent, DomainContract.codingAgentValues[0])
        XCTAssertEqual(SlashCommands.defaultAgent, "claude")
    }

    // MARK: - Per-agent catalog

    func testCatalogIsFilteredByAgentInContractOrder() {
        // The same two rows for every agent — the desktop maps `/clear` per
        // agent (pi has no `/clear`; it runs `ctx.newSession()`).
        for agent in ["claude", "codex", "pi"] {
            XCTAssertEqual(SlashCommands.catalog(for: agent).map(\.name), ["compact", "clear"])
        }
    }

    func testAnAbsentAgentFallsBackToClaude() {
        XCTAssertEqual(
            SlashCommands.catalog(for: nil).map(\.name),
            SlashCommands.catalog(for: "claude").map(\.name)
        )
        XCTAssertEqual(
            SlashCommands.catalog(for: "").map(\.name),
            SlashCommands.catalog(for: "claude").map(\.name)
        )
    }

    func testAnUnknownAgentRunsNothing() {
        XCTAssertTrue(SlashCommands.catalog(for: "gemini").isEmpty)
    }

    // MARK: - Menu query rule

    func testABareSlashListsEverythingTheAgentCanRun() {
        XCTAssertEqual(
            SlashCommands.matches(draft: "/", agent: "claude").map(\.name),
            ["compact", "clear"]
        )
    }

    func testPrefixFilteringIsCaseInsensitive() {
        XCTAssertEqual(
            SlashCommands.matches(draft: "/co", agent: "claude").map(\.name), ["compact"]
        )
        XCTAssertEqual(
            SlashCommands.matches(draft: "/CO", agent: "claude").map(\.name), ["compact"]
        )
        XCTAssertEqual(
            SlashCommands.matches(draft: "/c", agent: "claude").map(\.name), ["compact", "clear"]
        )
    }

    func testTheMenuClosesOnceAnArgumentStarts() {
        XCTAssertTrue(SlashCommands.matches(draft: "/compact ", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "/compact now", agent: "claude").isEmpty)
    }

    func testTheMenuNeedsTheSlashAtPositionZero() {
        XCTAssertTrue(SlashCommands.matches(draft: "x /c", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: " /c", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "", agent: "claude").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "hello", agent: "claude").isEmpty)
    }

    func testANameThatMatchesNothingClosesTheMenu() {
        XCTAssertTrue(SlashCommands.matches(draft: "/zzz", agent: "claude").isEmpty)
        // Non-name characters are not a partial name at all.
        XCTAssertTrue(SlashCommands.matches(draft: "/co/", agent: "claude").isEmpty)
    }

    func testTheMenuIsAgentScoped() {
        // Names outside the catalog never show, whatever the CLI itself ships.
        XCTAssertEqual(
            SlashCommands.matches(draft: "/cl", agent: "claude").map(\.name), ["clear"]
        )
        XCTAssertEqual(SlashCommands.matches(draft: "/cl", agent: "codex").map(\.name), ["clear"])
        XCTAssertTrue(SlashCommands.matches(draft: "/n", agent: "codex").isEmpty)
        XCTAssertTrue(SlashCommands.matches(draft: "/mo", agent: "claude").isEmpty)
        // A session with no agent is claude's catalog.
        XCTAssertEqual(SlashCommands.matches(draft: "/cl", agent: nil).map(\.name), ["clear"])
    }

    // MARK: - Insertion

    func testInsertionLeavesRoomForAnArgumentOnlyWhenThereIsOne() {
        let compact = SlashCommands.all.first { $0.name == "compact" }
        XCTAssertEqual(compact?.argHint, "instructions")
        XCTAssertEqual(compact?.insertion, "/compact ")
        let clear = SlashCommands.all.first { $0.name == "clear" }
        XCTAssertEqual(clear?.argHint, "")
        XCTAssertEqual(clear?.insertion, "/clear")
    }

    // MARK: - Send rule

    func testAMessageIsACommandOnlyWhenItsFirstTokenMatchesExactly() {
        XCTAssertEqual(SlashCommands.command(for: "/compact", agent: "claude")?.name, "compact")
        XCTAssertEqual(
            SlashCommands.command(for: "/compact keep the plan", agent: "claude")?.name, "compact"
        )
        XCTAssertEqual(SlashCommands.command(for: "/COMPACT", agent: "claude")?.name, "compact")
        XCTAssertNil(SlashCommands.command(for: "/compactify", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "please /compact", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "//compact", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "compact", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "   ", agent: "claude"))
    }

    func testTheSendRuleIsAgentScopedToo() {
        XCTAssertEqual(SlashCommands.command(for: "/clear", agent: "claude")?.name, "clear")
        XCTAssertEqual(SlashCommands.command(for: "/clear", agent: "codex")?.name, "clear")
        XCTAssertEqual(SlashCommands.command(for: "/clear", agent: "pi")?.name, "clear")
        // Outside the catalog, whatever the CLI itself ships.
        XCTAssertNil(SlashCommands.command(for: "/new", agent: "codex"))
        XCTAssertNil(SlashCommands.command(for: "/model opus", agent: "claude"))
        XCTAssertNil(SlashCommands.command(for: "/init", agent: "codex"))
        XCTAssertNil(SlashCommands.command(for: "/review", agent: "claude"))
    }

    // MARK: - Confirm

    func testOnlyTheConversationDiscardingCommandsConfirm() {
        let confirming = SlashCommands.all.filter(\.confirm).map(\.name)
        XCTAssertEqual(confirming, ["clear"])
        XCTAssertEqual(SlashCommands.command(for: "/clear", agent: "claude")?.confirm, true)
        XCTAssertEqual(SlashCommands.command(for: "/clear", agent: "codex")?.confirm, true)
        XCTAssertEqual(SlashCommands.command(for: "/compact", agent: "claude")?.confirm, false)
    }

    /// Byte-identical ×4 (EXP-724).
    func testConfirmCopyIsTheLockedLiterals() {
        guard let clear = SlashCommands.all.first(where: { $0.name == "clear" }) else {
            return XCTFail("no /clear row")
        }
        XCTAssertEqual(SlashCommands.confirmTitle(clear), "Run /clear?")
        XCTAssertEqual(SlashCommands.confirmButton(clear), "Run /clear")
        XCTAssertEqual(
            SlashCommands.confirmBody,
            "The agent forgets everything in this session so far. Files in the worktree are kept."
        )
    }
}
