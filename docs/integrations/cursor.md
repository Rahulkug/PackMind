# PackMind + Cursor

Cursor has its own codebase indexing, but PackMind gives you an explicit,
explainable, token-budgeted pack you control — useful when you want exactly the
right context (callers, tests, configs) for a specific task rather than
whatever the IDE retrieves.

## Option A — MCP

Cursor supports MCP servers. Add PackMind in `~/.cursor/mcp.json` (or the
project `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "packmind": {
      "command": "packmind",
      "args": ["--repo", "/absolute/path/to/repo", "mcp"]
    }
  }
}
```

Use an absolute path for `--repo`. Restart Cursor; the PackMind tools then
appear to the agent.

## Option B — paste a pack

Build a pack and drop it into the chat as context:

```sh
packmind --repo . pack "refactor the auth flow" --mode refactor --budget 8000 --copy
```

Then paste (Cmd/Ctrl+V) into the Cursor chat before describing the change. Each
item is labelled with the reason it was included, which helps the model weight
the context.

## Keeping fresh

Re-run `packmind index .` after substantial edits. `packmind doctor` will tell
you if the index has drifted from the working tree.
