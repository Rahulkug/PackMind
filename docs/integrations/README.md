# PackMind Integrations

PackMind is the context layer underneath your coding agent — not a replacement
for it. There are two ways to wire it in:

1. **MCP** (best for agents that speak it): PackMind runs as a read-only MCP
   stdio server exposing `search_code`, `explain_symbol`, `find_callers`,
   `find_tests`, `build_context_pack`, `changed_since`, `impact_analysis`, and
   `get_content`. The agent calls these tools itself.
2. **Paste / pipe** (works with anything): build a pack on the CLI and feed it
   to the tool, either via `--copy` (clipboard) or `--render plain` (stdout).

Per-tool guides:

- [Claude Code](claude-code.md) — MCP, native
- [Cursor](cursor.md) — MCP or pasted context
- [Cline](cline.md) — MCP
- [Continue](continue.md) — MCP or context provider
- [Aider](aider.md) — pipe a plain-rendered pack

Common prerequisites for every guide:

```sh
cargo install --git https://github.com/Rahulkug/PackMind packmind-cli
cd /path/to/your/repo
packmind init .
packmind index .
packmind doctor      # confirms the index is healthy and prints your MCP line
```

`packmind doctor` prints the exact `claude mcp add …` command for your machine,
with absolute paths filled in.
