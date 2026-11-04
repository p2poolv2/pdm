This file provides guidance to AI coding agents working with code in this repository.

## Project overview

PDM is the P2Poolv2 Deployment and Management tool: a terminal UI
(ratatui) for configuring and managing a bitcoin node, a P2Poolv2 node,
a lightning node and a P2Poolv2 share marketplace.

Project name is PDM (no whitespace). Rust edition 2024, minimum
rust-version 1.88.

## Build and test commands

```sh
cargo build --all-targets
cargo test                                  # what CI runs
cargo test test_name                        # single test by name substring
cargo clippy --all-targets                  # must pass with zero warnings
cargo fmt --all -- --check                  # must pass
just run                                    # run the TUI locally
```

A task is not done until `cargo clippy --all-targets` and
`cargo fmt --all -- --check` both pass cleanly (no new warnings, zero
errors).

## Testing

- Set mock expectations inline in each test -- no mock setup helpers.
  This keeps each test self-contained and readable.
- Test helpers are allowed only if they are generic and reusable, and
  they live in a shared module, not local to a test module. Local
  helpers inevitably diverge and hide setup differences between tests.
- **No loops over collections in tests.** Do not iterate a table of
  cases inside a test. Write each case as its own `#[test]` with a name
  that says what it covers, or as explicit sequential assertions.
  Looping adds indirection to read, hides which case failed, and lets
  one case's setup silently drift from the others.

## Workflow

- For multi-step plans, ask to commit after each step.
- Never create commits yourself. After completing a logical step, ask the user to review and commit before continuing.
- Code reviews stay local -- do not post to GitHub.

## Rust conventions

- **Imports:** All `use` declarations at top of module (or top of
  `#[cfg(test)] mod tests`). Never inside function bodies. Import items
  and refer by short name; at most `parent_module::function`, never
  full crate paths inline (no `crate::module::item(...)` in code).
- **Preallocate collections:** Use `HashMap::with_capacity(n)` /
  `Vec::with_capacity(n)` when the size is known or estimable at
  construction. Flag bare `HashMap::new()` as a potential problem.
- **Error handling:** Do not replace fallible operations with default
  values unless that behaviour is explicitly intended. If error
  handling is unclear, ask.
- Update docstrings for changed functions.

### Hard constraints

- **No `unsafe`.**
- **ASCII only** in all src and comments (no unicode emdashes, arrows, etc.).

### Preferred style

- **No abbreviations or single-character variable names.**
- **Concurrency:** Prefer the actor/channel pattern over locks. If you
  must lock, evaluate deadlock risk and document lock ordering.
- **Functions:** One function does one thing. Extract when a block
  needs a comment to explain it, unless extraction forces lifetimes or
  unnecessary copies.
- Avoid continue/break. Prefer structured control flow; code is easier
  to reason about without non-local jumps.

## Prefer existing code

Before introducing a new utility, helper, or dependency:

- search the crate for an existing implementation
- reuse existing abstractions
- avoid duplicate helper functions

## Performance matters.

- Avoid unnecessary cloning, temporary allocations, and repeated I/O
  (file reads, RPC and HTTP calls) in the UI loop.
- When performance and readability conflict, ask before making a large refactor.

## Security

- Never add support for generating private keys. This should be done
  by external third party tools and wallets. Tools that generate
  private keys end up eventually having a security flaw.
