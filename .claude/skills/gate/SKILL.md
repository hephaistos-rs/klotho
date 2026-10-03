---
name: gate
description: Run Klotho's full build gate - topcoat fmt, cargo fmt, clippy, the allow and palette greps, every test, cargo deny and cargo xtask dist - and report one PASS/FAIL line per step.
argument-hint: "[--no-dist]"
disable-model-invocation: true
---

# Build gate

Run the script in the background, because `cargo test` includes `raw_memory` (about 6 minutes):

```
bash .claude/skills/gate/gate.sh $ARGUMENTS
```

Use the Bash tool with `run_in_background: true` and a timeout of at least 20 minutes, and wait for the notification rather than polling.

When it finishes:
- **GATE PASS:** report the step lines as they are.
- **GATE FAIL:** read `target/gate/<step>.log` for each failed step and report the cause. Don't fix anything unless asked, and never pass the gate by adding `allow`s, weakening tests or changing dependencies.

`topcoat-fmt` fails when it had to reformat a file. The files are already fixed by then, so a rerun passes; say which files changed.
