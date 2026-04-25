<!--
Keep PRs small. Each commit should compile and pass tests.
PR descriptions explain the *why*; the diff shows the *what*.
-->

## Why

<!-- What problem does this solve? What's the motivation? -->

## What changed

<!-- One or two bullets at most. -->

## Test plan

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Manual smoke (if UI changed): `cargo run -p gh-tui`

## Roadmap phase

<!-- e.g. Phase 1 / Phase 2; or "out of band" if not on the roadmap. -->
