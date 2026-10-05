# thunder-daw

A tiny FL Studio-style step-sequencer DAW in Rust.

- **Audio backend:** [`cpal`](https://crates.io/crates/cpal) — cross-platform audio I/O
- **GUI:** [`egui`](https://crates.io/crates/egui) via [`eframe`](https://crates.io/crates/eframe) — immediate-mode UI, native + web
- **DSP:** [`fundsp`](https://crates.io/crates/fundsp) — every voice is a live fundsp graph
  (`var(pitch) >> osc >> lowpass_hz * (var(gate) >> adsr_live) >> pan`)

## Features

- 6-channel step sequencer (Kick / Snare / Hat / Bass / Lead / Pad), 16 steps each
- Per-channel editor: waveform, base note, filter cutoff + Q, ADSR
- Mixer with per-track volume + equal-power pan, master volume, peak meter
- Live piano (2 octaves) driving a dedicated fundsp saw voice
- Master scope rendering the real cpal output buffer
- Headless self-test: `cargo run -- --offline-test`

## Requirements (Linux)

```bash
sudo apt install libasound2-dev libx11-dev libxkbcommon-dev libwayland-dev libegl-dev libgl-dev
```

## Run

```bash
cargo run --release
```

## Self-test (no audio hardware / display needed)

```bash
cargo run -- --offline-test
```

## Website & docs

- Landing page + docs: [`website/`](website/) — deployed to GitHub Pages on every push to `main`
- Docs: open the deployed site and go to **Docs**

## Project status

See [PROD-READINESS](docs/PROD-READINESS.md) for the honest audit:
CI, Dependabot, Pages, and what's still missing.
