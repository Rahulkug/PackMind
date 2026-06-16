# PackMind + Cline

Cline (the VS Code agent) supports MCP servers, so PackMind attaches as a
read-only context provider.

## Setup

Open Cline's MCP settings (`Cline: Open MCP Settings`, which edits
`cline_mcp_settings.json`) and add:

```json
{
  "mcpServers": {
    "packmind": {
      "command": "packmind",
      "args": ["--repo", "/absolute/path/to/repo", "mcp"],
      "disabled": false
    }
  }
}
```

Use an absolute path for `--repo`. Reload the window; Cline lists the PackMind
tools and can call them automatically.

## Use it

Ask repo-scoped questions normally, or steer Cline:

> Call packmind build_context_pack (mode "bugfix") before editing, then fix the
> failing rounding logic.

Because the tools are read-only, they are safe to auto-approve.

## No-MCP fallback

```sh
packmind --repo . pack "your task" --mode bugfix --budget 8000 --copy
```

Paste into the Cline input.

## Freshness

Re-index after large changes with `packmind index .`; use the `changed_since`
tool or `packmind doctor` to see drift.
