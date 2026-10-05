# Prod-readiness audit — thunder-daw

Honest status: v0.2.0 (cpal 0.18, eframe/egui 0.36, fundsp 0.23, edition 2024).

## Done ✅

- **Latest dependencies** — no pinned old majors left in `[dependencies]`
- **CI** (`.github/workflows/ci.yml`) — fmt, clippy `-D warnings`, `cargo check`,
  `cargo test`, `--offline-test`
- **Unit tests** — 7 tests: automation interp, quantize, step roundtrip,
  save/load roundtrip, arpeggiator, engine scheduler fires, NaN-free render
- **Dependabot** (`.github/dependabot.yml`) — cargo + github-actions, weekly, already merging
- **Releases** — v0.1.0 + v0.2.0 on GitHub with Linux binaries + CHANGELOG.md
- **Docs site + landing page** (`website/`) auto-deployed to GitHub Pages (`pages.yml`)
- **README** with requirements / run / test instructions
- **MIT license**
- **Self-test** (`--offline-test`) renders headless and asserts non-silence
- **`.gitignore`** covers `/target`; `Cargo.lock` is committed (correct for a binary)

## Not done ❌

- **Realtime shortcut** — audio thread uses `Mutex::try_lock` and drops to
  silence on contention; correct for a toy, not stage-ready
- **Fixed 8 channels** — no add/remove, no VST hosting, no audio recording,
  no MIDI hardware I/O

## To go prod

1. ~~Push + enable Pages~~ done
2. ~~Tag releases, add CHANGELOG.md~~ done ([releases](https://canureal.github.io/thunder-daw/releases.html))
3. ~~Add `cargo test` unit tests~~ done — next: sequencer timing/pan-gain math tests
