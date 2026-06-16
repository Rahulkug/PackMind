# PackMind + Claude Code

Claude Code speaks MCP natively, so PackMind plugs in as a read-only tool
server. The agent decides when to call `build_context_pack` instead of reading
files one by one.

## Setup

From your repository root, after `packmind init . && packmind index .`:

```sh
claude mcp add packmind -- packmind --repo . mcp
```

(`packmind doctor` prints this line with absolute paths already filled in.)

Verify it registered:

```sh
claude mcp list
```

## Use it

Just ask normally. Claude Code will call the PackMind tools when a question is
repo-scoped. To nudge it explicitly:

> Use the packmind build_context_pack tool with mode "bugfix" to gather context
> before fixing the currency rounding bug, then make the change.

The pack it gets back is token-budgeted and carries a `why` for every item, so
the agent spends its turns editing instead of rediscovering structure.

## Keep the index fresh

The MCP server reports staleness, but it does not write the index. Re-index
after big changes:

```sh
packmind index .
```

Or check what changed without indexing via the `changed_since` MCP tool.

## No-MCP fallback

If you prefer to paste context yourself:

```sh
packmind pack "your task" --mode bugfix --budget 8000 --copy
```

Then paste into the Claude Code prompt.
