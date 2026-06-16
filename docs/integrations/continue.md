# PackMind + Continue

Continue (the open-source VS Code / JetBrains assistant) can consume PackMind
over MCP, or you can paste a rendered pack into the chat.

## Option A — MCP

Continue supports MCP servers. In your Continue config (`~/.continue/config.yaml`
or the JSON equivalent), add an MCP server:

```yaml
mcpServers:
  - name: packmind
    command: packmind
    args:
      - "--repo"
      - "/absolute/path/to/repo"
      - "mcp"
```

Use an absolute path for `--repo`. Reload Continue; the PackMind tools become
available to the model.

## Option B — paste context

```sh
packmind --repo . pack "add a retry to the http client" --mode refactor --budget 8000 --copy
```

Paste into the Continue chat before describing the task. Every item shows why it
was selected (`anchor`, `calls`, `tested_by`, …).

## Freshness

Re-run `packmind index .` after substantial edits; `packmind doctor` reports
whether the index matches the working tree.
