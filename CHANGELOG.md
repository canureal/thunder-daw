# Changelog

All notable changes to thunder-daw are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [v0.1.0] - 2026-10-05

Initial release.

### Added

- 6-channel × 16-step sequencer (Kick / Snare / Hat / Bass / Lead / Pad)
- Per-channel fundsp voices: `var(pitch) >> osc >> lowpass * adsr_live >> pan`
- Channel editor: waveform, base note (MIDI 21–108), cutoff, Q, ADSR with live graph rebuilds
- Mixer: per-track volume + equal-power pan, master bus with tanh soft clip
- Live 2-octave piano on a dedicated saw voice
- Master scope + peak meter
- Headless self-test: `cargo run -- --offline-test`
- CI (fmt, clippy `-D warnings`, check, offline-test), Dependabot, GitHub Pages site

### Fixed

- n/a (first release)

### Known limits

- Audio thread drops to silence on lock contention (`Mutex::try_lock`)
- Linux build needs ALSA/X11/Wayland dev packages (see README)
- No unit tests yet beyond the `--offline-test` smoke test

[v0.1.0]: https://github.com/canureal/thunder-daw/releases/tag/v0.1.0
