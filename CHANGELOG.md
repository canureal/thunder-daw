# Changelog

Changes are listed per release, newest first.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [v0.2.5] - 2026-10-08

### Added

- Old-Linux builds: every release now also ships a binary built against
  glibc 2.31 (Ubuntu 20.04 era), as a tarball and an AppImage, for systems
  the normal build refuses to start on

## [v0.2.4] - 2026-10-08

### Fixed

- Clicking the piano roll, velocity lane and automation lane did nothing.
  Pressing the mouse never started a drag, so no edit was ever recorded.
  Plain clicks now place notes, set velocity and add automation points,
  with a status-line message confirming each edit.

### Added

- In-app tutorial window on first launch, with a demo song loader
- New projects start blank (untitled, no notes) instead of the demo groove

## [v0.2.3] - 2026-10-06

### Added

- Double-click launch on all three systems: Windows MSI with Start Menu and
  desktop shortcuts plus a license page, macOS app bundle in a
  drag-to-Applications disk image, Linux AppImage
- Windows release builds no longer open a console window
- App icon (SVG, PNG, ICO, ICNS sources in `assets/`)

## [v0.2.2] - 2026-10-06

### Added

- Windows MSI installer with license page and Start Menu entry
- Homebrew formula (`brew install canureal/tap/thunder-daw`)
- Linux desktop entry and icon bundled in archives
- Custom install message pointing at the `thunder-daw` command

## [v0.2.1] - 2026-10-06

### Added

- Installers for all three desktop OSes: cargo-dist builds Linux, Windows
  and macOS binaries on every tag, with shell and PowerShell install scripts
- Sample browser opens the OS music folder on each platform

### Fixed

- File names now parse on Windows paths; music folder lookup no longer
  assumes `$HOME` exists

## [v0.2.0] - 2026-10-05

### Added

- Piano roll: click/drag note editing, resize, velocity lane, ghost notes,
  scale highlighting, chord stamps, quantize / humanize / strum / arpeggiate
- Playlist arrangement with Song / Pattern modes and per-bar pattern clips
- 8-voice polyphony per channel with voice stealing
- Sampler channels: WAV/MP3/OGG loading, built-in synthesized drum kit,
  sample browser with preview
- Mixer FX: per-channel delay + reverb sends, master delay/reverb
- Volume + filter-cutoff automation lanes with linear interpolation
- Project save/load (JSON), WAV mixdown export, MIDI (SMF) export
- Metronome, swing, tap tempo, song loop
- 7 unit tests (note tools, save/load, engine scheduler, NaN-free render)

### Fixed

- Zeroed channel defaults NaN'd the fundsp SVF and poisoned the mix bus;
  defaults are now playable and filter params are clamped

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

[v0.2.5]: https://github.com/canureal/thunder-daw/releases/tag/v0.2.5
[v0.2.4]: https://github.com/canureal/thunder-daw/releases/tag/v0.2.4
[v0.2.3]: https://github.com/canureal/thunder-daw/releases/tag/v0.2.3
[v0.2.2]: https://github.com/canureal/thunder-daw/releases/tag/v0.2.2
[v0.2.1]: https://github.com/canureal/thunder-daw/releases/tag/v0.2.1
[v0.2.0]: https://github.com/canureal/thunder-daw/releases/tag/v0.2.0
[v0.1.0]: https://github.com/canureal/thunder-daw/releases/tag/v0.1.0
