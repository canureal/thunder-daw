# thunder-daw

A small FL Studio-style DAW in Rust: step sequencer, piano roll, playlist, mixer.

- Audio through [cpal](https://crates.io/crates/cpal)
- Interface with [egui](https://crates.io/crates/egui) via [eframe](https://crates.io/crates/eframe)
- All sound from [fundsp](https://crates.io/crates/fundsp): each voice is a live DSP graph

## Install

Prebuilt binaries for Windows, macOS and Linux are attached to every
[release](https://github.com/canureal/thunder-daw/releases).
Pick the one for your system on the
[download page](https://canureal.github.io/thunder-daw/releases.html):
Windows gets an MSI wizard, Mac a drag-to-Applications disk image,
Linux an AppImage. No terminal required.

## Features

- Piano roll: add, drag and resize notes, velocity lane, ghost notes, scales,
  chord stamps, quantize, humanize, strum, arpeggiate
- Playlist: 8 patterns as one-bar clips, song or pattern mode
- Instruments: polyphonic synths (8 voices each), synthesized drum kit,
  sampler with WAV/MP3/OGG loading and a file browser
- Mixer: per-channel delay and reverb sends, soft-clipped master, scope, peak meter
- Automation lanes for volume and filter cutoff
- Save/load projects as JSON, export WAV mixdown or MIDI file
- Metronome, swing, tap tempo, song loop

## Build from source

You need a Rust toolchain (1.88 or newer):

```bash
git clone https://github.com/canureal/thunder-daw.git
cd thunder-daw
cargo run --release
```

Linux also needs the audio and windowing headers:

```bash
sudo apt install libasound2-dev libx11-dev libxkbcommon-dev libwayland-dev libegl-dev libgl-dev
```

Windows and macOS need nothing extra.

## Tests

Unit tests plus a headless audio render that fails on silence or NaN:

```bash
cargo test
cargo run -- --offline-test
```

## Website and docs

The site in [`website/`](website/) deploys to GitHub Pages on every push to `main`.
Releases live under [GitHub Releases](https://github.com/canureal/thunder-daw/releases)
with the [changelog](CHANGELOG.md) in the repo.
