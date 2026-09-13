# rust-ai-lean — Design

- **Date:** 2026-09-13
- **Status:** Draft, pending review
- **Scope:** A user-scope Claude Code plugin plus a companion Rust CLI that reduce the tokens an AI coding agent spends while working on Rust projects, and a single diagnostic skill that verifies the whole setup actually works.

## 1. Problem

When an agent works on a Rust codebase, most tokens do not go into the code it writes. They go into what it reads back:

1. **Reading code.** Locating a symbol usually means `grep`, then reading a whole file or large line ranges. Whole-file reads of big modules dominate tool output.
2. **Cargo output noise.** `Compiling …` status lines, one `test … ok` line per passing test, and warnings that bury the errors. In practice, the useful part of a failing `cargo test`/`cargo check` is often a minority of the output.
3. **Guessing crate APIs.** The agent guesses an API for the wrong version, fails to compile, then reads web docs or raw registry sources to recover.
4. **Semantic navigation is missing or silently broken.** Claude Code ships an LSP tool and an official `rust-analyzer-lsp` plugin, but the plugin runs `rust-analyzer` from `PATH`. With rustup that is a proxy that resolves the *project's* toolchain (for example a pinned `rust-toolchain.toml`). If that toolchain lacks the `rust-analyzer` component, the server crashes on first use and the agent quietly falls back to grep.

### Evidence collected during design

Public sources are listed in the appendix. Measurements below were taken on synthetic crates with Claude Code 2.1.270 and rustc 1.98.1, and will be re-created as reproducible fixtures (§8).

| Scenario | Variant | Output size |
|---|---|---|
| `cargo test`, 300 pass + 1 fail | default | 16,122 chars |
| | `CARGO_TERM_QUIET=true` | 860 chars; failure details intact |
| `cargo check`, 5 errors + 25 warnings | default human format | 8,808 chars |
| | `--message-format=short` | 3,873 chars; **drops `help:`/`note:`** such as the `Rc` not `Send` root cause and a `try_into()` suggestion |
| | errors only, full rendering | 2,769 chars |
| | `CARGO_BUILD_WARNINGS=allow` | 2,993 chars; errors intact; no rebuild triggered |
| | errors full + warnings one line each | 3,942 chars |

Other observations that shape the design:

- **Failed Bash commands are truncated in the middle and not saved.** A ~40 KB failing output kept its head and tail, and a 20 KB middle segment, including a marker planted there, was dropped with no file to recover it. Long cargo output can therefore hide the one error that matters. Successful commands above `bashOutputMaxChars` are saved to a file with a preview.
- **Claude Code sends `textDocument/didSave` after edits.** With rust-analyzer's default `checkOnSave: true`, every edit triggers a workspace-wide `cargo check`.
- **Context7 is unreliable for Rust crate APIs.** A query for `rust_decimal`'s `RoundingStrategy` returned, as its top snippet, variants and example code that do not exist in the crate (`Floor`, `Ceiling`, `RoundHalfEven`, `Truncate`, `Fixed(u32)`). The correct definition came second. The index tracks docs.rs *latest* only, so versions cannot be pinned.
- **Output-rewriting proxies can hide errors.** rtk 0.49.0 reproducibly omits a compile error from `rtk cargo test` when 20+ warnings precede it (rtk-ai/rtk#3762).
- Claude Code hooks support `PreToolUse` `updatedInput`, `SessionStart`/`SubagentStart` events, and `CLAUDE_ENV_FILE` for session environment variables.

## 2. Goals and non-goals

### Goals

- G1. Make semantic navigation through the LSP tool work reliably for any Rust project, including ones with pinned toolchains, without per-project setup.
- G2. Cut cargo output noise by default, without hiding compiler errors or their `help:`/`note:` suggestions.
- G3. Give the agent cheap, version-exact ways to outline code and look up dependency APIs.
- G4. Steer the agent toward these tools deterministically, rather than hoping a skill gets invoked.
- G5. One user-invoked command (`/rust-ai-lean:doctor`) that checks everything above, explains each failure, and offers each fix individually.
- G6. Linux and macOS.

### Non-goals

- Windows support.
- An MCP server. The tools are plain CLI commands usable from Bash in the main session and in subagents.
- Filtering `cargo test` beyond `CARGO_TERM_QUIET`, or wrapping arbitrary commands in an output proxy.
- A code knowledge graph or repo-wide index. rust-analyzer already provides references and call hierarchy with type-accurate resolution.
- Fixing rust-analyzer's one-instance-per-workspace memory cost. It is reported by doctor and documented, not solved.
- Tuning rust-analyzer resources such as thread counts per machine. Users adjust these with rust-analyzer's own `rust-analyzer.toml`.

## 3. Architecture

A single repository is at once a Claude Code marketplace, a plugin, and a Cargo package:

```
rust-ai-lean/
├── .claude-plugin/marketplace.json     # marketplace "rust-ai-lean"; plugin source ./plugin
├── plugin/
│   ├── .claude-plugin/plugin.json      # name, version, inline lspServers
│   ├── hooks/hooks.json                # SessionStart + SubagentStart
│   ├── hooks/session-start.sh          # POSIX sh
│   ├── scripts/lib.sh                  # shared detection helpers (POSIX sh)
│   └── skills/
│       ├── doctor/SKILL.md             # user-invoked; disable-model-invocation: true
│       ├── doctor/scripts/doctor.sh    # static checks (POSIX sh)
│       └── rust-lean/SKILL.md          # usage guide; model-invoked
├── Cargo.toml, src/                    # CLI binary `rust-ai-lean`
├── tests/                              # CLI integration tests + fixtures
├── scripts/bench.sh                    # reproduces the README measurements
├── .github/workflows/ci.yml
├── docs/superpowers/specs/
├── README.md
└── LICENSE-MIT, LICENSE-APACHE         # MIT OR Apache-2.0
```

**Split of responsibilities.**
- **POSIX sh** covers everything that must run before the CLI is installed: hooks, the LSP launcher, and doctor's static checks. No `jq`, `python3`, or bash-only syntax.
- **The Rust CLI** covers everything that needs real parsing: `syn` for outlines, `cargo metadata` for dependency sources, and cargo JSON diagnostics.

**Versioning.** `Cargo.toml`, `plugin/.claude-plugin/plugin.json`, and the marketplace entry share one version, enforced by a test. Releases are tagged `v<version>`. Doctor's install command pins `--tag v<plugin version>`, so the CLI always matches the installed plugin.

**Install flow** (documented in the README):

```
claude plugin marketplace add enoqv/rust-ai-lean
claude plugin install rust-ai-lean@rust-ai-lean
# start a new session, then:
/rust-ai-lean:doctor
```

Doctor then offers the remaining steps: installing the CLI, adding rustup components, and disabling the official `rust-analyzer-lsp` plugin.

## 4. Components

### 4.1 LSP server (in `plugin.json`)

```json
"lspServers": {
  "rust-analyzer": {
    "command": "sh",
    "args": ["-c", "if command -v rustup >/dev/null 2>&1; then ra=$(rustup which --toolchain stable rust-analyzer) || exit 1; else ra=$(command -v rust-analyzer) || exit 1; fi; exec \"$ra\"", "ra-launch"],
    "extensionToLanguage": { ".rs": "rust" },
    "initializationOptions": {
      "cargo": { "targetDir": true },
      "checkOnSave": false
    }
  }
}
```

**Binary resolution.**
- With rustup, the launcher always uses the **stable** toolchain's rust-analyzer. `rustup which` returns the real binary path, so the project's `rust-toolchain.toml` does not affect which rust-analyzer runs.
- Because the launcher `exec`s that path directly instead of going through `rustup run`, `RUSTUP_TOOLCHAIN` does not leak into rust-analyzer's child processes. Cargo invoked by rust-analyzer still uses the project's own toolchain, so builds and fingerprints stay consistent with CLI cargo.
- Without rustup (for example Homebrew), the launcher uses `rust-analyzer` from `PATH`.
- When no binary is found, it exits 1. Claude Code reports a crash, and doctor explains why.

**Options.**
- `cargo.targetDir: true` puts rust-analyzer's cargo invocations in a subdirectory of the target dir. This avoids build-lock contention with the agent's own cargo commands, at the cost of extra disk.
- `checkOnSave: false` prevents a workspace-wide `cargo check` after every edit.
  - Cost: cargo-level diagnostics such as borrowck and trait errors are not pushed automatically. rust-analyzer's native diagnostics still are.
  - The agent obtains compiler errors on demand with `rust-ai-lean diag`.

**Conflict.** The official `rust-analyzer-lsp` plugin registers a server for `.rs` too. Doctor reports it and offers to disable it.

### 4.2 Hooks

`hooks/hooks.json` registers `hooks/session-start.sh` for `SessionStart` (no matcher, so it runs on startup, resume, clear, and compact) and `SubagentStart` (the script receives the event name).

**Behavior, in order:**

1. **Environment** (SessionStart only). If `CLAUDE_ENV_FILE` is set and `CARGO_TERM_QUIET` is not already set in the environment, append `export CARGO_TERM_QUIET=true`. An explicit user value, including `false`, is respected. This runs for every session; it is harmless for non-Rust work.
2. **Rust project detection.** The project is Rust if a `Cargo.toml` exists in the cwd or an ancestor up to the git root, or within two directory levels below the cwd, excluding `target/`. The downward search covers multi-repo workspaces whose root is not itself a crate. If nothing is found, exit 0 with no output.
3. **Environment probe.**
   - `ra_ok`: the same resolution logic as the LSP launcher succeeds.
   - `cli_ok`: `rust-ai-lean` is on `PATH`.
4. **Hint injection.** Emit `{"hookSpecificOutput":{"hookEventName":"<event>","additionalContext":"<hint>"}}`. The hint is English, **at most 700 bytes** (enforced by a test), and assembled from these lines:

| Line | Included when |
|---|---|
| `[rust-ai-lean] Rust project: keep context small.` | always |
| `Outline before reading: rust-ai-lean outline <file\|dir> gives signatures + line ranges; Read only those ranges, never whole large .rs files.` | `cli_ok` |
| `Semantic lookups: LSP tool (goToDefinition, findReferences, hover, incomingCalls); load via ToolSearch "select:LSP" if deferred.` | `ra_ok` and SessionStart |
| `Crate APIs: rust-ai-lean crate-src <crate> = exact Cargo.lock version source. Do not use Context7/web docs for Rust crate APIs.` | `cli_ok` |
| `Compile errors: rust-ai-lean diag check -p <pkg>. Never --message-format=short.` | `cli_ok` |
| `Setup incomplete (<LSP unavailable\|CLI missing>): ask the user to run /rust-ai-lean:doctor.` | `!ra_ok` or `!cli_ok` |
| `Guide: skill rust-ai-lean:rust-lean.` | always |

- The LSP line is omitted for subagents, because background subagents are reported not to receive the LSP tool.
- **Budget:** the whole script must finish within 1 second on a typical project, and it runs no cargo commands.
- **Out of scope for the hook:** it does not parse Claude Code settings JSON. Configuration conflicts are doctor's job.

### 4.3 CLI `rust-ai-lean`

All subcommands share these rules:
- No color and no progress output.
- Deterministic ordering.
- `--version` prints the shared version.
- Dependencies: `syn` (`full`), `proc-macro2` (`span-locations`), `prettyplease`, `serde`/`serde_json`, `clap`, `anyhow`.
- Edition 2024.

#### `outline <path>...`

Accepts files and directories. Directories are walked recursively in sorted order, skipping `target/` and hidden directories. Output example:

```
src/repo.rs
  9-37     pub async fn insert_user(conn: &mut PgConnection, id: &str, name: &str) -> Result<User, Error>
  120-260  impl UserService
  121-130    pub fn new(db: Db) -> Self
  300-310  pub struct Account { pub name: String, pub balance: i64 }
  400-620  #[cfg(test)] mod tests  (18 items)
```

**Items.**
- Listed: free functions, structs (fields inline), enums (variant names), unions, traits and their method signatures, impl blocks with their items, modules (recursive), consts and statics (with type), type aliases, `macro_rules!` names.
- Not listed: `use` items.

**Rendering.**
- Each signature is normalized to one line with `prettyplease` and never truncated.
- Each line starts with the item's inclusive line range, so the agent can call `Read` with offset/limit or target an LSP position.
- `#[cfg(test)]` modules collapse to one summary line.
- `--docs` adds the first doc-comment line under each item.

**Parse failure** (common mid-edit): print `<file>: parse error at L:C; approximate outline` and fall back to a line scan that recognizes item-introducing keywords at line start. Output is still produced.

**Known blind spot:** items generated by macros are not visible. This is documented in the `rust-lean` skill.

**Exit code:** 0, unless a path could not be read. Parse fallback does not count as an error.

#### `crate-src <crate> [--manifest-path <path>] [--path-only]`

Runs `cargo metadata --format-version 1 --locked` and resolves the package or packages named `<crate>` (hyphen/underscore-insensitive) in the resolved graph:

```
rust_decimal 1.42.0 (registry+https://github.com/rust-lang/crates.io-index) features=[db-postgres,serde,std]
  <path to crate root>
```

- **All versions:** every version present in the graph is listed.
- **Features** are the ones enabled in the resolve, so the agent can tell whether a cfg-gated API exists.
- **Composable:** `--path-only` prints only the paths, one per line.
- **No Cargo.lock changes:** `--locked` guarantees that. When the lock is stale, cargo's error is printed verbatim with a hint, and the exit code is 1.
- **Crate not found:** exit 1, listing up to five package names that contain the query as a substring.
- **No manifest found:** exit 2.
- **No built-in search:** the skill teaches combining the output with `rg`/`grep` and `outline`.

#### `diag <check|clippy|build> [--max-errors N] [--full-warnings] [-- <cargo args>]`

Runs `cargo <sub> --message-format=json <cargo args>` and renders its output:

- **Errors:** the full `message.rendered` text, keeping `help:` and `note:`. Byte-identical messages are de-duplicated; JSON mode repeats diagnostics per target. Shown in compiler order, at most `N` (default 20, `0` = unlimited), followed by `… +K more errors (use --max-errors 0)`.
- **Warnings:** printed after all errors, one line each as `path:line:col: warning[code]: message`. Identical lines (the same warning repeated across targets) are printed once and counted in the summary. Warnings at different locations are never merged. `--full-warnings` renders them fully instead, which is useful when fixing clippy suggestions on a narrowed `-p`. **Warnings never consume the error limit.**
- **Other cargo stderr:** resolver errors, build-script failures, linker errors, and anything else that is not JSON are passed through verbatim. The only lines removed are the known cargo status lines (`Compiling`, `Checking`, `Fresh`, `Finished`, `Blocking`, `Downloading`, `Downloaded`, `Locking`, `Updating`, `Adding`). **Unrecognized output is never dropped.** A stdout line that fails to parse as JSON is also passed through.
- **Summary** as the final line: `N errors, M warnings (K duplicates merged)`.
- **Exit code:** cargo's own. Signals propagate.
- `test` is intentionally unsupported. Test output is handled by `CARGO_TERM_QUIET`.

### 4.4 Skill `doctor`

`/rust-ai-lean:doctor` has frontmatter `disable-model-invocation: true`. The SKILL.md is written in English and instructs replying in the user's language. It runs four phases.

**Phase 1 — static checks.** Run `sh "${CLAUDE_SKILL_DIR}/scripts/doctor.sh" --plugin-root "${CLAUDE_SKILL_DIR}/../.." --project "$PWD"`.
- Each output line is `STATUS<TAB>id<TAB>message<TAB>fix`, with `STATUS` ∈ `OK|WARN|FAIL|INFO` and `fix` possibly empty.
- Every check is independent. A check that itself errors reports `WARN <id> check failed: <reason>`, and the script always exits 0.

| id | Check | Fix offered |
|---|---|---|
| `toolchain.rustup` | rustup on `PATH` (WARN if absent; the launcher falls back to `PATH`) | — |
| `toolchain.ra` | the launcher's resolution succeeds and `rust-analyzer --version` runs | `rustup component add rust-analyzer --toolchain stable` |
| `toolchain.rust-src` | the project's active toolchain (`rustup show active-toolchain` in the project dir) has `lib/rustlib/src` | `rustup component add rust-src --toolchain <toolchain>` |
| `cli.installed` | `rust-ai-lean` on `PATH` | `cargo install --git https://github.com/enoqv/rust-ai-lean --tag v<ver> --locked` |
| `cli.version` | CLI version equals `plugin.json` version | same command plus `--force` |
| `env.quiet` | `CARGO_TERM_QUIET` set in the current shell (WARN: usually means the session predates the plugin) | start a new session |
| `project.detected` | same detection as the hook (INFO when not a Rust project; the remaining `project.*` checks are then reported as INFO `skipped`) | — |
| `project.lock` | `Cargo.lock` exists (WARN; `crate-src` needs it; never generated automatically) | — |
| `project.crate-src` | `rust-ai-lean crate-src` resolves the first direct dependency | — |

**Phase 2 — live checks, performed by the model.** Only the model can call tools.

1. `claude plugin list --json`:
   - FAIL if `rust-analyzer-lsp@claude-plugins-official` is enabled; fix: `claude plugin disable rust-analyzer-lsp@claude-plugins-official`.
   - FAIL if `rust-ai-lean@rust-ai-lean` is missing or disabled.
   - INFO if a Context7 plugin is enabled, as a reminder of the Rust crate rule.
2. Load the LSP tool, using ToolSearch `select:LSP` if it is deferred. If it is unavailable, FAIL: usually a restart is needed after installing.
3. `documentSymbol` on a project `.rs` file (prefer `src/lib.rs`, then `src/main.rs`, then the first file from `outline`).
   - "crashed": FAIL, pointing at `toolchain.ra`.
   - Empty result: rust-analyzer may still be indexing. Retry up to 3 times with a short wait between attempts, then WARN.
4. `hover` on a function found in step 3. Pass if a signature comes back. This also exercises sysroot and proc-macro loading.
5. `ps` for rust-analyzer processes: report count and RSS as INFO.
6. Report whether this session's context contains the `[rust-ai-lean]` hint, as the SessionStart hook check.

**Phase 3 — fixes.** Print one summary table. Then, for each FAIL or WARN that has a fix, ask the user individually and run the fix only on approval.

**Phase 4 — re-check.** Re-run the affected static checks. If any applied fix affects the LSP, hooks, or plugins, tell the user to start a new session and run doctor again.

### 4.5 Skill `rust-lean`

A model-invoked usage guide in English, at most about 600 words.
- The description targets: working in a Rust/Cargo project, reading or navigating `.rs` code, looking up crate APIs, fixing compile errors, running cargo.
- If the `paths` frontmatter works (§7), set it to `**/*.rs`, `**/Cargo.toml`.

Contents:

1. **Navigation order.** `outline` first, then LSP (`documentSymbol`, `goToDefinition`, `hover`, `findReferences`, `incomingCalls`), then `Read` with offset/limit from outline ranges. No whole-file `Read` or `cat` of large `.rs` files. `grep -n` is for locating, not reading. Macro-generated items are invisible to `outline`; use LSP or grep for those.
2. **Dependency APIs.** `crate-src <crate>`, then `outline` or `rg` inside that path. Never guess versions. Do not use Context7 or web docs for Rust crate APIs. After `cargo add`, read the `Cargo.toml` diff; quiet mode hides the added version and features.
3. **Fix loop.** `diag check -p <pkg>`, never `--message-format=short`. Errors first, warnings second. Finish with `diag clippy`. Do not kill cargo that is waiting on a build lock.
4. **Tests.** Scope with `-p` and a test-name filter. After a failure, re-run only the failing test. Keep failing output short, because long failing output loses its middle.
5. **Without LSP** (for example in subagents): CLI plus grep.

## 5. Error handling summary

| Component | Principle |
|---|---|
| LSP launcher | Exit non-zero with a one-line stderr reason; doctor turns it into an actionable fix |
| Hooks | Never break a session: any internal failure exits 0 with no output; env-file write failures are ignored |
| `outline` | Degrade (approximate outline) instead of failing on syntax errors; continue past unreadable paths and exit 1 at the end |
| `crate-src` | Surface cargo's own error verbatim; never modify `Cargo.lock` |
| `diag` | Never drop output it does not understand; preserve cargo's exit code |
| `doctor.sh` | Checks are isolated; a crashing check becomes a WARN line; always exit 0 |
| `doctor` skill | Never applies a fix without per-item approval |

## 6. Testing and CI

**CLI integration tests** (`tests/`, fixtures under `tests/fixtures/`, each built in a temp target dir):

- `outline`:
  - Multi-line signatures are rendered complete on one line.
  - Nested impl/trait items are indented under their parent.
  - `#[cfg(test)]` modules collapse.
  - A syntactically broken file yields an approximate outline.
  - Directory walking skips `target/`.
- `crate-src`:
  - The fixture has one path dependency and one small registry dependency, with a committed `Cargo.lock`.
  - Resolves exact versions and features, handles hyphen/underscore normalization, and exits correctly for not-found and stale-lock cases.
- `diag`:
  - A fixture with ≥25 warnings followed by errors must show **every** error.
  - Duplicate diagnostics merge.
  - `--max-errors` truncates with the `+K more` line.
  - A failing `build.rs` has its non-JSON stderr preserved.
  - The exit code matches cargo's.
- **Version consistency:** `Cargo.toml` = `plugin.json` = marketplace entry.

**Shell tests** (`tests/shell/run.sh`, run locally and by CI; not wired into `cargo test` because it needs `dash` and `shellcheck`):
- `shellcheck -s sh` on all scripts.
- Run all scripts under `dash` on Linux to catch non-POSIX syntax.
- `session-start.sh` is exercised with a temp `CLAUDE_ENV_FILE` and stub `rustup`/`rust-analyzer`/`rust-ai-lean` on a fake `PATH`, across the four `ra_ok` × `cli_ok` states, both events, non-Rust directories, and a pre-set `CARGO_TERM_QUIET=false`. It asserts JSON shape, the 700-byte cap, and env-file contents.
- `doctor.sh` is exercised with the same stubs; every check id appears exactly once with the expected status.

**CI** (`.github/workflows/ci.yml`):
- Matrix: `ubuntu-latest`, `macos-latest`.
- Steps: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, the shell tests (shellcheck on Linux).
- Third-party actions pinned to full commit SHAs. The toolchain is installed with the runner's preinstalled rustup rather than a third-party action.
- A generic check fails on absolute home-directory paths (`/home/<user>/`, `/Users/<user>/`) in tracked files.

**Benchmarks.** `scripts/bench.sh` regenerates the README table from the fixtures: raw vs. quiet `cargo test`; human vs. `short` vs. `diag` for `cargo check`; `cat -n` vs. `outline`.

## 7. Assumptions to validate first

The implementation plan starts with these spikes. Each outcome feeds back into this spec before dependent work begins.

1. The inline `sh -c` launcher in `lspServers` starts rust-analyzer through Claude Code's LSP spawn, with `args` passed as an array.
2. `SubagentStart` hooks can inject `additionalContext` into the subagent. If not, hints are SessionStart-only.
3. Skill `paths` frontmatter activates `rust-lean` when `.rs` files are touched. If not, rely on the description and the hint.
4. Precedence between `rust-analyzer.toml` and the plugin's `initializationOptions`. The README guidance depends on it.
5. Variables exported through `CLAUDE_ENV_FILE` are visible to Bash commands run by subagents.

## 8. Acceptance criteria

1. On a machine where the official `rust-analyzer-lsp` plugin is enabled and the stable toolchain lacks `rust-analyzer`, doctor reports both issues with correct fixes and applies each only after approval. After a new session, doctor reports no FAIL, and `documentSymbol` and `hover` both succeed on a pinned-toolchain project.
2. `scripts/bench.sh` on the fixtures shows:
   - `outline` output ≤ 20% of `cat -n` for the multi-signature fixture.
   - `diag check` preserves every `help:`/`note:` line of every error while dropping all status lines.
   - Quiet `cargo test` output ≤ 10% of the default for the 300-test fixture, with the failure's panic message intact.
3. The hint stays ≤ 700 bytes in every state, and the hook completes within 1 second on the fixtures.
4. CI is green on Linux and macOS.

## 9. Repository hygiene (public repo)

- Only generic content. No machine-specific paths, user names, internal hostnames, email addresses, or material from private projects in code, fixtures, docs, or commit messages.
- All examples and numbers come from the fixtures in this repository.
- Before every push, a manual review for private identifiers is done with a checklist kept **outside** the repository, so that the checklist itself does not leak those identifiers. CI additionally enforces the generic home-path check (§6).
- Commit messages and PR descriptions are in English.

## Appendix: references

- Claude Code docs — hooks: <https://code.claude.com/docs/en/hooks>; plugins reference: <https://code.claude.com/docs/en/plugins-reference>; costs: <https://code.claude.com/docs/en/costs>
- Official rust-analyzer LSP plugin: <https://github.com/anthropics/claude-plugins-official/tree/main/plugins/rust-analyzer-lsp>; per-workspace instance memory report: <https://github.com/anthropics/claude-plugins-official/issues/3417>
- Background subagents lack the LSP tool (reported): <https://github.com/anthropics/claude-code/issues/76090>
- Cargo JSON messages: <https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages>; `build.warnings`: <https://doc.rust-lang.org/cargo/reference/config.html#buildwarnings>
- rtk hides compile errors behind warnings: <https://github.com/rtk-ai/rtk/issues/3762>
- Proposal for agent-oriented, lockfile-versioned docs: <https://github.com/rust-lang/cargo/issues/16720>
- Anthropic, building a C compiler with Claude (test-harness output hygiene): <https://www.anthropic.com/engineering/building-c-compiler>
- Rust vs. Python token usage in agentic coding: <https://arxiv.org/html/2607.22807v1>
- cargo-nextest reporting options (considered, not adopted in v1): <https://nexte.st/docs/reporting/>
