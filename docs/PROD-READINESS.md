# Prod-readiness audit — thunder-daw

Honest status as of the latest-deps upgrade (cpal 0.18, eframe/egui 0.36, fundsp 0.23).

## Done ✅

- **Latest dependencies** — no pinned old majors left in `[dependencies]`
- **CI** (`.github/workflows/ci.yml`) — fmt, clippy `-D warnings`, `cargo check`, `--offline-test`
- **Dependabot** (`.github/dependabot.yml`) — cargo + github-actions, weekly, max 5 open PRs
- **Docs site + landing page** (`website/`) auto-deployed to GitHub Pages (`pages.yml`)
- **README** with requirements / run / test instructions
- **MIT license**
- **Self-test** (`--offline-test`) renders 2 s headless and asserts non-silence
- **`.gitignore`** covers `/target`; `Cargo.lock` is committed (correct for a binary)

## Not done ❌

- **Git remote live** — https://github.com/canureal/thunder-daw, Pages enabled
  (Actions source) at https://canureal.github.io/thunder-daw/
- **No release story** — no tags, no changelog, no packaged binaries, not on crates.io
- **No tests beyond the smoke test** — no `#[test]` unit tests, no coverage gate
- **Realtime shortcut** — audio thread uses `Mutex::try_lock` and drops to
  silence on contention; correct for a toy, not stage-ready
- **Dependabot active** — opened cargo + github-actions PRs on first push

## To go prod

1. ~~Push + enable Pages~~ done
2. ~~Tag `v0.1.0`, add CHANGELOG.md~~ done ([releases](https://canureal.github.io/thunder-daw/releases.html))
3. Add `cargo test` unit tests (sequencer timing, pan gains) so CI actually gates logic
