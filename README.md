# gh-tui

A fast, terminal-based replacement for the GitHub web UI, distributed as a
[`gh`](https://cli.github.com/) CLI extension. Written in Rust. Invoked as
`gh tui`.

Primary motivations: speed, keyboard-first vim-style navigation, first-class
support for code review, GitHub Actions, images, emojis, and Mermaid diagrams.

## Status

**Phase 1 — foundation.** Not yet usable. The MVU event loop, auth detection,
and architectural spine are in place; real screens (PR list, diff view, etc.)
land in subsequent phases.

## Build

Requires Rust 1.95+.

```bash
cargo run -p gh-tui
```

On first launch the app shells out to `gh auth token` (or reads `GH_TOKEN` /
`GITHUB_TOKEN`) to determine authentication. `q` or `Ctrl+C` exits cleanly.

## Architecture

Six-crate Cargo workspace with a strict dependency graph:

- **`gh-core`** — pure domain: `State`, `Msg`, `Cmd`, reducers. No I/O.
- **`gh-input`** — vim-style key resolver. No in-workspace deps.
- **`gh-api`** — REST + GraphQL client, auth, ETag cache, rate limiting.
- **`gh-render`** — pure rendering helpers: markdown, diff, syntax, images.
- **`gh-ui`** — `ratatui` widgets and screen composition.
- **`gh-tui`** — the binary: argv, tracing, panic guard, MVU loop driver.

Model-View-Update: pure reducers in `gh-core`, side effects as commands
dispatched to async workers, messages posted back through an `mpsc` channel.
The render loop never blocks on I/O.

## License

Licensed under either of

- Apache License, Version 2.0
- MIT License

at your option.
