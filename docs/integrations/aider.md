# PackMind + Aider

Aider does not use MCP, so the integration is a pipe: build a plain-rendered
pack and hand it to Aider as a message or read-only context.

## Pipe a pack as the message

```sh
packmind --repo . pack "fix the currency rounding bug" --mode bugfix \
  --budget 8000 --render plain > /tmp/ctx.md

aider --message-file /tmp/ctx.md
```

`--render plain` emits the pack as labelled `<pm:ctx …>` blocks (stable,
cache-friendly ordering) with a one-line savings header.

## Or add the changed files directly

`pr-context` tells you exactly which files matter for the current change, so you
can add just those to Aider's editable set:

```sh
packmind --repo . pr-context --json | jq -r '.changed_files[]'
# feed the list to: aider <files…>
```

## Clipboard variant

If you would rather paste:

```sh
packmind --repo . pack "your task" --mode refactor --budget 8000 --copy
```

Then paste into the Aider chat.

## Freshness

Re-run `packmind index .` after edits. `packmind doctor` reports drift between
the index and the working tree.
