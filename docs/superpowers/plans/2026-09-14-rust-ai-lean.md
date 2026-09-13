# rust-ai-lean Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the rust-ai-lean Claude Code plugin and companion CLI from the spec, starting with spikes that validate the spec's unverified platform assumptions.

**Architecture:** One repository serves as a marketplace (`.claude-plugin/marketplace.json`), a plugin (`plugin/`), and a Cargo package (repository root). POSIX sh handles everything that must work before the CLI exists: hooks, the LSP launcher, and doctor's static checks. The Rust CLI handles parsing work in its `outline`, `crate-src`, and `diag` subcommands. Two skills steer the agent and run doctor.

**Tech Stack:** Rust edition 2024 (clap 4, syn 2 + prettyplease 0.2, proc-macro2 with `span-locations`, quote, serde/serde_json, anyhow; tempfile for tests), POSIX sh (dash-compatible), Claude Code plugins (lspServers, hooks, skills), GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-13-rust-ai-lean-design.md`

## Global Constraints

- **Platforms:** Linux and macOS only.
- **Shell scripts:** POSIX sh only. They must pass `shellcheck -x -P SCRIPTDIR -s sh` and run under `dash`. No `jq`, `python3`, or bash-only syntax in shipped scripts.
- **One version everywhere:** `0.1.0` in `Cargo.toml`, `plugin/.claude-plugin/plugin.json`, and the marketplace entry, enforced by `tests/plugin.rs`.
- **Crates:** `syn` major 2 with `prettyplease` 0.2. prettyplease 0.3 requires syn 3; never mix the two. `proc-macro2` must enable `span-locations`, otherwise every line number is wrong.
- **Hint budget:** the session hint (`additionalContext` value) is at most 700 bytes in every state. The hook never exits non-zero.
- **Output safety:** `crate-src` never modifies `Cargo.lock`; it always passes `--locked`. `diag` never drops output it does not recognize.
- **Doctor:** never applies a fix without per-item user approval.
- **Public repository:** no machine-specific paths, user names, internal hostnames, email addresses, or material from private projects in any file or commit message. Use `mktemp -d` for scratch space.
- **Git:** commit messages in English, imperative mood, with no AI attribution trailers. Never push, tag, or open a PR without asking the user.
- **License:** `MIT OR Apache-2.0`.
- **Test builds:** always isolate cargo target directories per test, as `tests/common/mod.rs` does. Two checkouts of this crate sharing one target directory overwrite each other's `rust-ai-lean` binary and make tests run a stale build.

## File map

| Path | Responsibility | Task |
|---|---|---|
| `Cargo.toml`, `Cargo.lock`, `.gitignore`, `LICENSE-MIT`, `LICENSE-APACHE` | Crate manifest, locked deps (needed by `cargo install --locked`), licenses | 6 |
| `.claude-plugin/marketplace.json` | Marketplace listing the plugin at `./plugin` | 6 |
| `plugin/.claude-plugin/plugin.json` | Plugin manifest; `lspServers` added in Task 11 | 6, 11 |
| `src/main.rs` | clap entry point; grows one subcommand per task | 6, 7, 8, 9 |
| `src/outline.rs` | `outline` subcommand | 7 |
| `src/crate_src.rs` | `crate-src` subcommand | 8 |
| `src/diag.rs` | `diag` subcommand | 9 |
| `tests/common/mod.rs` | Test helpers: fixture paths, running the CLI with an isolated target dir | 7 |
| `tests/outline.rs`, `tests/crate_src.rs`, `tests/diag.rs` | CLI integration tests | 7, 8, 9 |
| `tests/plugin.rs` | Version consistency; LSP launcher behavior | 6, 11 |
| `tests/fixtures/outline/{sample,broken}.rs` | outline fixtures | 7 |
| `tests/fixtures/deps/**` | crate-src fixture: one registry dep, one path dep | 8 |
| `tests/fixtures/diag-noisy/**`, `tests/fixtures/diag-buildrs/**` | diag fixtures | 9 |
| `plugin/scripts/lib.sh` | Shared sh helpers: rust-analyzer resolution, Rust project detection | 10 |
| `plugin/hooks/hooks.json`, `plugin/hooks/session-start.sh` | SessionStart/SubagentStart hook | 10 |
| `tests/shell/{lib.sh,run.sh,session-start.test.sh}` | Shell test harness and hook tests | 10 |
| `plugin/skills/doctor/scripts/doctor.sh`, `tests/shell/doctor.test.sh` | Doctor static checks and tests | 12 |
| `plugin/skills/doctor/SKILL.md` | Doctor skill | 13 |
| `plugin/skills/rust-lean/SKILL.md` | Usage skill | 14 |
| `scripts/bench.sh`, `README.md` | Reproducible measurements; user docs | 15 |
| `.github/workflows/ci.yml` | CI on Linux and macOS | 16 |

---

### Task 0: Preconditions and branch

**Files:** none.

- [ ] **Step 1: Create the implementation branch from the spec branch**

```sh
git switch docs/design-spec
git switch -c feat/v0.1.0
```

- [ ] **Step 2: Check required tools**

```sh
cargo --version && rustup --version && claude --version && shellcheck --version | head -n 2 && command -v dash
```

Expected: every command prints a version or a path. If `shellcheck` or `dash` is missing, stop and ask the user to install it.

- [ ] **Step 3: Check that the stable toolchain has rust-analyzer** (spikes S1 and S4 need it)

```sh
rustup which --toolchain stable rust-analyzer
```

Expected: a path. If the output is `error: unknown binary 'rust-analyzer' ...`, STOP and ask the user for approval to run `rustup component add rust-analyzer --toolchain stable`. Do not run it without approval.

- [ ] **Step 4: Create a spike workspace outside the repository**

```sh
export SPIKE="$(mktemp -d)"
echo "$SPIKE"
printf '# Spike results\n\n' >"$SPIKE/results.md"
```

Keep `$SPIKE` for Tasks 1–5. Nothing under it is committed. For each spike, append to `$SPIKE/results.md`: the command, the evidence (quoted output or grep result), and a verdict of CONFIRMED, REFUTED, or INCONCLUSIVE.

The spikes run headless Claude Code sessions with `claude -p`:
- `--setting-sources project` keeps user settings, including already-enabled plugins such as the official `rust-analyzer-lsp`, out of the test.
- `--plugin-dir` loads only the spike plugin.
- `--session-id` fixes the transcript file name so evidence can be grepped. Transcripts are stored under `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects/`.

---

### Task 1: Spike S1 — the inline `sh -c` LSP launcher starts rust-analyzer

Validates spec §7 item 1.

**Files:** only under `$SPIKE`.

- [ ] **Step 1: Create a throwaway crate**

```sh
cd "$SPIKE" && cargo new --lib --vcs none s1-crate
cat >"$SPIKE/s1-crate/src/lib.rs" <<'EOF'
pub struct Answer;

pub fn answer() -> u32 {
    42
}
EOF
```

- [ ] **Step 2: Create a plugin that contains only the launcher from spec §4.1**

```sh
mkdir -p "$SPIKE/s1-plugin/.claude-plugin"
cat >"$SPIKE/s1-plugin/.claude-plugin/plugin.json" <<'EOF'
{
  "name": "spike-lsp",
  "version": "0.0.0",
  "description": "Spike: inline sh -c rust-analyzer launcher",
  "lspServers": {
    "rust-analyzer": {
      "command": "sh",
      "args": [
        "-c",
        "if command -v rustup >/dev/null 2>&1; then ra=$(rustup which --toolchain stable rust-analyzer) || exit 1; else ra=$(command -v rust-analyzer) || exit 1; fi; exec \"$ra\"",
        "ra-launch"
      ],
      "extensionToLanguage": { ".rs": "rust" },
      "initializationOptions": {
        "cargo": { "targetDir": true },
        "checkOnSave": false
      }
    }
  }
}
EOF
claude plugin validate "$SPIKE/s1-plugin"
```

Expected: `Validation passed` (an "author" warning is fine).

- [ ] **Step 3: Run a headless session that calls documentSymbol**

```sh
cd "$SPIKE/s1-crate"
SID=$(python3 -c 'import uuid; print(uuid.uuid4())')
claude -p --setting-sources project --plugin-dir "$SPIKE/s1-plugin" \
  --allowedTools LSP ToolSearch Bash --session-id "$SID" --model sonnet \
  "If the LSP tool is not in your tool list, load it with ToolSearch query select:LSP. Call LSP documentSymbol on src/lib.rs at line 1, character 1. If the result is empty, run 'sleep 15' with Bash and try again, at most 3 attempts. Print the final tool result verbatim and nothing else." \
  | tee "$SPIKE/s1-output.txt"
wc -c "$SPIKE/s1-output.txt"
```

Verdict:
- **CONFIRMED:** the output lists both `Answer` and `answer`.
- **REFUTED:** the output reports that the server crashed or failed to start, or that the LSP tool is unavailable.

Record the verdict and the output size in bytes. The size shows what one `documentSymbol` call costs.

- [ ] **Step 4: Informational variant — does `${CLAUDE_PLUGIN_ROOT}` expand in `command`?**

```sh
mkdir -p "$SPIKE/s1b-plugin/.claude-plugin"
cat >"$SPIKE/s1b-plugin/.claude-plugin/plugin.json" <<'EOF'
{
  "name": "spike-lsp-root",
  "version": "0.0.0",
  "description": "Spike: launcher script referenced through CLAUDE_PLUGIN_ROOT",
  "lspServers": {
    "rust-analyzer": {
      "command": "${CLAUDE_PLUGIN_ROOT}/ra-launch.sh",
      "extensionToLanguage": { ".rs": "rust" }
    }
  }
}
EOF
cat >"$SPIKE/s1b-plugin/ra-launch.sh" <<'EOF'
#!/bin/sh
exec "$(rustup which --toolchain stable rust-analyzer)"
EOF
chmod +x "$SPIKE/s1b-plugin/ra-launch.sh"
cd "$SPIKE/s1-crate"
claude -p --setting-sources project --plugin-dir "$SPIKE/s1b-plugin" \
  --allowedTools LSP ToolSearch Bash --model sonnet \
  "If the LSP tool is not in your tool list, load it with ToolSearch query select:LSP. Call LSP documentSymbol on src/lib.rs at line 1, character 1. If the result is empty, run 'sleep 15' with Bash and try again, at most 3 attempts. Print the final tool result verbatim and nothing else."
```

Record whether symbols came back: CONFIRMED means `${CLAUDE_PLUGIN_ROOT}` expands in `command`.

- [ ] **Step 5: Gate**

If Step 3 is REFUTED, first rule out a print-mode limitation: ask the user to start `claude --setting-sources project --plugin-dir "$SPIKE/s1-plugin"` interactively in `$SPIKE/s1-crate` and send the same prompt. If that also fails, STOP. Report the results to the user, because spec §4.1 needs a redesign. If Step 4 was CONFIRMED, propose the script-file launcher as the alternative.

---

### Task 2: Spikes S2 and S5 — SubagentStart context injection, and `CLAUDE_ENV_FILE` in subagents

Validates spec §7 items 2 and 5. It also records whether hooks receive `CLAUDE_PROJECT_DIR`.

**Files:** only under `$SPIKE`.

- [ ] **Step 1: Create the spike plugin**

```sh
mkdir -p "$SPIKE/s2-plugin/.claude-plugin" "$SPIKE/s2-plugin/hooks" "$SPIKE/s2-crate"
cat >"$SPIKE/s2-plugin/.claude-plugin/plugin.json" <<'EOF'
{
  "name": "spike-hooks",
  "version": "0.0.0",
  "description": "Spike: SubagentStart context and CLAUDE_ENV_FILE"
}
EOF
cat >"$SPIKE/s2-plugin/hooks/hooks.json" <<'EOF'
{
  "hooks": {
    "SessionStart": [
      { "hooks": [{ "type": "command", "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hook.sh\" SessionStart" }] }
    ],
    "SubagentStart": [
      { "hooks": [{ "type": "command", "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hook.sh\" SubagentStart" }] }
    ]
  }
}
EOF
cat >"$SPIKE/s2-plugin/hook.sh" <<'EOF'
#!/bin/sh
event=$1
if [ "$event" = SessionStart ] && [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  printf 'export SPIKE_ENV_MARKER=env-ok-7f3a\n' >>"$CLAUDE_ENV_FILE"
fi
printf '%s cwd=%s project=%s env_file=%s\n' "$event" "$PWD" "${CLAUDE_PROJECT_DIR:-unset}" "${CLAUDE_ENV_FILE:-unset}" >>"${SPIKE_LOG:-/dev/null}"
printf '{"hookSpecificOutput":{"hookEventName":"%s","additionalContext":"SPIKE-CONTEXT-%s-9c2e"}}\n' "$event" "$event"
EOF
claude plugin validate "$SPIKE/s2-plugin"
```

- [ ] **Step 2: Run a session that uses Bash in the main agent and in a subagent**

```sh
cd "$SPIKE/s2-crate"
export SPIKE_LOG="$SPIKE/s2-hook.log"
SID=$(python3 -c 'import uuid; print(uuid.uuid4())')
claude -p --setting-sources project --plugin-dir "$SPIKE/s2-plugin" \
  --allowedTools Bash Agent --session-id "$SID" --model sonnet \
  "Step 1: run the Bash command: echo main:\$SPIKE_ENV_MARKER. Step 2: use the Agent tool with subagent_type general-purpose and this prompt: 'Run the Bash command echo sub:\$SPIKE_ENV_MARKER and report its exact output. Then quote verbatim every piece of text in your context that contains SPIKE-CONTEXT.' Step 3: print the output of step 1 and the full subagent report." \
  | tee "$SPIKE/s2-output.txt"
cat "$SPIKE/s2-hook.log"
find "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects" -path "*$SID*" -name '*.jsonl' -print0 \
  | xargs -0 grep -l 'SPIKE-CONTEXT-SubagentStart-9c2e'
```

Verdicts:
- **S2** is CONFIRMED when the `find | grep` lists a file under a `subagents/` directory, meaning the SubagentStart marker reached the subagent transcript. It is REFUTED when no file under `subagents/` matches.
- **S5** is CONFIRMED when the output contains `sub:env-ok-7f3a`. It is REFUTED when the output contains `sub:` without the marker.
- **Also record:**
  - whether the output contains `main:env-ok-7f3a`, which shows that `CLAUDE_ENV_FILE` works at all;
  - the `project=` values in `s2-hook.log`, which show whether `CLAUDE_PROJECT_DIR` is set for SessionStart and for SubagentStart.

---

### Task 3: Spike S3 — skill `paths`, `${CLAUDE_SKILL_DIR}`, and `disable-model-invocation`

Validates spec §7 item 3. The substitution and invocation checks also protect the doctor skill.

**Files:** only under `$SPIKE`.

- [ ] **Step 1: Create the spike plugin and a crate with one `.rs` file and one `.txt` file**

```sh
mkdir -p "$SPIKE/s3-plugin/.claude-plugin" "$SPIKE/s3-plugin/skills/paths-probe" "$SPIKE/s3-plugin/skills/dir-probe"
cat >"$SPIKE/s3-plugin/.claude-plugin/plugin.json" <<'EOF'
{
  "name": "spike-skills",
  "version": "0.0.0",
  "description": "Spike: skill paths, CLAUDE_SKILL_DIR, disable-model-invocation"
}
EOF
cat >"$SPIKE/s3-plugin/skills/paths-probe/SKILL.md" <<'EOF'
---
name: paths-probe
description: Probe skill for a plugin test. Only relevant when .rs files are involved.
paths: ["**/*.rs"]
---

PATHS-PROBE-ACTIVE-51d0: when this text is in your context, end your final answer with the line PATHS-PROBE-ACTIVE-51d0.
EOF
cat >"$SPIKE/s3-plugin/skills/dir-probe/SKILL.md" <<'EOF'
---
name: dir-probe
description: Probe skill that prints its own directory.
disable-model-invocation: true
---

DIR-PROBE-BODY-3e1a. Print this line verbatim and nothing else: SKILL_DIR=${CLAUDE_SKILL_DIR}
EOF
claude plugin validate "$SPIKE/s3-plugin"
cd "$SPIKE" && cargo new --lib --vcs none s3-crate && printf 'plain notes\n' >"$SPIKE/s3-crate/notes.txt"
```

- [ ] **Step 2: Run A — touching a `.rs` file**

```sh
cd "$SPIKE/s3-crate"
SID_A=$(python3 -c 'import uuid; print(uuid.uuid4())')
claude -p --setting-sources project --plugin-dir "$SPIKE/s3-plugin" --allowedTools Read Skill \
  --session-id "$SID_A" --model sonnet "Read src/lib.rs, then reply with exactly: done" | tee "$SPIKE/s3-a.txt"
find "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects" -path "*$SID_A*" -name '*.jsonl' -print0 | xargs -0 grep -c 'PATHS-PROBE-ACTIVE-51d0'
```

- [ ] **Step 3: Run B — control, touching only a `.txt` file**

```sh
SID_B=$(python3 -c 'import uuid; print(uuid.uuid4())')
claude -p --setting-sources project --plugin-dir "$SPIKE/s3-plugin" --allowedTools Read Skill \
  --session-id "$SID_B" --model sonnet "Read notes.txt, then reply with exactly: done" | tee "$SPIKE/s3-b.txt"
find "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects" -path "*$SID_B*" -name '*.jsonl' -print0 | xargs -0 grep -c 'PATHS-PROBE-ACTIVE-51d0'
```

Verdict for `paths`:
- **CONFIRMED:** run A's transcript or output contains `PATHS-PROBE-ACTIVE-51d0` and run B's does not.
- **REFUTED (always active):** both runs contain it.
- **REFUTED (never active):** neither run contains it.

In the transcripts, ignore matches that are only the skill *listing*, meaning the name and description. The marker appears only in the skill body.

- [ ] **Step 4: Run C — `${CLAUDE_SKILL_DIR}` substitution through the slash command**

```sh
claude -p --setting-sources project --plugin-dir "$SPIKE/s3-plugin" --model sonnet "/spike-skills:dir-probe" | tee "$SPIKE/s3-c.txt"
```

Verdict:
- **CONFIRMED:** the output contains `SKILL_DIR=/` followed by an absolute path ending in `skills/dir-probe`.
- **REFUTED:** the output contains the literal `${CLAUDE_SKILL_DIR}`.
- **Retry interactively:** if print mode does not run slash commands (no `SKILL_DIR=` line at all), ask the user to start `claude --setting-sources project --plugin-dir "$SPIKE/s3-plugin"` interactively and type `/spike-skills:dir-probe`. Record that result instead.

- [ ] **Step 5: Run D — `disable-model-invocation` blocks model-initiated use**

```sh
SID_D=$(python3 -c 'import uuid; print(uuid.uuid4())')
claude -p --setting-sources project --plugin-dir "$SPIKE/s3-plugin" --allowedTools Skill \
  --session-id "$SID_D" --model sonnet "Use the dir-probe skill and print what it tells you to print." | tee "$SPIKE/s3-d.txt"
find "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects" -path "*$SID_D*" -name '*.jsonl' -print0 | xargs -0 grep -c 'DIR-PROBE-BODY-3e1a'
```

Verdict:
- **CONFIRMED:** there are no matches, so the skill body was never loaded.
- **REFUTED:** there is at least one match.

---

### Task 4: Spike S4 — does `rust-analyzer.toml` override the plugin's `initializationOptions`?

Validates spec §7 item 4. rust-analyzer runs build scripts when it loads a workspace, which creates its target directory. The name of the directory that appears shows which `cargo.targetDir` value won.

**Files:** only under `$SPIKE`.

- [ ] **Step 1: Control run — the plugin setting alone creates `target/rust-analyzer`**

```sh
cd "$SPIKE" && cargo new --lib --vcs none s4-control
printf 'fn main() {}\n' >"$SPIKE/s4-control/build.rs"
cd "$SPIKE/s4-control"
claude -p --setting-sources project --plugin-dir "$SPIKE/s1-plugin" \
  --allowedTools LSP ToolSearch Bash --model sonnet \
  "If the LSP tool is not in your tool list, load it with ToolSearch query select:LSP. Call LSP documentSymbol on src/lib.rs at line 1, character 1. Then run 'sleep 20' with Bash. Reply: done"
ls -d target/rust-analyzer ra-from-toml 2>/dev/null
```

Expected: `target/rust-analyzer` exists. If it does not, mark S4 INCONCLUSIVE, because the observation method does not work, and skip Step 2.

- [ ] **Step 2: Same setup plus a `rust-analyzer.toml` that sets a different target dir**

```sh
cd "$SPIKE" && cargo new --lib --vcs none s4-crate
printf 'fn main() {}\n' >"$SPIKE/s4-crate/build.rs"
printf '[cargo]\ntargetDir = "ra-from-toml"\n' >"$SPIKE/s4-crate/rust-analyzer.toml"
cd "$SPIKE/s4-crate"
claude -p --setting-sources project --plugin-dir "$SPIKE/s1-plugin" \
  --allowedTools LSP ToolSearch Bash --model sonnet \
  "If the LSP tool is not in your tool list, load it with ToolSearch query select:LSP. Call LSP documentSymbol on src/lib.rs at line 1, character 1. Then run 'sleep 20' with Bash. Reply: done"
ls -d target/rust-analyzer ra-from-toml 2>/dev/null
```

Verdict (record exactly which directories exist):
- **Only `ra-from-toml`:** `rust-analyzer.toml` overrides `initializationOptions`.
- **Only `target/rust-analyzer`:** plugin settings win.
- **Neither, or both:** INCONCLUSIVE.

---

### Task 5: Record spike results and prototype clarifications in the spec (decision gate)

**Files:**
- Modify: `docs/superpowers/specs/2026-09-13-rust-ai-lean-design.md`

- [ ] **Step 1: Replace spec §7 with the results**

Replace the numbered list under `## 7. Assumptions to validate first` with the paragraph and table below. Fill each Result cell with the verdict and one line of evidence from `$SPIKE/results.md`:

```markdown
Validated on <date> with Claude Code <version from `claude --version`>.

| # | Assumption | Result |
|---|---|---|
| 1 | Inline `sh -c` launcher starts rust-analyzer | <verdict — evidence> |
| 1b | `${CLAUDE_PLUGIN_ROOT}` expands in `lspServers.command` (informational) | <verdict — evidence> |
| 2 | `SubagentStart` can inject `additionalContext` | <verdict — evidence> |
| 3a | Skill `paths` activates on matching files | <verdict — evidence> |
| 3b | `${CLAUDE_SKILL_DIR}` is substituted in SKILL.md | <verdict — evidence> |
| 3c | `disable-model-invocation` blocks model-initiated use | <verdict — evidence> |
| 4 | `rust-analyzer.toml` overrides `initializationOptions` | <verdict — evidence> |
| 5 | `CLAUDE_ENV_FILE` exports reach subagent Bash | <verdict — evidence> |
| — | `CLAUDE_PROJECT_DIR` set in SessionStart / SubagentStart hooks | <yes/no each> |
```

- [ ] **Step 2: Apply the decision rules**

Apply each rule whose condition holds, editing the spec now and noting the plan change in the commit message:

- **S1 REFUTED:** already stopped in Task 1.
- **S2 REFUTED:**
  - In spec §4.2, delete `SubagentStart` from the event list and delete the sentence starting "The LSP line is omitted for subagents".
  - In Task 10, use the `hooks.json` variant without `SubagentStart`, given in Task 10, and delete test block 6 from `tests/shell/session-start.test.sh`.
- **S3a REFUTED:** in spec §4.5, delete the sentence about `paths`. In Task 14, delete the `paths:` line from the SKILL.md.
- **S3b REFUTED:** in spec §4.4, add after the Phase 1 command: "If `${CLAUDE_SKILL_DIR}` is not substituted, the skill falls back to the base directory Claude Code prints when the skill loads." The SKILL.md in Task 13 already contains this fallback.
- **S3c REFUTED:** in spec §4.4, add: "`disable-model-invocation` is not enforced; doctor stays safe because every fix requires approval."
- **S4:** the result selects the README paragraph in Task 15, Step 4.
- **S5 REFUTED:**
  - In spec §4.2, add to the subagent hint description: "adds `Cargo: pass -q to cargo commands.` because subagents do not inherit `CARGO_TERM_QUIET`."
  - In Task 10, apply the S5 variant given there.

- [ ] **Step 3: Apply clarifications found while prototyping the plan**

Make these edits in the spec:

1. **§4.3 `outline`, Rendering.** Add bullets:
   - "A range starts at the item's first token after its outer attributes and doc comments, so the start line is where `fn`/`struct`/`pub` appears."
   - "With `--docs`, the first doc line is printed on the next row with a blank range column."
2. **§4.3 `diag`.** Replace the Summary bullet with: "**Order:** errors, then warnings, then passed-through lines, then the summary line `N errors, M warnings (K duplicates merged)` (singular forms for 1)."
   - In the Errors bullet, change the notice to `… +K more error(s) (use --max-errors 0)`.
3. **§4.4 Phase 1 table.** Add a first row: `| plugin.lib | plugin/scripts/lib.sh exists (FAIL and stop otherwise) | — |`.
   - Add after the table: "`doctor.sh` sets `RUSTUP_AUTO_INSTALL=0` so no check can download a toolchain. `project.lock` and `project.crate-src` are INFO `skipped` when the project directory is not inside a Cargo package (`cargo locate-project --workspace` fails)."
4. **§4.2 hint table.** Replace `load via ToolSearch "select:LSP" if deferred` with `load it with ToolSearch query select:LSP if deferred`.
   - Add below the table: "Hint lines contain no double quotes or backslashes, so the hook can emit JSON without an escaping step."
5. **§8 item 2.** Replace with:
   - "`scripts/bench.sh` shows `rust-ai-lean outline` output ≤ 20% of `cat -n` for this repository's `src/outline.rs`, and quiet `cargo test` output ≤ 10% of the default for its generated 300-test crate."
   - "`tests/diag.rs` proves that `diag` keeps error `help:`/`note:` text while dropping status lines."

- [ ] **Step 4: Commit**

```sh
git add docs/superpowers/specs/2026-09-13-rust-ai-lean-design.md
git commit -m "Record spike results in design spec"
```

- [ ] **Step 5: Gate**

If any verdict is REFUTED or INCONCLUSIVE, show the user the §7 table and the decisions taken, then wait for confirmation before Task 6.

---

### Task 6: Crate scaffold, manifests, licenses, and version-consistency test

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `.gitignore`, `LICENSE-MIT`, `LICENSE-APACHE`, `plugin/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, `tests/plugin.rs`

**Interfaces:**
- Produces: binary `rust-ai-lean` with `--version` printing `rust-ai-lean 0.1.0`; `tests/plugin.rs` helper `read_json(rel: &str) -> serde_json::Value` (extended in Task 11).

- [ ] **Step 1: Create the crate manifest, entry point, and ignore file**

`Cargo.toml`:

```toml
[package]
name = "rust-ai-lean"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"
description = "Token-lean Rust tooling for AI coding agents"
repository = "https://github.com/enoqv/rust-ai-lean"
publish = false

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
prettyplease = "0.2"
proc-macro2 = { version = "1", features = ["span-locations"] }
quote = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
syn = { version = "2", features = ["full"] }

[dev-dependencies]
tempfile = "3"
```

`src/main.rs`:

```rust
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "rust-ai-lean",
    version,
    about = "Token-lean Rust tooling for AI coding agents"
)]
struct Cli {}

fn main() {
    Cli::parse();
}
```

`.gitignore`:

```gitignore
target/
```

- [ ] **Step 2: Write the failing version-consistency test**

`tests/plugin.rs`:

```rust
use std::fs;
use std::path::Path;

use serde_json::Value;

fn read_json(rel: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn versions_match_across_cargo_plugin_and_marketplace() {
    let cargo = env!("CARGO_PKG_VERSION");
    let plugin = read_json("plugin/.claude-plugin/plugin.json");
    let market = read_json(".claude-plugin/marketplace.json");
    assert_eq!(plugin["version"], cargo, "plugin.json version");
    let entry = market["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "rust-ai-lean")
        .expect("marketplace entry");
    assert_eq!(entry["version"], cargo, "marketplace.json version");
}
```

- [ ] **Step 3: Run it and confirm it fails**

Run: `cargo test --test plugin`
Expected: FAIL. `read_json` panics with `No such file or directory` because the manifests do not exist yet.

- [ ] **Step 4: Create the plugin and marketplace manifests**

`plugin/.claude-plugin/plugin.json`:

```json
{
  "name": "rust-ai-lean",
  "version": "0.1.0",
  "description": "Spend fewer tokens on Rust: working rust-analyzer LSP, quiet cargo, outline/crate-src/diag tools, and a doctor that checks it all",
  "author": { "name": "enoqv" },
  "homepage": "https://github.com/enoqv/rust-ai-lean",
  "license": "MIT OR Apache-2.0"
}
```

`.claude-plugin/marketplace.json`:

```json
{
  "$schema": "https://anthropic.com/claude-code/marketplace.schema.json",
  "name": "rust-ai-lean",
  "description": "Token-lean Rust development for Claude Code",
  "owner": { "name": "enoqv" },
  "plugins": [
    {
      "name": "rust-ai-lean",
      "description": "Spend fewer tokens on Rust: working rust-analyzer LSP, quiet cargo, outline/crate-src/diag tools, and a doctor that checks it all",
      "version": "0.1.0",
      "source": "./plugin",
      "category": "development"
    }
  ]
}
```

- [ ] **Step 5: Add the licenses**

`LICENSE-MIT`:

```text
MIT License

Copyright (c) 2026 enoqv

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Then fetch the canonical Apache text:

```sh
curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE
head -n 3 LICENSE-APACHE
```

Expected: the header includes `Apache License` and `Version 2.0, January 2004`.

- [ ] **Step 6: Run the tests and checks**

```sh
cargo test --test plugin
cargo run --quiet -- --version
cargo clippy --all-targets -- -D warnings
cargo fmt --check
claude plugin validate plugin
claude plugin validate .
```

Expected: 1 test passes; `rust-ai-lean 0.1.0`; clippy and fmt clean; both validations pass.

- [ ] **Step 7: Commit** (include `Cargo.lock`; `cargo install --locked` needs it)

```sh
git add Cargo.toml Cargo.lock src/main.rs .gitignore LICENSE-MIT LICENSE-APACHE plugin/.claude-plugin/plugin.json .claude-plugin/marketplace.json tests/plugin.rs
git commit -m "Scaffold rust-ai-lean crate and plugin manifests"
```

---

### Task 7: `outline` subcommand

**Files:**
- Create: `src/outline.rs`, `tests/common/mod.rs`, `tests/outline.rs`, `tests/fixtures/outline/sample.rs`, `tests/fixtures/outline/broken.rs`
- Modify: `src/main.rs` (full replacement below)

**Interfaces:**
- Produces:
  - `outline::run(paths: &[PathBuf], docs: bool) -> anyhow::Result<u8>`: the exit code is 1 if any path could not be read.
  - `tests/common/mod.rs` exposes:
    - `fixture(name: &str) -> PathBuf`
    - `run_in(dir: &Path, target: &Path, args: &[&str]) -> Output`
    - `stdout(&Output) -> String`
    - `stderr(&Output) -> String`
  - Output format: a path header line, then rows `"  {start}-{end:<9} {indent}{text}"`, with two spaces of indent per nesting level.

- [ ] **Step 1: Add the fixtures**

`tests/fixtures/outline/sample.rs` (line numbers matter; copy exactly):

```rust
//! Fixture for `outline` tests.

use std::fmt;

/// Maximum retries.
pub const MAX_RETRIES: u32 = 3;

static mut COUNTER: u64 = 0;

pub type Result<T> = std::result::Result<T, Error>;

/// A user record.
#[derive(Debug, Clone)]
pub struct User {
    /// Primary key.
    pub id: String,
    #[allow(dead_code)]
    name: String,
}

pub struct Id(pub String);

pub struct Marker;

pub enum Error {
    NotFound,
    Invalid(String),
    Conflict { expected: u64, actual: u64 },
}

pub union Bits {
    pub int: u32,
    pub float: f32,
}

pub trait Repository: Send + Sync {
    type Item;
    const NAME: &'static str;
    fn get(&self, id: &str) -> Option<Self::Item>;
    fn count(&self) -> usize {
        0
    }
}

impl<T> Repository for Vec<T>
where
    T: Clone + Send + Sync,
{
    type Item = T;
    const NAME: &'static str = "vec";
    fn get(&self, _id: &str) -> Option<T> {
        None
    }
}

impl User {
    pub const KIND: &'static str = "user";

    /// Creates a user.
    pub async fn insert_user(
        conn: &mut Vec<User>,
        id: &str,
        name: &str,
        display_name: Option<&str>,
        email: Option<&str>,
    ) -> Result<User> {
        let user = User { id: id.into(), name: name.into() };
        conn.push(user.clone());
        let _ = (display_name, email);
        Ok(user)
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

pub mod nested {
    pub fn helper() {}

    mod deeper {
        pub(crate) fn inner() {}
    }
}

mod external;

macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn one() {}

    #[test]
    fn two() {}
}
```

`tests/fixtures/outline/broken.rs`:

```rust
pub struct Ok1 {
    pub a: u32,
}

pub fn broken(x: u32 -> u32 {
    x
}

impl Ok1 {
    pub(crate) fn method(&self) {}
}
```

- [ ] **Step 2: Add the shared test helpers**

`tests/common/mod.rs`:

```rust
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Run the CLI in `dir` with an isolated cargo target directory.
pub fn run_in(dir: &Path, target: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rust-ai-lean"))
        .args(args)
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("run rust-ai-lean")
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}
```

- [ ] **Step 3: Write the failing tests**

`tests/outline.rs`:

```rust
mod common;

use common::{fixture, run_in, stderr, stdout};

fn outline(args: &[&str]) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    run_in(&fixture("outline"), tmp.path(), args)
}

#[test]
fn renders_signatures_with_line_ranges() {
    let out = outline(&["outline", "sample.rs"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    for line in [
        "sample.rs",
        "  6-6       pub const MAX_RETRIES: u32",
        "  14-19     pub struct User { pub id: String, name: String }",
        "  21-21     pub struct Id(pub String)",
        "  25-29     pub enum Error { NotFound, Invalid(..), Conflict { .. } }",
        "  36-43     pub trait Repository: Send + Sync",
        "  39-39       fn get(&self, id: &str) -> Option<Self::Item>",
        "  45-54     impl<T> Repository for Vec<T> where T: Clone + Send + Sync",
        "  60-71       pub async fn insert_user(conn: &mut Vec<User>, id: &str, name: &str, display_name: Option<&str>, email: Option<&str>) -> Result<User>",
        "  84-84         pub(crate) fn inner()",
        "  88-88     mod external",
        "  90-94     macro_rules! square",
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "missing line {line:?} in:\n{text}"
        );
    }
    assert!(
        !text.contains("conn.push"),
        "bodies must not be printed:\n{text}"
    );
    assert!(
        !text.contains("derive"),
        "attributes must not be printed:\n{text}"
    );
}

#[test]
fn collapses_cfg_test_modules() {
    let text = stdout(&outline(&["outline", "sample.rs"]));
    assert!(
        text.contains("  97-103    #[cfg(test)] mod tests  (2 items)\n"),
        "{text}"
    );
    assert!(!text.contains("fn one"), "{text}");
}

#[test]
fn docs_flag_adds_first_doc_line() {
    let text = stdout(&outline(&["outline", "--docs", "sample.rs"]));
    assert!(
        text.contains(
            "  6-6       pub const MAX_RETRIES: u32\n              /// Maximum retries.\n"
        ),
        "{text}"
    );
    let plain = stdout(&outline(&["outline", "sample.rs"]));
    assert!(!plain.contains("///"), "{plain}");
}

#[test]
fn unparsable_file_gets_approximate_outline() {
    let out = outline(&["outline", "broken.rs"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.starts_with("broken.rs: parse error at 5:"), "{text}");
    assert!(text.contains("approximate outline"), "{text}");
    assert!(
        text.contains("  5-5       pub fn broken(x: u32 -> u32\n"),
        "{text}"
    );
    assert!(
        text.contains("  10-10       pub(crate) fn method(&self)\n"),
        "{text}"
    );
}

#[test]
fn directories_are_walked_skipping_target_and_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    for (path, body) in [
        ("src/a.rs", "pub fn a() {}"),
        ("target/b.rs", "pub fn b() {}"),
        (".hidden/c.rs", "pub fn c() {}"),
    ] {
        let full = tmp.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, body).unwrap();
    }
    let out = run_in(tmp.path(), tmp.path(), &["outline", "."]);
    let text = stdout(&out);
    assert!(text.contains("pub fn a()"), "{text}");
    assert!(
        !text.contains("pub fn b()") && !text.contains("pub fn c()"),
        "{text}"
    );
}

#[test]
fn unreadable_path_exits_1_but_prints_the_rest() {
    let out = outline(&["outline", "missing.rs", "sample.rs"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("missing.rs"));
    assert!(stdout(&out).contains("pub struct Marker"));
}
```

- [ ] **Step 4: Run them and confirm they fail**

Run: `cargo test --test outline`
Expected: FAIL. The stderr assertions show `unexpected argument 'outline' found`.

- [ ] **Step 5: Implement**

`src/outline.rs`:

```rust
//! `outline`: item signatures with line ranges, bodies omitted.

use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{Attribute, Fields, ImplItem, Item, TraitItem};

/// One printed row: 1-based inclusive line range, nesting depth, text, and
/// the first doc-comment line when `--docs` is set.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub depth: usize,
    pub text: String,
    pub doc: Option<String>,
}

pub fn run(paths: &[PathBuf], docs: bool) -> anyhow::Result<u8> {
    let mut files = Vec::new();
    let mut failed = false;
    for path in paths {
        if path.is_dir() {
            collect_rs_files(path, &mut files);
        } else {
            files.push(path.clone());
        }
    }
    for file in files {
        match fs::read_to_string(&file) {
            Ok(src) => print!("{}", render_file(&file, &src, docs)),
            Err(err) => {
                eprintln!("{}: {err}", file.display());
                failed = true;
            }
        }
    }
    Ok(u8::from(failed))
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect_rs_files(&path, out);
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// Render one file's outline, including its header line.
pub fn render_file(path: &Path, src: &str, docs: bool) -> String {
    let mut out = format!("{}\n", path.display());
    let rows = match syn::parse_file(src) {
        Ok(file) => {
            let mut rows = Vec::new();
            items(&file.items, 0, docs, &mut rows);
            rows
        }
        Err(err) => {
            let at = err.span().start();
            out = format!(
                "{}: parse error at {}:{}; approximate outline\n",
                path.display(),
                at.line,
                at.column + 1
            );
            approximate(src)
        }
    };
    for row in rows {
        let range = format!("{}-{}", row.start, row.end);
        let indent = "  ".repeat(row.depth);
        out.push_str(&format!("  {range:<9} {indent}{}\n", row.text));
        if let Some(doc) = row.doc {
            out.push_str(&format!("  {:<9} {indent}  /// {doc}\n", ""));
        }
    }
    out
}

fn items(items: &[Item], depth: usize, docs: bool, rows: &mut Vec<Row>) {
    for item in items {
        item_rows(item, depth, docs, rows);
    }
}

fn item_rows(item: &Item, depth: usize, docs: bool, rows: &mut Vec<Row>) {
    let (start, end) = range_without_attrs(item);
    let push = |rows: &mut Vec<Row>, text: String, attrs: &[Attribute]| {
        let doc = if docs { first_doc_line(attrs) } else { None };
        rows.push(Row {
            start,
            end,
            depth,
            text,
            doc,
        });
    };
    match item {
        Item::Fn(f) => {
            let mut f = f.clone();
            f.attrs.clear();
            f.block = Box::new(syn::parse_quote!({}));
            push(rows, render(Item::Fn(f)), &item_attrs(item));
        }
        Item::Struct(s) => {
            let mut s = s.clone();
            s.attrs.clear();
            s.fields.iter_mut().for_each(|field| field.attrs.clear());
            push(rows, render(Item::Struct(s)), &item_attrs(item));
        }
        Item::Union(u) => {
            let mut u = u.clone();
            u.attrs.clear();
            u.fields
                .named
                .iter_mut()
                .for_each(|field| field.attrs.clear());
            push(rows, render(Item::Union(u)), &item_attrs(item));
        }
        Item::Enum(e) => {
            let names: Vec<String> = e
                .variants
                .iter()
                .map(|v| match v.fields {
                    Fields::Unit => v.ident.to_string(),
                    Fields::Unnamed(_) => format!("{}(..)", v.ident),
                    Fields::Named(_) => format!("{} {{ .. }}", v.ident),
                })
                .collect();
            let mut header = e.clone();
            header.attrs.clear();
            header.variants.clear();
            let text = format!("{} {{ {} }}", render(Item::Enum(header)), names.join(", "));
            push(rows, text, &item_attrs(item));
        }
        Item::Trait(t) => {
            let mut header = t.clone();
            header.attrs.clear();
            header.items.clear();
            push(rows, render(Item::Trait(header)), &item_attrs(item));
            for inner in &t.items {
                let (start, end) = range_without_attrs(inner);
                let text = render_trait_item(inner);
                rows.push(Row {
                    start,
                    end,
                    depth: depth + 1,
                    text,
                    doc: None,
                });
            }
        }
        Item::Impl(i) => {
            let mut header = i.clone();
            header.attrs.clear();
            header.items.clear();
            push(rows, render(Item::Impl(header)), &item_attrs(item));
            for inner in &i.items {
                let (start, end) = range_without_attrs(inner);
                let text = render_impl_item(inner);
                rows.push(Row {
                    start,
                    end,
                    depth: depth + 1,
                    text,
                    doc: None,
                });
            }
        }
        Item::Mod(m) => {
            let mut header = m.clone();
            header.attrs.clear();
            header.content = None;
            header.semi = Some(Default::default());
            let text = render(Item::Mod(header));
            match &m.content {
                Some((_, inner)) if is_cfg_test(&m.attrs) => {
                    let text = format!("#[cfg(test)] {text}  ({} items)", inner.len());
                    rows.push(Row {
                        start,
                        end,
                        depth,
                        text,
                        doc: None,
                    });
                }
                Some((_, inner)) => {
                    push(rows, text, &m.attrs);
                    items(inner, depth + 1, docs, rows);
                }
                None => push(rows, text, &m.attrs),
            }
        }
        Item::Const(c) => {
            let mut c = c.clone();
            c.attrs.clear();
            c.expr = Box::new(syn::parse_quote!(_));
            let text = render(Item::Const(c));
            push(
                rows,
                text.trim_end_matches(" = _").to_string(),
                &item_attrs(item),
            );
        }
        Item::Static(s) => {
            let mut s = s.clone();
            s.attrs.clear();
            s.expr = Box::new(syn::parse_quote!(_));
            let text = render(Item::Static(s));
            push(
                rows,
                text.trim_end_matches(" = _").to_string(),
                &item_attrs(item),
            );
        }
        Item::Type(t) => {
            let mut t = t.clone();
            t.attrs.clear();
            push(rows, render(Item::Type(t)), &item_attrs(item));
        }
        Item::Macro(m) => {
            if let Some(ident) = &m.ident {
                push(rows, format!("macro_rules! {ident}"), &m.attrs);
            }
        }
        _ => {}
    }
}

fn render_trait_item(item: &TraitItem) -> String {
    let mut item = item.clone();
    match &mut item {
        TraitItem::Fn(f) => {
            f.attrs.clear();
            f.default = None;
            f.semi_token = Some(Default::default());
        }
        TraitItem::Const(c) => c.attrs.clear(),
        TraitItem::Type(t) => t.attrs.clear(),
        TraitItem::Macro(m) => m.attrs.clear(),
        _ => {}
    }
    let wrapper: Item = syn::parse_quote!(trait __RustAiLean { #item });
    inner_of(&render_raw(wrapper))
}

fn render_impl_item(item: &ImplItem) -> String {
    let mut item = item.clone();
    match &mut item {
        ImplItem::Fn(f) => {
            f.attrs.clear();
            f.block = syn::parse_quote!({});
        }
        ImplItem::Const(c) => {
            c.attrs.clear();
            c.expr = syn::parse_quote!(_);
        }
        ImplItem::Type(t) => t.attrs.clear(),
        ImplItem::Macro(m) => m.attrs.clear(),
        _ => {}
    }
    let wrapper: Item = syn::parse_quote!(impl __RustAiLean { #item });
    inner_of(&render_raw(wrapper))
        .trim_end_matches(" = _")
        .to_string()
}

/// Pretty-print a single item as a one-line signature with its body removed.
fn render(item: Item) -> String {
    strip_body(&collapse(&render_raw(item)))
}

fn render_raw(item: Item) -> String {
    prettyplease::unparse(&syn::File {
        shebang: None,
        attrs: Vec::new(),
        items: vec![item],
    })
}

/// Text between a wrapper's first line (`trait X {`) and its closing `}`.
fn inner_of(rendered: &str) -> String {
    let lines: Vec<&str> = rendered.lines().collect();
    let inner = lines
        .get(1..lines.len().saturating_sub(1))
        .unwrap_or(&[])
        .join("\n");
    strip_body(&collapse(&inner))
}

/// Join all lines into one and undo the line-wrapping punctuation.
fn collapse(text: &str) -> String {
    let mut s = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [
        (", )", ")"),
        ("( ", "("),
        (" )", ")"),
        (", >", ">"),
        ("< ", "<"),
        (", }", " }"),
        (", ]", "]"),
        ("[ ", "["),
    ] {
        while s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

fn strip_body(text: &str) -> String {
    let t = text.trim_end();
    let t = t.strip_suffix("{}").map(str::trim_end).unwrap_or(t);
    let t = t.strip_suffix(';').unwrap_or(t);
    let t = t.strip_suffix(',').unwrap_or(t);
    t.trim_end().to_string()
}

fn item_attrs(item: &Item) -> Vec<Attribute> {
    match item {
        Item::Fn(x) => x.attrs.clone(),
        Item::Struct(x) => x.attrs.clone(),
        Item::Union(x) => x.attrs.clone(),
        Item::Enum(x) => x.attrs.clone(),
        Item::Trait(x) => x.attrs.clone(),
        Item::Impl(x) => x.attrs.clone(),
        Item::Const(x) => x.attrs.clone(),
        Item::Static(x) => x.attrs.clone(),
        Item::Type(x) => x.attrs.clone(),
        _ => Vec::new(),
    }
}

fn first_doc_line(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("doc") {
            return None;
        }
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return None;
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = &nv.value
        else {
            return None;
        };
        let line = s.value().trim().to_string();
        (!line.is_empty()).then_some(line)
    })
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .map(|id| id == "test")
                .unwrap_or(false)
    })
}

/// Line range of a node, starting at its first token after outer attributes.
fn range_without_attrs<T: ToTokens>(node: &T) -> (usize, usize) {
    let tokens: Vec<_> = strip_outer_attrs(node.to_token_stream())
        .into_iter()
        .collect();
    let start = tokens.first().map(|t| t.span().start().line).unwrap_or(0);
    let end = tokens.last().map(|t| t.span().end().line).unwrap_or(start);
    (start, end)
}

/// Drop leading `# [ ... ]` groups (outer attributes and doc comments).
fn strip_outer_attrs(tokens: TokenStream) -> TokenStream {
    use proc_macro2::{Delimiter, TokenTree};
    let mut iter = tokens.into_iter().peekable();
    loop {
        match iter.peek() {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                let hash = iter.next();
                match iter.peek() {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => {
                        iter.next();
                    }
                    _ => return hash.into_iter().chain(iter).collect(),
                }
            }
            _ => return iter.collect(),
        }
    }
}

/// Fallback for files that do not parse: keyword-based line scan.
fn approximate(src: &str) -> Vec<Row> {
    const STARTS: &[&str] = &[
        "fn ",
        "async fn ",
        "const fn ",
        "unsafe fn ",
        "extern fn ",
        "struct ",
        "enum ",
        "union ",
        "trait ",
        "unsafe trait ",
        "impl ",
        "impl<",
        "unsafe impl",
        "mod ",
        "type ",
        "const ",
        "static ",
        "macro_rules!",
    ];
    src.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let indent = line.len() - line.trim_start().len();
            let mut rest = line.trim();
            if let Some(r) = rest.strip_prefix("pub") {
                rest = match r.strip_prefix('(') {
                    Some(r) => r.split_once(')').map(|(_, r)| r).unwrap_or(r),
                    None => r,
                }
                .trim_start();
            }
            STARTS.iter().any(|s| rest.starts_with(s)).then(|| Row {
                start: i + 1,
                end: i + 1,
                depth: indent / 4,
                text: strip_body(line.trim())
                    .trim_end_matches('{')
                    .trim_end()
                    .to_string(),
                doc: None,
            })
        })
        .collect()
}
```

`src/main.rs` (replace the whole file):

```rust
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod outline;

#[derive(Parser)]
#[command(
    name = "rust-ai-lean",
    version,
    about = "Token-lean Rust tooling for AI coding agents"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print item signatures with line ranges, without bodies
    Outline {
        /// Also print the first doc-comment line of each item
        #[arg(long)]
        docs: bool,
        /// Files or directories
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Outline { docs, paths } => outline::run(&paths, docs),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 6: Run the tests and checks**

```sh
cargo test --test outline
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: 6 tests pass; clippy and fmt are clean.

- [ ] **Step 7: Commit**

```sh
git add src/outline.rs src/main.rs tests/common/mod.rs tests/outline.rs tests/fixtures/outline
git commit -m "Add outline subcommand"
```

---

### Task 8: `crate-src` subcommand

**Files:**
- Create: `src/crate_src.rs`, `tests/crate_src.rs`, `tests/fixtures/deps/Cargo.toml`, `tests/fixtures/deps/src/lib.rs`, `tests/fixtures/deps/local-helper/Cargo.toml`, `tests/fixtures/deps/local-helper/src/lib.rs`, `tests/fixtures/deps/Cargo.lock` (generated)
- Modify: `src/main.rs` (full replacement below)

**Interfaces:**
- Consumes: `tests/common/mod.rs` from Task 7.
- Produces:
  - `crate_src::run(krate: &str, manifest_path: Option<&Path>, path_only: bool) -> anyhow::Result<u8>`, with exit code 0 when found, 1 when not found or the lock is stale, and 2 when there is no manifest.
  - Output row format: `"{name} {version} ({source|path}) features=[{a,b}]\n  {crate_root}\n"`.

- [ ] **Step 1: Add the fixture crate**

`tests/fixtures/deps/Cargo.toml`:

```toml
[package]
name = "deps-fixture"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
itoa = "=1.0.18"
local-helper = { path = "local-helper" }
```

`tests/fixtures/deps/src/lib.rs`:

```rust
pub fn f() -> String { itoa::Buffer::new().format(1).to_string() + &local_helper::name() }
```

`tests/fixtures/deps/local-helper/Cargo.toml`:

```toml
[package]
name = "local-helper"
version = "0.2.0"
edition = "2021"
publish = false
```

`tests/fixtures/deps/local-helper/src/lib.rs`:

```rust
pub fn name() -> String { "helper".into() }
```

Generate and check the lockfile:

```sh
(cd tests/fixtures/deps && cargo generate-lockfile)
grep -A1 'name = "itoa"' tests/fixtures/deps/Cargo.lock
```

Expected: `version = "1.0.18"`.

- [ ] **Step 2: Write the failing tests**

`tests/crate_src.rs`:

```rust
mod common;

use common::{fixture, run_in, stderr, stdout};

#[test]
fn resolves_registry_dependency_to_locked_version() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("deps"), tmp.path(), &["crate-src", "itoa"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.starts_with("itoa 1.0.18 (registry+"), "{text}");
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .trim_start()
            .ends_with("itoa-1.0.18"),
        "{text}"
    );
}

#[test]
fn matches_path_dependency_ignoring_hyphen_underscore() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("deps"), tmp.path(), &["crate-src", "local_helper"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).starts_with("local-helper 0.2.0 (path) features=[]\n"));
}

#[test]
fn path_only_prints_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("deps"),
        tmp.path(),
        &["crate-src", "itoa", "--path-only"],
    );
    let text = stdout(&out);
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.trim_end().ends_with("itoa-1.0.18"), "{text}");
}

#[test]
fn unknown_crate_exits_1_with_suggestions() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("deps"), tmp.path(), &["crate-src", "ito"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("no package named `ito`"), "{err}");
    assert!(err.contains("similar: itoa"), "{err}");
}

#[test]
fn no_manifest_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(tmp.path(), tmp.path(), &["crate-src", "itoa"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

#[test]
fn stale_lock_exits_1_and_leaves_lock_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("deps");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("local-helper/src")).unwrap();
    for file in [
        "Cargo.lock",
        "src/lib.rs",
        "local-helper/Cargo.toml",
        "local-helper/src/lib.rs",
    ] {
        std::fs::copy(fixture("deps").join(file), dir.join(file)).unwrap();
    }
    let manifest = std::fs::read_to_string(fixture("deps").join("Cargo.toml")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        manifest.replace("=1.0.18", "=1.0.17"),
    )
    .unwrap();
    let lock_before = std::fs::read(dir.join("Cargo.lock")).unwrap();
    let out = run_in(&dir, tmp.path(), &["crate-src", "itoa"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("crate-src never modifies it"));
    assert_eq!(std::fs::read(dir.join("Cargo.lock")).unwrap(), lock_before);
}
```

- [ ] **Step 3: Run them and confirm they fail**

Run: `cargo test --test crate_src`
Expected: FAIL with `unexpected argument 'crate-src' found`.

- [ ] **Step 4: Implement**

`src/crate_src.rs`:

```rust
//! `crate-src`: dependency source location for the exact locked version.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
    pub resolve: Option<Resolve>,
}

#[derive(Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub manifest_path: PathBuf,
}

#[derive(Deserialize)]
pub struct Resolve {
    pub nodes: Vec<Node>,
}

#[derive(Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub features: Vec<String>,
}

pub fn run(krate: &str, manifest_path: Option<&Path>, path_only: bool) -> anyhow::Result<u8> {
    let mut cmd = Command::new("cargo");
    cmd.args(["metadata", "--format-version", "1", "--locked"]);
    if let Some(path) = manifest_path {
        cmd.arg("--manifest-path").arg(path);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{stderr}");
        if stderr.contains("could not find `Cargo.toml`") {
            return Ok(2);
        }
        if stderr.contains("--locked") {
            eprintln!("hint: Cargo.lock is missing or out of date; crate-src never modifies it");
        }
        return Ok(1);
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let (text, code) = render(&metadata, krate, path_only);
    if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    Ok(code)
}

fn normalize(name: &str) -> String {
    name.replace('-', "_")
}

/// Render matches for `krate`; returns the text and the exit code.
pub fn render(metadata: &Metadata, krate: &str, path_only: bool) -> (String, u8) {
    let wanted = normalize(krate);
    let features: HashMap<&str, &[String]> = metadata
        .resolve
        .iter()
        .flat_map(|r| &r.nodes)
        .map(|n| (n.id.as_str(), n.features.as_slice()))
        .collect();
    let mut matches: Vec<&Package> = metadata
        .packages
        .iter()
        .filter(|p| normalize(&p.name) == wanted)
        .collect();
    matches.sort_by(|a, b| a.version.cmp(&b.version).then(a.id.cmp(&b.id)));
    if matches.is_empty() {
        let similar: BTreeSet<&str> = metadata
            .packages
            .iter()
            .filter(|p| normalize(&p.name).contains(&wanted))
            .map(|p| p.name.as_str())
            .collect();
        let mut text = format!("error: no package named `{krate}` in the dependency graph\n");
        if !similar.is_empty() {
            let list: Vec<&str> = similar.into_iter().take(5).collect();
            text.push_str(&format!("similar: {}\n", list.join(", ")));
        }
        return (text, 1);
    }
    let mut text = String::new();
    for pkg in matches {
        let root = pkg.manifest_path.parent().unwrap_or(Path::new(""));
        if path_only {
            text.push_str(&format!("{}\n", root.display()));
            continue;
        }
        let mut feats: Vec<&str> = features
            .get(pkg.id.as_str())
            .map(|f| f.iter().map(String::as_str).collect())
            .unwrap_or_default();
        feats.sort_unstable();
        let source = pkg.source.as_deref().unwrap_or("path");
        text.push_str(&format!(
            "{} {} ({}) features=[{}]\n  {}\n",
            pkg.name,
            pkg.version,
            source,
            feats.join(","),
            root.display()
        ));
    }
    (text, 0)
}
```

`src/main.rs` (replace the whole file):

```rust
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod crate_src;
mod outline;

#[derive(Parser)]
#[command(
    name = "rust-ai-lean",
    version,
    about = "Token-lean Rust tooling for AI coding agents"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print item signatures with line ranges, without bodies
    Outline {
        /// Also print the first doc-comment line of each item
        #[arg(long)]
        docs: bool,
        /// Files or directories
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Locate a dependency's source for the exact version in Cargo.lock
    CrateSrc {
        #[arg(value_name = "CRATE")]
        krate: String,
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Print only crate root paths, one per line
        #[arg(long)]
        path_only: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Outline { docs, paths } => outline::run(&paths, docs),
        Cmd::CrateSrc {
            krate,
            manifest_path,
            path_only,
        } => crate_src::run(&krate, manifest_path.as_deref(), path_only),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 5: Run the tests and checks**

```sh
cargo test --test crate_src --test outline
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: 12 tests pass (6 + 6); clippy and fmt are clean. `crate-src` tests need crates.io access, or `itoa 1.0.18` already in the local cargo cache.

- [ ] **Step 6: Commit**

```sh
git add src/crate_src.rs src/main.rs tests/crate_src.rs tests/fixtures/deps
git commit -m "Add crate-src subcommand"
```

---

### Task 9: `diag` subcommand

**Files:**
- Create: `src/diag.rs`, `tests/diag.rs`, `tests/fixtures/diag-noisy/{Cargo.toml,src/lib.rs,Cargo.lock}`, `tests/fixtures/diag-buildrs/{Cargo.toml,build.rs,src/lib.rs,Cargo.lock}`
- Modify: `src/main.rs` (full replacement below)

**Interfaces:**
- Consumes: `tests/common/mod.rs` from Task 7.
- Produces:
  - `diag::Sub` (`Check | Clippy | Build`, a clap `ValueEnum`).
  - `diag::run(sub: Sub, max_errors: usize, full_warnings: bool, cargo_args: &[String]) -> anyhow::Result<u8>`, which returns cargo's exit code.

- [ ] **Step 1: Add the fixtures**

`tests/fixtures/diag-noisy/Cargo.toml`:

```toml
[package]
name = "diag-noisy"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]
```

`tests/fixtures/diag-noisy/src/lib.rs` (25 unused variables followed by two errors; line numbers matter):

```rust
pub fn unused_00() { let value_00 = 0; }
pub fn unused_01() { let value_01 = 1; }
pub fn unused_02() { let value_02 = 2; }
pub fn unused_03() { let value_03 = 3; }
pub fn unused_04() { let value_04 = 4; }
pub fn unused_05() { let value_05 = 5; }
pub fn unused_06() { let value_06 = 6; }
pub fn unused_07() { let value_07 = 7; }
pub fn unused_08() { let value_08 = 8; }
pub fn unused_09() { let value_09 = 9; }
pub fn unused_10() { let value_10 = 10; }
pub fn unused_11() { let value_11 = 11; }
pub fn unused_12() { let value_12 = 12; }
pub fn unused_13() { let value_13 = 13; }
pub fn unused_14() { let value_14 = 14; }
pub fn unused_15() { let value_15 = 15; }
pub fn unused_16() { let value_16 = 16; }
pub fn unused_17() { let value_17 = 17; }
pub fn unused_18() { let value_18 = 18; }
pub fn unused_19() { let value_19 = 19; }
pub fn unused_20() { let value_20 = 20; }
pub fn unused_21() { let value_21 = 21; }
pub fn unused_22() { let value_22 = 22; }
pub fn unused_23() { let value_23 = 23; }
pub fn unused_24() { let value_24 = 24; }
pub fn mismatch() -> u32 { "not a number" }
pub fn missing() -> u32 { undefined_value }
```

`tests/fixtures/diag-buildrs/Cargo.toml`:

```toml
[package]
name = "diag-buildrs"
version = "0.1.0"
edition = "2021"
publish = false
build = "build.rs"

[workspace]
```

`tests/fixtures/diag-buildrs/build.rs`:

```rust
fn main() { eprintln!("BUILD-SCRIPT-MARKER: generator failed"); std::process::exit(3); }
```

`tests/fixtures/diag-buildrs/src/lib.rs`:

```rust
pub fn f() {}
```

Generate the lockfiles; neither fixture has dependencies:

```sh
(cd tests/fixtures/diag-noisy && cargo generate-lockfile)
(cd tests/fixtures/diag-buildrs && cargo generate-lockfile)
```

- [ ] **Step 2: Write the failing tests**

`tests/diag.rs`:

```rust
mod common;

use common::{fixture, run_in, stdout};

#[test]
fn errors_survive_many_warnings_and_come_first() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--all-targets"],
    );
    assert_eq!(out.status.code(), Some(101));
    let text = stdout(&out);
    assert!(
        text.starts_with("error[E0425]: cannot find value `undefined_value`"),
        "{text}"
    );
    assert!(text.contains("error[E0308]: mismatched types"), "{text}");
    assert!(
        text.contains("expected `u32` because of return type"),
        "help/label lines kept:\n{text}"
    );
    assert_eq!(
        text.matches(": warning[unused_variables]: unused variable")
            .count(),
        25,
        "{text}"
    );
    assert!(
        text.ends_with("2 errors, 25 warnings (27 duplicates merged)\n"),
        "{text}"
    );
    for status in ["Checking", "Compiling", "Finished"] {
        assert!(
            !text.lines().any(|l| l.trim_start().starts_with(status)),
            "{status} leaked:\n{text}"
        );
    }
}

#[test]
fn max_errors_truncates_with_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--max-errors", "1"],
    );
    let text = stdout(&out);
    assert_eq!(
        text.matches("\nerror[E").count() + usize::from(text.starts_with("error[E")),
        1,
        "{text}"
    );
    assert!(
        text.contains("… +1 more error (use --max-errors 0)"),
        "{text}"
    );
}

#[test]
fn full_warnings_render_complete_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--full-warnings"],
    );
    let text = stdout(&out);
    assert!(
        text.contains("warning: unused variable: `value_00`\n --> src/lib.rs:1:26"),
        "{text}"
    );
    assert!(
        text.contains("help: if this is intentional, prefix it with an underscore"),
        "{text}"
    );
}

#[test]
fn non_json_cargo_errors_pass_through() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("diag-buildrs"), tmp.path(), &["diag", "build"]);
    assert_eq!(out.status.code(), Some(101));
    let text = stdout(&out);
    assert!(
        text.contains("failed to run custom build command"),
        "{text}"
    );
    assert!(
        text.contains("BUILD-SCRIPT-MARKER: generator failed"),
        "{text}"
    );
}
```

- [ ] **Step 3: Run them and confirm they fail**

Run: `cargo test --test diag`
Expected: FAIL with `unexpected argument 'diag' found`.

- [ ] **Step 4: Implement**

`src/diag.rs`:

```rust
//! `diag`: run cargo with JSON diagnostics and print a compact report.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

use clap::ValueEnum;
use serde_json::Value;

#[derive(Clone, Copy, ValueEnum)]
pub enum Sub {
    Check,
    Clippy,
    Build,
}

impl Sub {
    fn as_str(self) -> &'static str {
        match self {
            Sub::Check => "check",
            Sub::Clippy => "clippy",
            Sub::Build => "build",
        }
    }
}

/// Cargo status verbs that carry no diagnostic information.
const STATUS_VERBS: &[&str] = &[
    "Compiling",
    "Checking",
    "Fresh",
    "Finished",
    "Blocking",
    "Downloading",
    "Downloaded",
    "Locking",
    "Updating",
    "Adding",
];

#[derive(Default)]
pub struct Report {
    passthrough: Vec<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
    seen: HashSet<String>,
    duplicates: usize,
}

impl Report {
    /// Feed one line of cargo stdout.
    pub fn stdout_line(&mut self, line: &str, full_warnings: bool) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            self.passthrough.push(line.to_string());
            return;
        };
        if value["reason"] != "compiler-message" {
            return;
        }
        let msg = &value["message"];
        let level = msg["level"].as_str().unwrap_or("");
        let text = msg["message"].as_str().unwrap_or("");
        let no_spans = msg["spans"].as_array().is_none_or(|s| s.is_empty());
        if no_spans && (text.starts_with("aborting due to") || text.ends_with("emitted")) {
            return;
        }
        let rendered = msg["rendered"]
            .as_str()
            .unwrap_or(text)
            .trim_end()
            .to_string();
        if level.starts_with("error") {
            self.add(rendered, true);
        } else if level == "warning" {
            let entry = if full_warnings {
                rendered
            } else {
                one_line(msg, text)
            };
            self.add(entry, false);
        }
    }

    /// Feed one line of cargo stderr.
    pub fn stderr_line(&mut self, line: &str) {
        let first = line.split_whitespace().next().unwrap_or("");
        if !STATUS_VERBS.contains(&first) {
            self.passthrough.push(line.to_string());
        }
    }

    fn add(&mut self, entry: String, is_error: bool) {
        if !self.seen.insert(entry.clone()) {
            self.duplicates += 1;
            return;
        }
        if is_error {
            self.errors.push(entry);
        } else {
            self.warnings.push(entry);
        }
    }

    pub fn render(&self, max_errors: usize) -> String {
        let mut out = String::new();
        let shown = if max_errors == 0 {
            self.errors.len()
        } else {
            max_errors.min(self.errors.len())
        };
        for err in &self.errors[..shown] {
            out.push_str(err);
            out.push_str("\n\n");
        }
        if shown < self.errors.len() {
            let hidden = self.errors.len() - shown;
            let noun = if hidden == 1 { "error" } else { "errors" };
            out.push_str(&format!("… +{hidden} more {noun} (use --max-errors 0)\n\n"));
        }
        for warning in &self.warnings {
            out.push_str(warning);
            out.push('\n');
        }
        for line in &self.passthrough {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!(
            "{}, {} ({} duplicates merged)\n",
            plural(self.errors.len(), "error"),
            plural(self.warnings.len(), "warning"),
            self.duplicates
        ));
        out
    }
}

fn one_line(msg: &Value, text: &str) -> String {
    let span = msg["spans"]
        .as_array()
        .and_then(|spans| spans.iter().find(|s| s["is_primary"] == true));
    let code = msg["code"]["code"]
        .as_str()
        .map(|c| format!("[{c}]"))
        .unwrap_or_default();
    match span {
        Some(s) => format!(
            "{}:{}:{}: warning{code}: {text}",
            s["file_name"].as_str().unwrap_or("?"),
            s["line_start"],
            s["column_start"]
        ),
        None => format!("warning{code}: {text}"),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

pub fn run(
    sub: Sub,
    max_errors: usize,
    full_warnings: bool,
    cargo_args: &[String],
) -> anyhow::Result<u8> {
    let mut child = Command::new("cargo")
        .arg(sub.as_str())
        .arg("--message-format=json")
        .args(cargo_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_thread = thread::spawn(move || {
        BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
            .collect::<Vec<_>>()
    });
    let mut report = Report::default();
    let stdout = child.stdout.take().expect("piped stdout");
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        report.stdout_line(&line, full_warnings);
    }
    for line in stderr_thread.join().unwrap_or_default() {
        report.stderr_line(&line);
    }
    let status = child.wait()?;
    print!("{}", report.render(max_errors));
    Ok(status.code().map_or(1, |c| u8::try_from(c).unwrap_or(1)))
}
```

`src/main.rs` (replace the whole file):

```rust
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod crate_src;
mod diag;
mod outline;

#[derive(Parser)]
#[command(
    name = "rust-ai-lean",
    version,
    about = "Token-lean Rust tooling for AI coding agents"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print item signatures with line ranges, without bodies
    Outline {
        /// Also print the first doc-comment line of each item
        #[arg(long)]
        docs: bool,
        /// Files or directories
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Locate a dependency's source for the exact version in Cargo.lock
    CrateSrc {
        #[arg(value_name = "CRATE")]
        krate: String,
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Print only crate root paths, one per line
        #[arg(long)]
        path_only: bool,
    },
    /// Run cargo and print compact diagnostics
    Diag {
        #[arg(value_enum)]
        sub: diag::Sub,
        /// Maximum errors to print; 0 = unlimited
        #[arg(long, default_value_t = 20)]
        max_errors: usize,
        /// Render warnings in full instead of one line each
        #[arg(long)]
        full_warnings: bool,
        /// Arguments passed through to cargo
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cargo_args: Vec<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Outline { docs, paths } => outline::run(&paths, docs),
        Cmd::CrateSrc {
            krate,
            manifest_path,
            path_only,
        } => crate_src::run(&krate, manifest_path.as_deref(), path_only),
        Cmd::Diag {
            sub,
            max_errors,
            full_warnings,
            cargo_args,
        } => diag::run(sub, max_errors, full_warnings, &cargo_args),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 5: Run the tests and checks**

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: all tests pass (outline 6, crate_src 6, diag 4, plugin 1); clippy and fmt are clean.

- [ ] **Step 6: Commit**

```sh
git add src/diag.rs src/main.rs tests/diag.rs tests/fixtures/diag-noisy tests/fixtures/diag-buildrs
git commit -m "Add diag subcommand"
```

---

### Task 10: Session hook, shared shell helpers, and shell test harness

**Files:**
- Create: `plugin/scripts/lib.sh`, `plugin/hooks/session-start.sh`, `plugin/hooks/hooks.json`, `tests/shell/lib.sh`, `tests/shell/run.sh`, `tests/shell/session-start.test.sh`

**Interfaces:**
- Produces:
  - `ral_resolve_ra` prints a rust-analyzer path, or returns 1.
  - `ral_is_rust_project <dir>` returns 0 when the directory is a Rust project.
  - Hook contract: `session-start.sh <SessionStart|SubagentStart>` prints hook JSON or nothing, and always exits 0.
  - Test helpers: `assert_eq`, `assert_contains`, `assert_not_contains`, `make_stub <dir> <name> <body>`, and `finish <label>`. `run.sh [shell]` runs every `tests/shell/*.test.sh`.

- [ ] **Step 1: Write the harness and the failing hook tests**

`tests/shell/lib.sh`:

```sh
# Test helpers. POSIX sh; sourced by tests/shell/*.test.sh.

fails=0

assert_contains() { # haystack needle label
  case $1 in
    *"$2"*) ;;
    *) printf 'FAIL %s: expected to contain [%s]\n--- got ---\n%s\n' "$3" "$2" "$1"; fails=$((fails + 1)) ;;
  esac
}

assert_not_contains() { # haystack needle label
  case $1 in
    *"$2"*) printf 'FAIL %s: expected NOT to contain [%s]\n--- got ---\n%s\n' "$3" "$2" "$1"; fails=$((fails + 1)) ;;
  esac
}

assert_eq() { # actual expected label
  [ "$1" = "$2" ] || { printf 'FAIL %s: expected [%s] got [%s]\n' "$3" "$2" "$1"; fails=$((fails + 1)); }
}

# make_stub <dir> <name> <body>: executable script on a fake PATH.
make_stub() {
  mkdir -p "$1"
  printf '#!/bin/sh\n%s\n' "$3" >"$1/$2"
  chmod +x "$1/$2"
}

finish() {
  if [ "$fails" -eq 0 ]; then echo "ok: $1"; else echo "$fails failure(s): $1"; exit 1; fi
}
```

`tests/shell/run.sh`:

```sh
#!/bin/sh
# Run all shell tests under the given shell (default: sh). Usage: run.sh [shell]
set -u
shell=${1:-sh}
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
status=0
for t in "$here"/*.test.sh; do
  "$shell" "$t" || status=1
done
exit $status
```

`tests/shell/session-start.test.sh`:

```sh
#!/bin/sh
# Tests for plugin/hooks/session-start.sh. Run via tests/shell/run.sh.
set -u
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH='' cd -- "$here/../.." && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"
hook="$repo/plugin/hooks/session-start.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/rustproj/src" "$tmp/plain" "$tmp/multi/server"
: >"$tmp/rustproj/Cargo.toml"
: >"$tmp/multi/server/Cargo.toml"
base_path=/usr/bin:/bin

# Stub directories for the four environment states.
make_stub "$tmp/bin-ra" rustup 'echo /opt/stub/rust-analyzer'
make_stub "$tmp/bin-cli" rust-ai-lean 'echo rust-ai-lean 0.1.0'
make_stub "$tmp/bin-nora" rustup 'exit 1'

run_hook() { # event project path [extra env assignments via env]
  env -i PATH="$3" HOME="$tmp" CLAUDE_PROJECT_DIR="$2" CLAUDE_ENV_FILE="$tmp/envfile" \
    sh "$hook" "$1" </dev/null
}

# 1. Non-Rust project: no output, but env var still written.
: >"$tmp/envfile"
out=$(run_hook SessionStart "$tmp/plain" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_eq "$out" "" "non-rust project prints nothing"
assert_eq "$(cat "$tmp/envfile")" "export CARGO_TERM_QUIET=true" "env file written for any project"

# 2. Everything available.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_contains "$out" '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"[rust-ai-lean] Rust project' "full: json prefix"
assert_contains "$out" 'rust-ai-lean outline' "full: outline line"
assert_contains "$out" 'LSP tool' "full: LSP line"
assert_contains "$out" 'crate-src' "full: crate-src line"
assert_not_contains "$out" 'Setup incomplete' "full: no setup warning"

# 3. CLI missing, LSP ok.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-ra:$base_path")
assert_not_contains "$out" 'rust-ai-lean outline' "no-cli: no outline line"
assert_contains "$out" 'LSP tool' "no-cli: LSP line"
assert_contains "$out" 'Setup incomplete (CLI missing)' "no-cli: setup warning"

# 4. LSP unavailable, CLI ok.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-nora:$tmp/bin-cli:$base_path")
assert_not_contains "$out" 'LSP tool' "no-ra: no LSP line"
assert_contains "$out" 'Setup incomplete (LSP unavailable)' "no-ra: setup warning"

# 5. Nothing available.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-nora:$base_path")
assert_contains "$out" 'Setup incomplete (LSP unavailable, CLI missing)' "none: both missing"

# 6. SubagentStart: no LSP line, no env write.
: >"$tmp/envfile"
out=$(run_hook SubagentStart "$tmp/rustproj" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_contains "$out" '"hookEventName":"SubagentStart"' "subagent: event name"
assert_not_contains "$out" 'LSP tool' "subagent: no LSP line"
assert_eq "$(cat "$tmp/envfile")" "" "subagent: env file untouched"

# 7. Multi-repo workspace root detected via child Cargo.toml.
out=$(run_hook SessionStart "$tmp/multi" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
assert_contains "$out" '[rust-ai-lean]' "multi-repo root detected"

# 8. Explicit CARGO_TERM_QUIET is respected.
: >"$tmp/envfile"
env -i PATH="$base_path" HOME="$tmp" CLAUDE_PROJECT_DIR="$tmp/plain" CLAUDE_ENV_FILE="$tmp/envfile" \
  CARGO_TERM_QUIET=false sh "$hook" SessionStart </dev/null >/dev/null
assert_eq "$(cat "$tmp/envfile")" "" "user CARGO_TERM_QUIET respected"

# 9. Size cap: additionalContext value <= 700 bytes in the largest state.
out=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-ra:$tmp/bin-cli:$base_path")
value=${out#*'"additionalContext":"'}
value=${value%'"}}'}
size=$(printf '%s' "$value" | wc -c | tr -d ' ')
[ "$size" -le 700 ] || { echo "FAIL size cap: $size bytes"; fails=$((fails + 1)); }
worst=$(run_hook SessionStart "$tmp/rustproj" "$tmp/bin-nora:$base_path")
value=${worst#*'"additionalContext":"'}
value=${value%'"}}'}
size=$(printf '%s' "$value" | wc -c | tr -d ' ')
[ "$size" -le 700 ] || { echo "FAIL size cap (none): $size bytes"; fails=$((fails + 1)); }

# 10. Unknown event exits 0 with no output.
out=$(run_hook Bogus "$tmp/rustproj" "$tmp/bin-ra:$base_path")
assert_eq "$out" "" "unknown event silent"

finish session-start
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `sh tests/shell/run.sh dash`
Expected: FAIL. Many assertions report empty output, because `plugin/hooks/session-start.sh` does not exist.

- [ ] **Step 3: Implement**

`plugin/scripts/lib.sh`:

```sh
# Shared helpers for rust-ai-lean shell scripts. POSIX sh; source, do not execute.

# Print the rust-analyzer binary the LSP launcher would run; return 1 if none.
# Keep in sync with the launcher in plugin/.claude-plugin/plugin.json.
ral_resolve_ra() {
  if command -v rustup >/dev/null 2>&1; then
    rustup which --toolchain stable rust-analyzer 2>/dev/null
  else
    command -v rust-analyzer 2>/dev/null
  fi
}

# Succeed if $1 is inside a Cargo project (Cargo.toml in it or an ancestor up
# to the git root) or has a Cargo.toml within two levels below it.
ral_is_rust_project() {
  ral_dir=$1
  ral_d=$ral_dir
  while :; do
    [ -f "$ral_d/Cargo.toml" ] && return 0
    [ -e "$ral_d/.git" ] && break
    ral_parent=$(dirname -- "$ral_d")
    [ "$ral_parent" = "$ral_d" ] && break
    ral_d=$ral_parent
  done
  for ral_f in "$ral_dir"/*/Cargo.toml "$ral_dir"/*/*/Cargo.toml; do
    [ -f "$ral_f" ] || continue
    case $ral_f in
      */target/*) continue ;;
    esac
    return 0
  done
  return 1
}
```

`plugin/hooks/session-start.sh`:

```sh
#!/bin/sh
# rust-ai-lean SessionStart/SubagentStart hook.
# Usage: session-start.sh <SessionStart|SubagentStart>
# Must never break a session: every failure path exits 0 silently.

event=${1:-SessionStart}
case $event in
  SessionStart | SubagentStart) ;;
  *) exit 0 ;;
esac

plugin_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." 2>/dev/null && pwd) || exit 0
# `.` on a missing file aborts a POSIX shell, so test for it first.
[ -f "$plugin_root/scripts/lib.sh" ] || exit 0
# shellcheck source=../scripts/lib.sh
. "$plugin_root/scripts/lib.sh"

if [ "$event" = SessionStart ] && [ -n "${CLAUDE_ENV_FILE:-}" ] && [ -z "${CARGO_TERM_QUIET+set}" ]; then
  printf 'export CARGO_TERM_QUIET=true\n' >>"$CLAUDE_ENV_FILE" 2>/dev/null || :
fi

project=${CLAUDE_PROJECT_DIR:-$PWD}
ral_is_rust_project "$project" || exit 0

ra_ok=false
ral_resolve_ra >/dev/null 2>&1 && ra_ok=true
cli_ok=false
command -v rust-ai-lean >/dev/null 2>&1 && cli_ok=true

# Lines are joined with a literal backslash-n so the value is valid JSON.
# Keep every line free of double quotes and backslashes.
nl='\n'
hint='[rust-ai-lean] Rust project: keep context small.'
if $cli_ok; then
  hint="$hint${nl}Outline before reading: rust-ai-lean outline <file|dir> gives signatures + line ranges; Read only those ranges, never whole large .rs files."
fi
if $ra_ok && [ "$event" = SessionStart ]; then
  hint="$hint${nl}Semantic lookups: LSP tool (goToDefinition, findReferences, hover, incomingCalls); load it with ToolSearch query select:LSP if deferred."
fi
if $cli_ok; then
  hint="$hint${nl}Crate APIs: rust-ai-lean crate-src <crate> = exact Cargo.lock version source. Do not use Context7/web docs for Rust crate APIs."
  hint="$hint${nl}Compile errors: rust-ai-lean diag check -p <pkg>. Never --message-format=short."
fi
missing=
$ra_ok || missing='LSP unavailable'
if ! $cli_ok; then
  missing="${missing:+$missing, }CLI missing"
fi
if [ -n "$missing" ]; then
  hint="$hint${nl}Setup incomplete ($missing): ask the user to run /rust-ai-lean:doctor."
fi
hint="$hint${nl}Guide: skill rust-ai-lean:rust-lean."

printf '{"hookSpecificOutput":{"hookEventName":"%s","additionalContext":"%s"}}\n' "$event" "$hint"
exit 0
```

`plugin/hooks/hooks.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh\" SessionStart",
            "timeout": 10
          }
        ]
      }
    ],
    "SubagentStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh\" SubagentStart",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

**Variant if S2 was REFUTED:** delete the `SubagentStart` array and its preceding comma from `hooks.json`, and delete the block `# 6. SubagentStart: ...` through the line before `# 7.` in `tests/shell/session-start.test.sh`.

**Variant if S5 was REFUTED:** in `session-start.sh`, insert these lines immediately before `hint="$hint${nl}Guide: skill rust-ai-lean:rust-lean."`:

```sh
if [ "$event" = SubagentStart ]; then
  hint="$hint${nl}Cargo: pass -q to cargo commands."
fi
```

Also add this assertion to test block 6 in `tests/shell/session-start.test.sh`:

```sh
assert_contains "$out" 'Cargo: pass -q' "subagent: quiet cargo reminder"
```

- [ ] **Step 4: Run the tests and checks**

```sh
chmod +x plugin/hooks/session-start.sh tests/shell/run.sh tests/shell/*.test.sh
sh tests/shell/run.sh dash
sh tests/shell/run.sh bash
shellcheck -x -P SCRIPTDIR -s sh plugin/scripts/lib.sh plugin/hooks/session-start.sh tests/shell/*.sh
claude plugin validate plugin
```

Expected: `ok: session-start` under both shells; shellcheck prints nothing; validation passes.

- [ ] **Step 5: Commit**

```sh
git add plugin/scripts/lib.sh plugin/hooks tests/shell
git commit -m "Add session hook that quiets cargo and injects a Rust hint"
```

---

### Task 11: rust-analyzer LSP launcher in the plugin manifest

**Files:**
- Modify: `plugin/.claude-plugin/plugin.json` (full replacement), `tests/plugin.rs` (full replacement)

**Interfaces:**
- Consumes: `read_json` from Task 6.
- Produces: `lspServers.rust-analyzer`, whose `args[1]` is the launcher script. It must stay equivalent to `ral_resolve_ra` in `plugin/scripts/lib.sh`.

- [ ] **Step 1: Write the failing launcher tests**

`tests/plugin.rs` (replace the whole file):

```rust
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn read_json(rel: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn versions_match_across_cargo_plugin_and_marketplace() {
    let cargo = env!("CARGO_PKG_VERSION");
    let plugin = read_json("plugin/.claude-plugin/plugin.json");
    let market = read_json(".claude-plugin/marketplace.json");
    assert_eq!(plugin["version"], cargo, "plugin.json version");
    let entry = market["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "rust-ai-lean")
        .expect("marketplace entry");
    assert_eq!(entry["version"], cargo, "marketplace.json version");
}

fn launcher_script() -> String {
    let plugin = read_json("plugin/.claude-plugin/plugin.json");
    let server = &plugin["lspServers"]["rust-analyzer"];
    assert_eq!(server["command"], "sh");
    assert_eq!(server["args"][0], "-c");
    server["args"][1].as_str().unwrap().to_string()
}

fn stub(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn launch(path: &str) -> (Option<i32>, String) {
    let out = Command::new("/bin/sh")
        .args(["-c", &launcher_script(), "ra-launch"])
        .env_clear()
        .env("PATH", path)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn launcher_prefers_rustup_stable_rust_analyzer() {
    let tmp = tempfile::tempdir().unwrap();
    let ra = tmp.path().join("toolchain");
    stub(&ra, "rust-analyzer", "echo stable-ra");
    let bin = tmp.path().join("bin");
    stub(
        &bin,
        "rustup",
        &format!(
            "[ \"$*\" = 'which --toolchain stable rust-analyzer' ] && echo {}/rust-analyzer",
            ra.display()
        ),
    );
    stub(&bin, "rust-analyzer", "echo path-ra");
    let (code, out) = launch(&format!("{}:/usr/bin:/bin", bin.display()));
    assert_eq!((code, out.trim()), (Some(0), "stable-ra"));
}

#[test]
fn launcher_fails_when_rustup_lacks_component() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    stub(&bin, "rustup", "exit 1");
    stub(&bin, "rust-analyzer", "echo proxy-must-not-run");
    let (code, out) = launch(&format!("{}:/usr/bin:/bin", bin.display()));
    assert_eq!(code, Some(1));
    assert!(!out.contains("proxy-must-not-run"));
}

#[test]
fn launcher_uses_path_without_rustup() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    stub(&bin, "rust-analyzer", "echo path-ra");
    let (code, out) = launch(&format!("{}:/usr/bin:/bin", bin.display()));
    assert_eq!((code, out.trim()), (Some(0), "path-ra"));
}
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test --test plugin`
Expected: the 3 launcher tests FAIL at `assert_eq!(server["command"], "sh")` (the value is `null`); the version test passes.

- [ ] **Step 3: Add the LSP server**

`plugin/.claude-plugin/plugin.json` (replace the whole file):

```json
{
  "name": "rust-ai-lean",
  "version": "0.1.0",
  "description": "Spend fewer tokens on Rust: working rust-analyzer LSP, quiet cargo, outline/crate-src/diag tools, and a doctor that checks it all",
  "author": { "name": "enoqv" },
  "homepage": "https://github.com/enoqv/rust-ai-lean",
  "license": "MIT OR Apache-2.0",
  "lspServers": {
    "rust-analyzer": {
      "command": "sh",
      "args": [
        "-c",
        "if command -v rustup >/dev/null 2>&1; then ra=$(rustup which --toolchain stable rust-analyzer) || exit 1; else ra=$(command -v rust-analyzer) || exit 1; fi; exec \"$ra\"",
        "ra-launch"
      ],
      "extensionToLanguage": { ".rs": "rust" },
      "initializationOptions": {
        "cargo": { "targetDir": true },
        "checkOnSave": false
      }
    }
  }
}
```

- [ ] **Step 4: Run the tests and checks**

```sh
cargo test --test plugin
cargo clippy --all-targets -- -D warnings
cargo fmt --check
claude plugin validate plugin
```

Expected: 4 tests pass; checks are clean; validation passes.

- [ ] **Step 5: Commit**

```sh
git add plugin/.claude-plugin/plugin.json tests/plugin.rs
git commit -m "Add rust-analyzer LSP launcher that uses the stable toolchain"
```

---

### Task 12: Doctor static checks

**Files:**
- Create: `plugin/skills/doctor/scripts/doctor.sh`, `tests/shell/doctor.test.sh`

**Interfaces:**
- Consumes: `ral_resolve_ra` and `ral_is_rust_project` from Task 10.
- Produces: `doctor.sh [--plugin-root DIR] [--project DIR]` prints `STATUS<TAB>id<TAB>detail<TAB>fix` rows and always exits 0.
  - Normal ids, in order: `toolchain.rustup`, `toolchain.ra`, `project.detected`, `toolchain.rust-src`, `cli.installed`, `cli.version`, `env.quiet`, `project.lock`, `project.crate-src`.
  - `plugin.lib` is reported alone when `lib.sh` is missing.

- [ ] **Step 1: Write the failing tests**

`tests/shell/doctor.test.sh`:

```sh
#!/bin/sh
# Tests for plugin/skills/doctor/scripts/doctor.sh. Run via tests/shell/run.sh.
set -u
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH='' cd -- "$here/../.." && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"
doctor="$repo/plugin/skills/doctor/scripts/doctor.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/plain"
base_path=/usr/bin:/bin
version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$repo/plugin/.claude-plugin/plugin.json" | head -n 1)

make_stub "$tmp/ra-ok" rust-analyzer 'echo "rust-analyzer 1.0.0-stub"'
make_stub "$tmp/bin-ok" rustup "case \"\$1 \$2\" in 'which --toolchain') echo $tmp/ra-ok/rust-analyzer ;; *) exit 1 ;; esac"
make_stub "$tmp/bin-ok" rust-ai-lean "echo rust-ai-lean $version"
make_stub "$tmp/bin-bad" rustup 'exit 1'
make_stub "$tmp/bin-bad" rust-ai-lean 'echo rust-ai-lean 0.0.1'

run_doctor() { # path project [VAR=value...]
  p=$1; proj=$2; shift 2
  env -i PATH="$p" HOME="$tmp" "$@" sh "$doctor" --plugin-root "$repo/plugin" --project "$proj" </dev/null
}

status_of() { # output id
  printf '%s\n' "$1" | awk -F '\t' -v id="$2" '$2 == id { print $1 }'
}

fix_of() { # output id
  printf '%s\n' "$1" | awk -F '\t' -v id="$2" '$2 == id { print $4 }'
}

# Healthy toolchain, non-Rust directory.
out=$(run_doctor "$tmp/bin-ok:$base_path" "$tmp/plain" CARGO_TERM_QUIET=true)
for id in toolchain.rustup toolchain.ra project.detected toolchain.rust-src cli.installed cli.version env.quiet project.lock project.crate-src; do
  count=$(printf '%s\n' "$out" | awk -F '\t' -v id="$id" '$2 == id' | wc -l | tr -d ' ')
  assert_eq "$count" 1 "healthy: $id reported once"
done
assert_eq "$(status_of "$out" toolchain.ra)" OK "healthy: ra ok"
assert_eq "$(status_of "$out" cli.version)" OK "healthy: version ok"
assert_eq "$(status_of "$out" env.quiet)" OK "healthy: env ok"
assert_eq "$(status_of "$out" project.detected)" INFO "healthy: plain dir is not rust"

# Broken toolchain: component missing, CLI version mismatch, env unset.
out=$(run_doctor "$tmp/bin-bad:$base_path" "$tmp/plain")
assert_eq "$(status_of "$out" toolchain.ra)" FAIL "broken: ra fail"
assert_eq "$(fix_of "$out" toolchain.ra)" "rustup component add rust-analyzer --toolchain stable" "broken: ra fix"
assert_eq "$(status_of "$out" cli.version)" WARN "broken: version mismatch"
assert_eq "$(fix_of "$out" cli.version)" "cargo install --git https://github.com/enoqv/rust-ai-lean --tag v$version --locked --force" "broken: version fix"
assert_eq "$(status_of "$out" env.quiet)" WARN "broken: env unset"

# No rustup, no CLI.
out=$(run_doctor "$base_path" "$tmp/plain" CARGO_TERM_QUIET=false)
assert_eq "$(status_of "$out" toolchain.rustup)" WARN "bare: rustup warn"
assert_eq "$(status_of "$out" cli.installed)" WARN "bare: cli missing"
assert_eq "$(fix_of "$out" cli.installed)" "cargo install --git https://github.com/enoqv/rust-ai-lean --tag v$version --locked" "bare: install fix"
assert_eq "$(status_of "$out" env.quiet)" INFO "bare: user value respected"

# Missing plugin root never crashes.
out=$(env -i PATH="$base_path" sh "$doctor" --plugin-root "$tmp/nowhere" --project "$tmp/plain" </dev/null)
assert_eq "$(status_of "$out" plugin.lib)" FAIL "missing lib reported"

finish doctor
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `sh tests/shell/run.sh dash`
Expected: `doctor` reports failures, because the script does not exist; `ok: session-start` still passes.

- [ ] **Step 3: Implement**

`plugin/skills/doctor/scripts/doctor.sh`:

```sh
#!/bin/sh
# rust-ai-lean doctor: static environment checks.
# Usage: doctor.sh [--plugin-root DIR] [--project DIR]
# Output: one line per check, STATUS<TAB>id<TAB>message<TAB>fix. Always exits 0.

REPO_URL=https://github.com/enoqv/rust-ai-lean
plugin_root=
project=$PWD
while [ $# -gt 0 ]; do
  case $1 in
    --plugin-root) plugin_root=${2:-}; shift 2 || shift ;;
    --project) project=${2:-}; shift 2 || shift ;;
    *) shift ;;
  esac
done
[ -n "$plugin_root" ] || plugin_root=$(CDPATH='' cd -- "$(dirname -- "$0")/../../.." && pwd)

report() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "${4:-}"; }

# `.` on a missing file aborts a POSIX shell, so test for it first.
if [ ! -f "$plugin_root/scripts/lib.sh" ]; then
  report FAIL plugin.lib "cannot load $plugin_root/scripts/lib.sh" ""
  exit 0
fi
# shellcheck source=../../../scripts/lib.sh
. "$plugin_root/scripts/lib.sh"

plugin_version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
  "$plugin_root/.claude-plugin/plugin.json" 2>/dev/null | head -n 1)
install_cmd="cargo install --git $REPO_URL --tag v$plugin_version --locked"

# Never let rustup download a toolchain as a side effect of a check.
RUSTUP_AUTO_INSTALL=0
export RUSTUP_AUTO_INSTALL

check_rustup() {
  if command -v rustup >/dev/null 2>&1; then
    report OK toolchain.rustup "rustup found"
    has_rustup=true
  else
    report WARN toolchain.rustup "rustup not found; the LSP launcher will use rust-analyzer from PATH"
    has_rustup=false
  fi
}

check_ra() {
  ra=$(ral_resolve_ra) || ra=
  if [ -n "$ra" ] && ra_version=$("$ra" --version 2>/dev/null); then
    report OK toolchain.ra "$ra_version ($ra)"
  elif $has_rustup; then
    report FAIL toolchain.ra "rust-analyzer is not installed for the stable toolchain" \
      "rustup component add rust-analyzer --toolchain stable"
  else
    report FAIL toolchain.ra "rust-analyzer not found on PATH; see https://rust-analyzer.github.io/book/installation.html"
  fi
}

check_project_detected() {
  if ral_is_rust_project "$project"; then
    report OK project.detected "Rust project: $project"
    is_rust=true
  else
    report INFO project.detected "not a Rust project: $project"
    is_rust=false
  fi
}

check_rust_src() {
  if ! $is_rust || ! $has_rustup; then
    report INFO toolchain.rust-src "skipped"
    return
  fi
  toolchain=$(cd "$project" 2>/dev/null && rustup show active-toolchain 2>/dev/null | awk 'NR == 1 { print $1 }')
  sysroot=$(cd "$project" 2>/dev/null && rustc --print sysroot 2>/dev/null)
  if [ -z "$toolchain" ] || [ -z "$sysroot" ]; then
    report WARN toolchain.rust-src "check failed: cannot determine the project toolchain (is it installed?)"
  elif [ -d "$sysroot/lib/rustlib/src/rust/library" ]; then
    report OK toolchain.rust-src "rust-src present for $toolchain"
  else
    report WARN toolchain.rust-src "rust-src missing for $toolchain; std types resolve poorly in rust-analyzer" \
      "rustup component add rust-src --toolchain $toolchain"
  fi
}

check_cli() {
  if ! command -v rust-ai-lean >/dev/null 2>&1; then
    report WARN cli.installed "rust-ai-lean CLI not on PATH" "$install_cmd"
    report INFO cli.version "skipped"
    cli_ok=false
    return
  fi
  cli_ok=true
  report OK cli.installed "$(command -v rust-ai-lean)"
  cli_version=$(rust-ai-lean --version 2>/dev/null | awk '{ print $2 }')
  if [ "$cli_version" = "$plugin_version" ]; then
    report OK cli.version "$cli_version"
  else
    report WARN cli.version "CLI $cli_version does not match plugin $plugin_version" "$install_cmd --force"
  fi
}

check_env() {
  if [ "${CARGO_TERM_QUIET:-}" = true ]; then
    report OK env.quiet "CARGO_TERM_QUIET=true"
  elif [ -n "${CARGO_TERM_QUIET+set}" ]; then
    report INFO env.quiet "CARGO_TERM_QUIET=$CARGO_TERM_QUIET set by the user; respected"
  else
    report WARN env.quiet "CARGO_TERM_QUIET not set; this session started before the plugin hook ran" \
      "start a new Claude Code session"
  fi
}

check_lock_and_crate_src() {
  if ! $is_rust; then
    report INFO project.lock "skipped"
    report INFO project.crate-src "skipped"
    return
  fi
  manifest=$(cd "$project" 2>/dev/null && cargo locate-project --workspace --message-format plain 2>/dev/null)
  if [ -z "$manifest" ]; then
    report INFO project.lock "skipped: $project is not inside a Cargo package; run doctor from a crate directory"
    report INFO project.crate-src "skipped"
    return
  fi
  ws_root=$(dirname -- "$manifest")
  if [ -f "$ws_root/Cargo.lock" ]; then
    report OK project.lock "$ws_root/Cargo.lock"
  else
    report WARN project.lock "no Cargo.lock in $ws_root; crate-src needs one (never generated automatically)"
    report INFO project.crate-src "skipped"
    return
  fi
  if ! $cli_ok; then
    report INFO project.crate-src "skipped: CLI not installed"
    return
  fi
  dep=$(awk '
    /^\[(workspace\.)?dependencies\]/ { in_deps = 1; next }
    /^\[/ { in_deps = 0 }
    in_deps && /^[A-Za-z0-9_-]+[[:space:]]*=/ { sub(/[[:space:]]*=.*/, ""); print; exit }
  ' "$manifest")
  if [ -z "$dep" ]; then
    report INFO project.crate-src "skipped: no direct dependencies in $manifest"
    return
  fi
  if resolved=$(cd "$project" && rust-ai-lean crate-src "$dep" --path-only 2>&1); then
    report OK project.crate-src "$dep -> $(printf '%s\n' "$resolved" | head -n 1)"
  else
    report WARN project.crate-src "crate-src $dep failed: $(printf '%s\n' "$resolved" | head -n 1)"
  fi
}

has_rustup=false
is_rust=false
cli_ok=false
check_rustup
check_ra
check_project_detected
check_rust_src
check_cli
check_env
check_lock_and_crate_src
exit 0
```

- [ ] **Step 4: Run the tests, checks, and a real-machine smoke run**

```sh
chmod +x plugin/skills/doctor/scripts/doctor.sh tests/shell/doctor.test.sh
sh tests/shell/run.sh dash
sh tests/shell/run.sh bash
shellcheck -x -P SCRIPTDIR -s sh plugin/skills/doctor/scripts/doctor.sh tests/shell/*.sh
sh plugin/skills/doctor/scripts/doctor.sh --project tests/fixtures/deps
```

Expected: `ok: doctor` and `ok: session-start` under both shells; shellcheck prints nothing. The smoke run prints exactly 9 tab-separated rows that reflect this machine; any FAIL/WARN rows must carry a correct fix.

- [ ] **Step 5: Commit**

```sh
git add plugin/skills/doctor/scripts/doctor.sh tests/shell/doctor.test.sh
git commit -m "Add doctor static checks"
```

---

### Task 13: Doctor skill

**Files:**
- Create: `plugin/skills/doctor/SKILL.md`

**Interfaces:**
- Consumes: `doctor.sh` output contract from Task 12.
- Produces: the user command `/rust-ai-lean:doctor`, plus live-check ids `plugin.official-lsp`, `plugin.enabled`, `plugin.list`, `info.context7`, `lsp.tool`, `lsp.document-symbol`, `lsp.hover`, `lsp.processes`, `hook.context`.

- [ ] **Step 1: Write the skill**

`plugin/skills/doctor/SKILL.md`:

````markdown
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
````

- [ ] **Step 2: Verify**

```sh
claude plugin validate plugin
wc -w plugin/skills/doctor/SKILL.md
```

Expected: validation passes; about 600 words. The behavioral check happens in Task 17.

- [ ] **Step 3: Commit**

```sh
git add plugin/skills/doctor/SKILL.md
git commit -m "Add doctor skill"
```

---

### Task 14: Usage skill

**Files:**
- Create: `plugin/skills/rust-lean/SKILL.md`

- [ ] **Step 1: Write the skill**

`plugin/skills/rust-lean/SKILL.md`. If S3a was REFUTED, omit the `paths:` line.

```markdown
---
name: rust-lean
description: Use when working in a Rust or Cargo project - reading or navigating .rs code, looking up a dependency's API, fixing compile errors, or running cargo build, check, test, or clippy. Keeps token usage low without losing compiler detail.
paths: ["**/*.rs", "**/Cargo.toml"]
---

# Token-lean Rust workflow

## Navigate before reading

1. `rust-ai-lean outline <file|dir>` prints signatures with `start-end` line ranges and no bodies.
2. Use the LSP tool for semantics: `documentSymbol`, `goToDefinition`, `findReferences`, `hover`, `incomingCalls`, `outgoingCalls`. If it is deferred, load it with ToolSearch query `select:LSP`. Positions are 1-based; use a range's start line and the column of the item name.
3. Read only what you need: `Read` with `offset` set to the range start and `limit` set to the range length.

- Do not `Read` or `cat` whole `.rs` files longer than about 200 lines, and do not page through files with `sed -n`.
- `grep -n` and `rg -n` are for locating text, not for reading it.
- `outline` cannot see items generated by macros (derive output, `macro_rules!` expansions, code generated by sqlx or serde). Use LSP or grep for those.

## Dependency APIs

1. `rust-ai-lean crate-src <crate>` prints the exact version from Cargo.lock, its enabled features, and its source directory.
2. Inspect that directory with `rust-ai-lean outline <dir>/src/<module>.rs` or `rg -n 'pub fn <name>' <dir>/src`.

- Never write code against an API recalled from memory or from another version.
- Do not use Context7 or web docs for Rust crate APIs. They track the latest release, not your lockfile, and can return APIs that do not exist.
- After `cargo add`, read the `Cargo.toml` diff to see which version was added; quiet cargo output does not print it.

## Compile and fix

- Run `rust-ai-lean diag check -p <package>`. Errors come first with their full `help:` and `note:` text; warnings are one line each.
- Never pass `--message-format=short`. It drops the suggestions that make fixes one-shot.
- Fix errors first, then warnings. Before finishing, run `rust-ai-lean diag clippy -p <package>`; add `--full-warnings` when you are fixing clippy suggestions.
- Cargo waiting on a build lock is not hung. Do not kill it.

## Tests

- Scope every run: `cargo test -p <package> <test name filter>`.
- With `CARGO_TERM_QUIET=true` (set by this plugin), passing tests print as dots and failures print in full.
- After a failure, re-run only the failing test. Long output from a failing command loses its middle, so keep it short.

## Without the LSP tool

Subagents may not have the LSP tool. Use `outline`, `crate-src`, and `rg -n` instead.
```

- [ ] **Step 2: Verify**

```sh
claude plugin validate plugin
wc -w plugin/skills/rust-lean/SKILL.md
```

Expected: validation passes; at most 600 words.

- [ ] **Step 3: Commit**

```sh
git add plugin/skills/rust-lean/SKILL.md
git commit -m "Add rust-lean usage skill"
```

---

### Task 15: Benchmark script and README

**Files:**
- Create: `scripts/bench.sh`, `README.md`

- [ ] **Step 1: Write the benchmark**

`scripts/bench.sh`:

```sh
#!/bin/sh
# shellcheck disable=SC2016 # backticks are Markdown, not command substitution
# Reproduce the README token table from this repository's own fixtures.
# Usage: scripts/bench.sh   (run from the repository root; needs cargo)
set -eu

repo=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export CARGO_TARGET_DIR="$work/target" CARGO_TERM_COLOR=never

cargo build --quiet --manifest-path "$repo/Cargo.toml"
bin="$CARGO_TARGET_DIR/debug/rust-ai-lean"

chars() { wc -c | tr -d ' '; }
pct() { echo "$(( $2 * 100 / $1 ))%"; }
row() { printf '| %s | %s | %s chars | %s |\n' "$1" "$2" "$3" "$4"; }

# 300 passing tests and 1 failing test, generated so the fixture stays small.
mkdir -p "$work/tests-crate/src"
printf '[package]\nname = "bench-tests"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n' >"$work/tests-crate/Cargo.toml"
{
  echo 'pub fn add(a: u64, b: u64) -> u64 { a + b }'
  echo '#[cfg(test)] mod tests {'
  i=0
  while [ $i -lt 300 ]; do
    echo "  #[test] fn case_$i() { assert_eq!(super::add($i, 1), $i + 1); }"
    i=$((i + 1))
  done
  echo '  #[test] fn case_failing() { assert_eq!(super::add(2, 2), 5, "deliberate failure"); }'
  echo '}'
} >"$work/tests-crate/src/lib.rs"

cd "$work/tests-crate"
cargo test --quiet --no-run 2>/dev/null
t_default=$(cargo test 2>&1 | chars || true)
t_quiet=$(CARGO_TERM_QUIET=true cargo test 2>&1 | chars || true)

cd "$repo/tests/fixtures/diag-noisy"
cargo check --quiet 2>/dev/null || true
c_human=$(cargo check 2>&1 | chars || true)
c_short=$(cargo check --message-format=short 2>&1 | chars || true)
c_diag=$("$bin" diag check 2>&1 | chars || true)

o_cat=$(cat -n "$repo/src/outline.rs" | chars)
o_outline=$("$bin" outline "$repo/src/outline.rs" | chars)

echo '| Scenario | Variant | Output | vs. baseline |'
echo '|---|---|---|---|'
row '`cargo test`, 300 pass + 1 fail' 'default' "$t_default" '100%'
row '' '`CARGO_TERM_QUIET=true`' "$t_quiet" "$(pct "$t_default" "$t_quiet")"
row '`cargo check`, 2 errors + 25 warnings' 'default' "$c_human" '100%'
row '' '`--message-format=short` (drops help/notes)' "$c_short" "$(pct "$c_human" "$c_short")"
row '' '`rust-ai-lean diag check`' "$c_diag" "$(pct "$c_human" "$c_diag")"
row 'Read `src/outline.rs`' '`cat -n`' "$o_cat" '100%'
row '' '`rust-ai-lean outline`' "$o_outline" "$(pct "$o_cat" "$o_outline")"
```

- [ ] **Step 2: Run it**

```sh
chmod +x scripts/bench.sh
shellcheck -s sh scripts/bench.sh
dash scripts/bench.sh | tee "$(mktemp -d)/bench.md"
```

Expected: a 7-row Markdown table. Check the acceptance thresholds:
- the `CARGO_TERM_QUIET=true` row is ≤ 10%;
- the `rust-ai-lean outline` row is ≤ 20%;
- `rust-ai-lean diag check` is below the default.

If a threshold fails, stop and report.

- [ ] **Step 3: Write the README**

`README.md`. Replace the table with the table printed in Step 2:

````markdown
# rust-ai-lean

Spend fewer tokens when an AI coding agent works on Rust. rust-ai-lean is a [Claude Code](https://code.claude.com) plugin plus a small CLI:

- **A rust-analyzer LSP server that actually starts.** The launcher always uses the stable toolchain's rust-analyzer, so projects with a pinned `rust-toolchain.toml` work without per-toolchain setup.
- **Quiet cargo.** `CARGO_TERM_QUIET=true` for every session: no `Compiling` lines, dots instead of one line per passing test, failures in full.
- **Read less code.** `rust-ai-lean outline` prints signatures with line ranges, `crate-src` finds a dependency's source for the exact locked version, and `diag` prints compiler errors in full with warnings one line each.
- **Steering.** A short session hint and a usage skill point the agent at these tools.
- **`/rust-ai-lean:doctor`** checks all of the above and offers each fix.

## Measured on this repository

Output of `scripts/bench.sh` (characters the agent would read):

| Scenario | Variant | Output | vs. baseline |
|---|---|---|---|
| `cargo test`, 300 pass + 1 fail | default | 8971 chars | 100% |
|  | `CARGO_TERM_QUIET=true` | 835 chars | 9% |
| `cargo check`, 2 errors + 25 warnings | default | 6537 chars | 100% |
|  | `--message-format=short` (drops help/notes) | 3675 chars | 56% |
|  | `rust-ai-lean diag check` | 2440 chars | 37% |
| Read `src/outline.rs` | `cat -n` | 17247 chars | 100% |
|  | `rust-ai-lean outline` | 1362 chars | 7% |

`diag` keeps every error's `help:` and `note:` lines; `--message-format=short` does not.

## Install

Requirements: Linux or macOS, `rustup` (recommended) or `rust-analyzer` on `PATH`, and `cargo`.

```sh
claude plugin marketplace add enoqv/rust-ai-lean
claude plugin install rust-ai-lean@rust-ai-lean
```

Start a new Claude Code session in a Rust project and run:

```
/rust-ai-lean:doctor
```

Doctor reports what is missing and asks before running each fix. Typical fixes:

- `rustup component add rust-analyzer --toolchain stable`
- `cargo install --git https://github.com/enoqv/rust-ai-lean --tag v0.1.0 --locked`
- `claude plugin disable rust-analyzer-lsp@claude-plugins-official` (otherwise two rust-analyzer servers are registered for `.rs`)

## CLI

```sh
rust-ai-lean outline src/            # signatures + line ranges, no bodies
rust-ai-lean outline --docs src/lib.rs
rust-ai-lean crate-src serde         # exact locked version, features, source dir
rust-ai-lean crate-src serde --path-only
rust-ai-lean diag check -p my-crate  # errors in full, warnings one line each
rust-ai-lean diag clippy --full-warnings -- --all-targets
```

## Configuration

- **Keep cargo's normal output:** set `CARGO_TERM_QUIET=false` in the environment you start Claude Code from. The plugin never overrides a value you set.
- **rust-analyzer settings:** the plugin starts rust-analyzer with `cargo.targetDir = true` (its own target subdirectory, so it never blocks your builds) and `checkOnSave = false` (no workspace-wide `cargo check` after every edit).

RUST_ANALYZER_TOML_PARAGRAPH

## Limitations

- rust-analyzer runs one instance per workspace, and each can use a lot of memory. Doctor reports how many are running.
- `outline` does not see items generated by macros.
- Subagents may not receive the LSP tool; the usage skill tells them to use the CLI instead.
- Windows is not supported.

## Uninstall

```sh
claude plugin uninstall rust-ai-lean@rust-ai-lean
claude plugin marketplace remove rust-ai-lean
cargo uninstall rust-ai-lean
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
````

- [ ] **Step 4: Replace the `RUST_ANALYZER_TOML_PARAGRAPH` line according to spike S4**

If S4 showed that `rust-analyzer.toml` overrides the plugin's settings:

```markdown
To change these for one project, add a `rust-analyzer.toml` at the project root, for example `checkOnSave = true`; values there take precedence over the plugin's. For anything else, such as `numThreads`, install the plugin from a local clone (`claude plugin marketplace add /path/to/rust-ai-lean`) and add the setting to `initializationOptions` in `plugin/.claude-plugin/plugin.json`.
```

Otherwise, meaning plugin settings won or the result was INCONCLUSIVE:

```markdown
A project's `rust-analyzer.toml` does not override these. To change them, or to add settings such as `numThreads`, install the plugin from a local clone (`claude plugin marketplace add /path/to/rust-ai-lean`) and edit `initializationOptions` in `plugin/.claude-plugin/plugin.json`.
```

Then confirm that no marker remains:

```sh
grep -n 'RUST_ANALYZER_TOML_PARAGRAPH' README.md; test $? -eq 1 && echo clean
```

Expected: `clean`.

- [ ] **Step 5: Commit**

```sh
git add scripts/bench.sh README.md
git commit -m "Add benchmark script and README"
```

---

### Task 16: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the workflow**

`.github/workflows/ci.yml`. `actions/checkout` is pinned to the v7.0.1 commit:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false

      - name: Install Rust
        run: |
          rustup toolchain install stable --profile minimal --component clippy,rustfmt
          rustup default stable

      - name: Format
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings

      - name: Test
        run: cargo test

      - name: Shell tests
        run: sh tests/shell/run.sh sh

      - name: Shell tests under dash
        if: runner.os == 'Linux'
        run: sh tests/shell/run.sh dash

      - name: ShellCheck
        if: runner.os == 'Linux'
        run: shellcheck -x -P SCRIPTDIR -s sh plugin/scripts/lib.sh plugin/hooks/session-start.sh plugin/skills/doctor/scripts/doctor.sh scripts/bench.sh tests/shell/*.sh

      - name: No absolute home-directory paths
        if: runner.os == 'Linux'
        run: |
          if git grep -nE '/(home|Users)/[A-Za-z0-9_.-]+/' -- ':!.github/workflows/ci.yml'; then
            echo "Found an absolute home-directory path in tracked files."
            exit 1
          fi
```

- [ ] **Step 2: Run every CI step locally**

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
sh tests/shell/run.sh sh
sh tests/shell/run.sh dash
shellcheck -x -P SCRIPTDIR -s sh plugin/scripts/lib.sh plugin/hooks/session-start.sh plugin/skills/doctor/scripts/doctor.sh scripts/bench.sh tests/shell/*.sh
if git grep -nE '/(home|Users)/[A-Za-z0-9_.-]+/' -- ':!.github/workflows/ci.yml'; then echo "home path found"; else echo "no home paths"; fi
```

Expected: every step passes and the last line prints `no home paths`.

- [ ] **Step 3: Commit**

```sh
git add .github/workflows/ci.yml
git commit -m "Add CI for Linux and macOS"
```

---

### Task 17: End-to-end acceptance, hygiene review, and handoff

This task needs the user. A subagent cannot start new Claude Code sessions, so run it from the main session.

**Files:** none. Only fix-up commits are made here, if the steps below find problems.

- [ ] **Step 1: Full local gate**

Run all of Task 16 Step 2, then `dash scripts/bench.sh`. Expected: all pass, and the table matches the README within a few percent.

- [ ] **Step 2: Hygiene review of everything that will be published**

```sh
git log --format='%h %s' main..HEAD
git diff --stat main..HEAD
git grep -nE '/(home|Users)/[A-Za-z0-9_.-]+/' main..HEAD -- . ':!.github/workflows/ci.yml' || echo "no home paths"
```

Read the full diff (`git diff main..HEAD`) for machine-specific paths, user names, hostnames, email addresses, or content from private projects. Ask the user whether they want to run their own private-identifier check, which they keep outside this repository, before continuing.

- [ ] **Step 3: Install the local build**

```sh
cargo install --path . --locked
claude plugin marketplace add "$PWD"
claude plugin install rust-ai-lean@rust-ai-lean
```

Expected: the CLI is installed, and the plugin is installed from the local marketplace.

- [ ] **Step 4: Prepare acceptance criterion 1**

Criterion 1 needs a machine where the official `rust-analyzer-lsp` plugin is enabled and stable lacks rust-analyzer.
- Ask the user whether the official plugin is currently enabled. Do not change it yourself.
- If `rustup which --toolchain stable rust-analyzer` succeeds, ask the user for approval to run `rustup component remove rust-analyzer --toolchain stable` temporarily. If they decline, note that criterion 1 is verified only partially.

- [ ] **Step 5: Doctor run 1 (user)**

Ask the user to start a new Claude Code session in a Rust project that pins its toolchain with `rust-toolchain.toml`, then run `/rust-ai-lean:doctor`.

Expected:
- FAIL `toolchain.ra` with fix `rustup component add rust-analyzer --toolchain stable`;
- FAIL `plugin.official-lsp` with fix `claude plugin disable rust-analyzer-lsp@claude-plugins-official`, when that plugin is enabled;
- each fix is offered in its own question and runs only when approved.

- [ ] **Step 6: Doctor run 2 (user)**

After the approved fixes, ask the user to start another new session in the same project and run `/rust-ai-lean:doctor` again.

Expected:
- no FAIL rows;
- `lsp.document-symbol` OK and `lsp.hover` OK;
- `hook.context` OK;
- `env.quiet` OK.

- [ ] **Step 7: Hook latency (criterion 3)**

```sh
time (CLAUDE_PROJECT_DIR="$PWD" CLAUDE_ENV_FILE=/dev/null sh plugin/hooks/session-start.sh SessionStart >/dev/null)
```

Expected: `real` well under 1 second.

- [ ] **Step 8: Handoff**

Report the results of Steps 1–7 to the user. Then ask whether to:
- push `feat/v0.1.0` to `origin` and open a PR;
- after merge, create tag `v0.1.0`, which doctor's install command pins;
- remove the local marketplace (`claude plugin marketplace remove rust-ai-lean`) and reinstall from GitHub.

Do none of these without an explicit yes.
