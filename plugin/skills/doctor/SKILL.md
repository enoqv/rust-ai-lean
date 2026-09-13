---
name: doctor
description: Check that rust-ai-lean works end to end in this project (rust-analyzer LSP, CLI, hooks, toolchain) and offer each fix one at a time.
disable-model-invocation: true
---

# rust-ai-lean doctor

Verify the whole rust-ai-lean setup for the current project, then offer fixes. Reply to the user in the language they write in.

## Rules

- Never run a fix without first asking the user about that specific fix.
- Report faithfully. A check you could not run is WARN with the reason, never OK.
- Record every result as a row: STATUS (OK, WARN, FAIL, INFO), check id, detail, fix (may be empty).

## Phase 1: static checks

Run:

```sh
sh "${CLAUDE_SKILL_DIR}/scripts/doctor.sh" --plugin-root "${CLAUDE_SKILL_DIR}/../.." --project "$PWD"
```

If `${CLAUDE_SKILL_DIR}` was not replaced with a real path, use the base directory shown when this skill was loaded instead. Each output line is `STATUS<TAB>id<TAB>detail<TAB>fix`; keep all rows.

## Phase 2: live checks

1. **Plugins** (`plugin.official-lsp`, `plugin.enabled`, `info.context7`). Run `claude plugin list --json` and inspect the array:
   - `rust-analyzer-lsp@claude-plugins-official` with `"enabled": true`: FAIL `plugin.official-lsp`, "a second rust-analyzer server is registered for .rs", fix `claude plugin disable rust-analyzer-lsp@claude-plugins-official`.
   - `rust-ai-lean@rust-ai-lean` missing or `"enabled": false`: FAIL `plugin.enabled`, fix `claude plugin enable rust-ai-lean@rust-ai-lean`.
   - Any enabled plugin whose id starts with `context7`: INFO `info.context7`, "for Rust crate APIs use rust-ai-lean crate-src, not Context7".
   - If `claude` cannot be run: WARN `plugin.list` with the error.
2. **LSP tool** (`lsp.tool`). If the LSP tool is not in your tool list, load it with ToolSearch query `select:LSP`. Still unavailable: FAIL, "LSP tool unavailable in this session", fix `start a new Claude Code session`.
3. **documentSymbol** (`lsp.document-symbol`). Skip as INFO unless `project.detected` is OK. Pick a `.rs` file: `src/lib.rs`, else `src/main.rs`, else the first `.rs` path printed by `rust-ai-lean outline .` (or by `find . -name '*.rs' -not -path '*/target/*' | head -n 1` if the CLI is missing). Call LSP `documentSymbol` on it at line 1, character 1.
   - Error saying the server crashed or failed to start: FAIL with the error text; fix is the `toolchain.ra` fix if that row is FAIL, otherwise empty.
   - Empty result: rust-analyzer may still be indexing. Retry up to 3 times, running `sleep 20` between attempts when allowed. Still empty: WARN "no symbols after 3 attempts".
   - Symbols returned: OK with the symbol count.
4. **hover** (`lsp.hover`). Only if step 3 is OK: call LSP `hover` on the name of a function from step 3. OK when a signature is returned, otherwise WARN with the response.
5. **rust-analyzer processes** (`lsp.processes`). Run `ps -eo rss,args | grep '[r]ust-analyzer'`. INFO with the process count and total RSS in MB.
6. **Session hint** (`hook.context`). Skip as INFO unless `project.detected` is OK. OK if this conversation contains context starting with `[rust-ai-lean]`; otherwise WARN "SessionStart hint not seen", fix `start a new Claude Code session`.

## Phase 3: summary and fixes

Show one table with columns Status, Check, Detail, ordered FAIL, WARN, OK, INFO.

Then, for each FAIL or WARN row with a non-empty fix, in table order, ask the user whether to apply exactly that fix (use AskUserQuestion when available, one question per fix). Run each approved fix and show its result. A fix of `start a new Claude Code session` is an instruction for the user, not a command.

## Phase 4: re-check

Run Phase 1 again and report what changed. If any applied fix touched plugins, rustup components, or the CLI, tell the user to start a new Claude Code session and run `/rust-ai-lean:doctor` again: LSP servers and hooks load when a session starts.
