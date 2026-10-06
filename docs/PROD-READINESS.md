# Prod-readiness audit — thunder-daw

Status: v0.2.0 (cpal 0.18, eframe/egui 0.36, fundsp 0.23, edition 2024).

## Done

- Dependencies are current, no pinned old majors in `[dependencies]`
- CI (`.github/workflows/ci.yml`): fmt, clippy `-D warnings`, `cargo check`,
  `cargo test`, `--offline-test`
- 7 unit tests: automation interp, quantize, step roundtrip,
  save/load roundtrip, arpeggiator, engine scheduler fires, NaN-free render
- Dependabot: cargo + github-actions, weekly
- Releases: v0.1.0 + v0.2.0 on GitHub with Linux binaries + CHANGELOG.md
- Docs site + landing page (`website/`), deployed to GitHub Pages (`pages.yml`)
- README with requirements, run and test instructions
- MIT license
- Self-test (`--offline-test`) renders headless and asserts non-silence
- `.gitignore` covers `/target`; `Cargo.lock` is committed, as it should be for a binary

## Not done

- Audio thread uses `Mutex::try_lock` and drops to silence on contention.
  Acceptable for a toy, not for stage use.
- Fixed 8 channels. No add/remove, no VST hosting, no audio recording,
  no MIDI hardware I/O.

## To go prod

1. Push + enable Pages: done
2. Tag releases, keep CHANGELOG.md: done (v0.1.0, v0.2.0, see Releases page)
3. `cargo test` unit tests: done. Next: sequencer timing and pan-gain math tests
