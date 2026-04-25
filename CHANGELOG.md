# Changelog

All notable changes to this project are documented here. Format:
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning:
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Repo hygiene: `LICENSE-MIT`, `LICENSE-APACHE`, PR template, Dependabot config.

## [0.1.0] - YYYY-MM-DD

_(Placeholder — to be filled in when the first release tag is cut.)_

### Added

- Six-crate Rust workspace (`gh-core`, `gh-input`, `gh-api`, `gh-render`,
  `gh-ui`, `gh-tui`) with strict dependency graph.
- MVU event loop with RAII terminal guard, panic hook that restores the
  terminal, tracing to file.
- Async auth detection via `gh auth token` / `GH_TOKEN` / `GITHUB_TOKEN`.
- CI workflow (fmt, clippy, test, build, docs) on Linux + macOS.
- Tag-triggered release workflow with native-runner matrix.

[Unreleased]: https://github.com/JonathanBerhe/gh-tui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/JonathanBerhe/gh-tui/releases/tag/v0.1.0
