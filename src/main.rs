use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui;
use fundsp::prelude::{
    adsr_live, highpass_hz, lowpass_hz, midi_hz, pan, saw, shared, sine, square, triangle, var,
    white, AudioUnit, Shared,
};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaveKind {
    Sine,
    Triangle,
    Saw,
    Square,
    Noise,
}

impl WaveKind {
    fn all() -> [WaveKind; 5] {
        [
            WaveKind::Sine,
            WaveKind::Triangle,
            WaveKind::Saw,
            WaveKind::Square,
            WaveKind::Noise,
        ]
    }
    fn name(&self) -> &'static str {
        match self {
            WaveKind::Sine => "Sine",
            WaveKind::Triangle => "Tri",
            WaveKind::Saw => "Saw",
            WaveKind::Square => "Square",
            WaveKind::Noise => "Noise",
        }
    }
}

#[derive(Clone)]
struct Track {
    name: String,
    wave: WaveKind,
    midi_base: u8,
    pattern: [bool; 16],
    volume: f32,
    pan: f32,
    cutoff: f32,
    q: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    mute: bool,
    solo: bool,
}

impl Track {
    fn kick() -> Self {
        Self {
            name: "Kick".into(),
            wave: WaveKind::Sine,
            midi_base: 36,
            pattern: [
                true, false, false, false, true, false, false, false, true, false, false, false,
                true, false, false, false,
            ],
            volume: 1.0,
            pan: 0.0,
            cutoff: 900.0,
            q: 0.8,
            attack: 0.001,
            decay: 0.12,
            sustain: 0.0,
            release: 0.05,
            mute: false,
            solo: false,
        }
    }
    fn snare() -> Self {
        Self {
            name: "Snare".into(),
            wave: WaveKind::Square,
            midi_base: 55,
            pattern: [
                false, false, false, false, true, false, false, false, false, false, false, false,
                true, false, false, false,
            ],
            volume: 0.8,
            pan: 0.0,
            cutoff: 3200.0,
            q: 0.8,
            attack: 0.001,
            decay: 0.09,
            sustain: 0.0,
            release: 0.04,
            mute: false,
            solo: false,
        }
    }
    fn hat() -> Self {
        Self {
            name: "Hat".into(),
            wave: WaveKind::Noise,
            midi_base: 84,
            pattern: [
                true, false, true, false, true, false, true, false, true, false, true, false, true,
                false, true, true,
            ],
            volume: 0.5,
            pan: 0.2,
            cutoff: 8000.0,
            q: 0.7,
            attack: 0.001,
            decay: 0.04,
            sustain: 0.0,
            release: 0.03,
            mute: false,
            solo: false,
        }
    }
    fn bass() -> Self {
        Self {
            name: "Bass".into(),
            wave: WaveKind::Saw,
            midi_base: 33,
            pattern: [
                true, false, false, true, false, false, true, false, false, true, false, false,
                true, false, false, false,
            ],
            volume: 0.85,
            pan: -0.1,
            cutoff: 520.0,
            q: 1.2,
            attack: 0.008,
            decay: 0.1,
            sustain: 0.7,
            release: 0.1,
            mute: false,
            solo: false,
        }
    }
    fn lead() -> Self {
        Self {
            name: "Lead".into(),
            wave: WaveKind::Square,
            midi_base: 69,
            pattern: [
                false, false, true, false, false, false, true, false, false, false, true, false,
                false, true, false, false,
            ],
            volume: 0.55,
            pan: 0.0,
            cutoff: 2400.0,
            q: 1.0,
            attack: 0.01,
            decay: 0.12,
            sustain: 0.6,
            release: 0.12,
            mute: false,
            solo: false,
        }
    }
    fn pad() -> Self {
        Self {
            name: "Pad".into(),
            wave: WaveKind::Triangle,
            midi_base: 57,
            pattern: [
                true, false, false, false, false, false, false, false, false, false, false, false,
                false, false, true, false,
            ],
            volume: 0.5,
            pan: 0.0,
            cutoff: 1200.0,
            q: 0.7,
            attack: 0.2,
            decay: 0.3,
            sustain: 0.7,
            release: 0.4,
            mute: false,
            solo: false,
        }
    }
}

// ---------------------------------------------------------------------------
// DSP: fundsp voices. cpal = audio backend, fundsp = all sound generation.
// ---------------------------------------------------------------------------

struct VoiceRt {
    unit: Box<dyn AudioUnit + Send>,
    pitch: Shared,
    gate: Shared,
}

#[allow(clippy::too_many_arguments)]
fn make_unit(
    wave: WaveKind,
    cutoff: f32,
    q: f32,
    a: f32,
    d: f32,
    s: f32,
    r: f32,
    pitch: &Shared,
    gate: &Shared,
    sample_rate: f64,
) -> Box<dyn AudioUnit + Send> {
    let cutoff = cutoff.clamp(40.0, 18000.0);
    let q = q.clamp(0.1, 8.0);
    let unit: Box<dyn AudioUnit + Send> = match wave {
        WaveKind::Sine => Box::new(
            ((var(pitch) >> sine::<f32>() >> lowpass_hz(cutoff, q))
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Triangle => Box::new(
            ((var(pitch) >> triangle() >> lowpass_hz(cutoff, q))
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Saw => Box::new(
            ((var(pitch) >> saw() >> lowpass_hz(cutoff, q)) * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Square => Box::new(
            ((var(pitch) >> square() >> lowpass_hz(cutoff, q))
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Noise => Box::new(
            ((white() >> highpass_hz(1500.0_f32, 0.7_f32) >> lowpass_hz(cutoff, q))
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
    };
    let mut unit = unit;
    unit.set_sample_rate(sample_rate);
    unit.allocate();
    unit
}

struct AudioState {
    tracks: Vec<Track>,
    voices: Vec<VoiceRt>,
    offs: Vec<usize>,
    live_unit: Box<dyn AudioUnit + Send>,
    live_pitch: Shared,
    live_gate: Shared,
    live_off_in: usize,
    bpm: f32,
    master_vol: f32,
    playing: bool,
    step: usize,
    samples_to_next: usize,
    sample_rate: f32,
    scope: Vec<f32>,
    scope_pos: usize,
    scope_tick: usize,
    peak: f32,
}

impl AudioState {
    fn new() -> Self {
        let sr = 44100.0;
        let tracks = vec![
            Track::kick(),
            Track::snare(),
            Track::hat(),
            Track::bass(),
            Track::lead(),
            Track::pad(),
        ];
        let mut voices = Vec::new();
        for t in &tracks {
            let pitch = shared(midi_hz(t.midi_base as f32));
            let gate = shared(0.0);
            let unit = make_unit(
                t.wave, t.cutoff, t.q, t.attack, t.decay, t.sustain, t.release, &pitch, &gate, sr,
            );
            voices.push(VoiceRt { unit, pitch, gate });
        }
        let live_pitch = shared(midi_hz(69.0));
        let live_gate = shared(0.0);
        let live_unit = make_unit(
            WaveKind::Saw,
            2600.0,
            1.0,
            0.005,
            0.08,
            0.7,
            0.1,
            &live_pitch,
            &live_gate,
            sr,
        );
        Self {
            tracks,
            voices,
            offs: vec![0; 6],
            live_unit,
            live_pitch,
            live_gate,
            live_off_in: 0,
            bpm: 128.0,
            master_vol: 0.9,
            playing: false,
            step: 0,
            samples_to_next: 0,
            sample_rate: sr as f32,
            scope: vec![0.0; 512],
            scope_pos: 0,
            scope_tick: 0,
            peak: 0.0,
        }
    }

    fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr as f32;
        self.offs = vec![0; self.tracks.len()];
        for (i, t) in self.tracks.clone().iter().enumerate() {
            self.voices[i].unit = make_unit(
                t.wave,
                t.cutoff,
                t.q,
                t.attack,
                t.decay,
                t.sustain,
                t.release,
                &self.voices[i].pitch,
                &self.voices[i].gate,
                sr,
            );
        }
        self.live_unit = make_unit(
            WaveKind::Saw,
            2600.0,
            1.0,
            0.005,
            0.08,
            0.7,
            0.1,
            &self.live_pitch,
            &self.live_gate,
            sr,
        );
    }

    fn rebuild_voice(&mut self, idx: usize) {
        let sr = self.sample_rate as f64;
        let t = self.tracks[idx].clone();
        self.voices[idx].unit = make_unit(
            t.wave,
            t.cutoff,
            t.q,
            t.attack,
            t.decay,
            t.sustain,
            t.release,
            &self.voices[idx].pitch,
            &self.voices[idx].gate,
            sr,
        );
    }
}

fn pan_gains(pan: f32, vol: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0);
    let angle = (p + 1.0) * std::f32::consts::FRAC_PI_4;
    (vol * angle.cos(), vol * angle.sin())
}

fn render_block<T>(s: &mut AudioState, data: &mut [T], channels: usize)
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let sr = s.sample_rate;
    let step_len = ((sr * 60.0 / s.bpm.max(30.0) / 4.0) as usize).max(1);
    if s.offs.len() != s.voices.len() {
        s.offs = vec![0; s.voices.len()];
    }
    for frame in data.chunks_mut(channels) {
        if s.playing {
            if s.samples_to_next == 0 {
                let cur = s.step % 16;
                let solo_any = s.tracks.iter().any(|t| t.solo);
                for i in 0..s.tracks.len() {
                    let audible = if solo_any {
                        s.tracks[i].solo
                    } else {
                        !s.tracks[i].mute
                    };
                    if audible && s.tracks[i].pattern[cur] {
                        let hz = midi_hz(s.tracks[i].midi_base as f32);
                        s.voices[i].pitch.set_value(hz);
                        s.voices[i].gate.set_value(1.0);
                        // fundsp envelope off-timer is managed here in the cpal backend
                        let len = match s.tracks[i].wave {
                            WaveKind::Noise => (sr * 0.09) as usize,
                            _ => ((step_len * 8 / 10).max((sr * 0.06) as usize))
                                .min((sr * 2.0) as usize),
                        };
                        s.offs[i] = len;
                    }
                }
                s.step = (cur + 1) % 16;
                s.samples_to_next = step_len;
            }
            s.samples_to_next = s.samples_to_next.saturating_sub(1);
            for i in 0..s.offs.len() {
                if s.offs[i] > 0 {
                    s.offs[i] -= 1;
                    if s.offs[i] == 0 {
                        s.voices[i].gate.set_value(0.0);
                    }
                }
            }
            if s.live_off_in > 0 {
                s.live_off_in -= 1;
                if s.live_off_in == 0 {
                    s.live_gate.set_value(0.0);
                }
            }
        }
        let mut l = 0.0f32;
        let mut r = 0.0f32;
        for i in 0..s.voices.len() {
            let (vl, vr) = s.voices[i].unit.get_stereo();
            let (lg, rg) = pan_gains(s.tracks[i].pan, s.tracks[i].volume);
            l += vl * lg;
            r += vr * rg;
        }
        {
            let (vl, vr) = s.live_unit.get_stereo();
            l += vl * 0.8;
            r += vr * 0.8;
        }
        l *= s.master_vol * 0.22;
        r *= s.master_vol * 0.22;
        l = l.tanh();
        r = r.tanh();
        s.peak = (s.peak * 0.9995).max(l.abs().max(r.abs()));
        s.scope_tick += 1;
        if s.scope_tick.is_multiple_of(4) {
            s.scope[s.scope_pos] = (l + r) * 0.5;
            s.scope_pos = (s.scope_pos + 1) % s.scope.len();
        }
        if channels == 0 {
            continue;
        } else if channels == 1 {
            frame[0] = T::from_sample(l);
        } else {
            frame[0] = T::from_sample(l);
            frame[1] = T::from_sample(r);
            for (c, x) in frame.iter_mut().enumerate().skip(2) {
                *x = if c % 2 == 0 {
                    T::from_sample(l)
                } else {
                    T::from_sample(r)
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Audio backend (cpal) startup
// ---------------------------------------------------------------------------

fn start_audio(state: Arc<Mutex<AudioState>>) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let supported = device.default_output_config().ok()?;
    let sr = supported.sample_rate() as f64;
    {
        if let Ok(mut s) = state.lock() {
            s.set_sample_rate(sr);
        }
    }
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels as usize;
    let err_fn = |err| eprintln!("audio stream error: {err}");
    let build = |cfg: cpal::StreamConfig| -> Option<cpal::Stream> {
        let st = Arc::clone(&state);
        match supported.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_output_stream(
                    cfg,
                    move |data: &mut [f32], _| {
                        if let Ok(mut s) = st.try_lock() {
                            render_block(&mut s, data, channels);
                        } else {
                            for x in data.iter_mut() {
                                *x = 0.0;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .ok(),
            cpal::SampleFormat::I16 => device
                .build_output_stream(
                    cfg,
                    move |data: &mut [i16], _| {
                        if let Ok(mut s) = st.try_lock() {
                            render_block(&mut s, data, channels);
                        } else {
                            for x in data.iter_mut() {
                                *x = 0;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .ok(),
            cpal::SampleFormat::U16 => device
                .build_output_stream(
                    cfg,
                    move |data: &mut [u16], _| {
                        if let Ok(mut s) = st.try_lock() {
                            render_block(&mut s, data, channels);
                        } else {
                            for x in data.iter_mut() {
                                *x = 32768;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .ok(),
            _ => None,
        }
    };
    let stream = build(config)?;
    stream.play().ok()?;
    Some(stream)
}

// ---------------------------------------------------------------------------
// GUI (egui via eframe)
// ---------------------------------------------------------------------------

struct DawApp {
    audio: Arc<Mutex<AudioState>>,
    selected: usize,
}

impl DawApp {
    fn new(audio: Arc<Mutex<AudioState>>) -> Self {
        Self { audio, selected: 3 }
    }

    fn trigger_live(&self, midi: u8) {
        if let Ok(mut s) = self.audio.lock() {
            let hz = midi_hz(midi as f32);
            s.live_pitch.set_value(hz);
            s.live_gate.set_value(1.0);
            s.live_off_in = (s.sample_rate * 0.4) as usize;
        }
    }
}

impl eframe::App for DawApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
        let mut rebuild_idx: Option<usize> = None;
        let mut live_hit: Option<u8> = None;

        egui::Panel::top("transport").show(ui, |ui| {
            ui.horizontal(|ui| {
                let (mut playing, bpm, master, step, peak) = {
                    if let Ok(s) = self.audio.lock() {
                        (s.playing, s.bpm, s.master_vol, s.step, s.peak)
                    } else {
                        (false, 128.0, 0.9, 0, 0.0)
                    }
                };
                let play_label = if playing { "⏹ Stop" } else { "▶ Play" };
                if ui.button(play_label).clicked() {
                    playing = !playing;
                    if let Ok(mut s) = self.audio.lock() {
                        s.playing = playing;
                        if playing && s.samples_to_next == 0 {
                            s.samples_to_next = 0;
                        }
                    }
                }
                if ui.button("⏮ Reset").clicked() {
                    if let Ok(mut s) = self.audio.lock() {
                        s.step = 0;
                        s.samples_to_next = 0;
                    }
                }
                let mut bpm_mut = bpm;
                ui.add(egui::Slider::new(&mut bpm_mut, 60.0..=200.0).text("BPM"));
                if (bpm_mut - bpm).abs() > f32::EPSILON {
                    if let Ok(mut s) = self.audio.lock() {
                        s.bpm = bpm_mut;
                    }
                }
                let mut master_mut = master;
                ui.add(egui::Slider::new(&mut master_mut, 0.0..=1.5).text("Master"));
                if (master_mut - master).abs() > f32::EPSILON {
                    if let Ok(mut s) = self.audio.lock() {
                        s.master_vol = master_mut;
                    }
                }
                ui.label(format!("Step {:02}/16", (step % 16) + 1));
                ui.add(
                    egui::ProgressBar::new(peak.clamp(0.0, 1.0))
                        .text(format!("{:.0}%", peak * 100.0)),
                );
                if ui.button("Demo pattern").clicked() {
                    if let Ok(mut s) = self.audio.lock() {
                        s.tracks[0].pattern = Track::kick().pattern;
                        s.tracks[1].pattern = Track::snare().pattern;
                        s.tracks[2].pattern = Track::hat().pattern;
                        s.tracks[3].pattern = Track::bass().pattern;
                        s.tracks[4].pattern = Track::lead().pattern;
                        s.tracks[5].pattern = Track::pad().pattern;
                    }
                }
                if ui.button("Clear all").clicked() {
                    if let Ok(mut s) = self.audio.lock() {
                        for t in s.tracks.iter_mut() {
                            t.pattern = [false; 16];
                        }
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                // ---- Channel rack ----
                ui.vertical(|ui| {
                    ui.heading("Channel rack");
                    let n = {
                        if let Ok(s) = self.audio.lock() {
                            s.tracks.len()
                        } else {
                            0
                        }
                    };
                    for i in 0..n {
                        let (name, wave_name, mute, solo, is_current) = {
                            if let Ok(s) = self.audio.lock() {
                                let t = &s.tracks[i];
                                (
                                    t.name.clone(),
                                    t.wave.name().to_string(),
                                    t.mute,
                                    t.solo,
                                    s.step % 16,
                                )
                            } else {
                                continue;
                            }
                        };
                        let playing_step = {
                            if let Ok(s) = self.audio.lock() {
                                if s.playing {
                                    Some((s.step + 15) % 16)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        };
                        ui.horizontal(|ui| {
                            if ui.selectable_label(self.selected == i, name).clicked() {
                                self.selected = i;
                            }
                            ui.label(wave_name);
                            let mut m = mute;
                            if ui.toggle_value(&mut m, "M").changed() {
                                if let Ok(mut s) = self.audio.lock() {
                                    s.tracks[i].mute = m;
                                }
                            }
                            let mut so = solo;
                            if ui.toggle_value(&mut so, "S").changed() {
                                if let Ok(mut s) = self.audio.lock() {
                                    s.tracks[i].solo = so;
                                }
                            }
                            // 16 step buttons
                            for step_idx in 0..16 {
                                let active = {
                                    if let Ok(s) = self.audio.lock() {
                                        s.tracks[i].pattern[step_idx]
                                    } else {
                                        false
                                    }
                                };
                                let is_playing = playing_step == Some(step_idx);
                                let mut btn = egui::Button::new(format!("{}", step_idx + 1))
                                    .min_size(egui::vec2(26.0, 24.0));
                                if active && is_playing {
                                    btn = btn.fill(egui::Color32::LIGHT_GREEN);
                                } else if active {
                                    btn = btn.fill(egui::Color32::DARK_GREEN);
                                } else if is_playing {
                                    btn = btn.fill(egui::Color32::from_rgb(90, 70, 20));
                                } else if step_idx % 4 == 0 {
                                    btn = btn.fill(egui::Color32::from_rgb(55, 55, 60));
                                }
                                if ui.add(btn).clicked() {
                                    if let Ok(mut s) = self.audio.lock() {
                                        s.tracks[i].pattern[step_idx] =
                                            !s.tracks[i].pattern[step_idx];
                                    }
                                }
                            }
                        });
                        let _ = is_current;
                    }
                });

                ui.separator();

                // ---- Right side: editor + mixer + piano + scope ----
                ui.vertical(|ui| {
                    // Selected channel editor (fundsp params)
                    ui.heading("Channel editor (fundsp)");
                    {
                        let sel = self.selected;
                        if let Ok(s) = self.audio.lock() {
                            if sel >= s.tracks.len() {
                                return;
                            }
                        }
                        // read snapshot
                        let snap = {
                            if let Ok(s) = self.audio.lock() {
                                s.tracks[sel].clone()
                            } else {
                                return;
                            }
                        };
                        let mut edited = snap.clone();
                        ui.horizontal(|ui| {
                            ui.label("Wave:");
                            for w in WaveKind::all() {
                                if ui.selectable_value(&mut edited.wave, w, w.name()).clicked() {}
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Slider::new(&mut edited.midi_base, 21..=108)
                                    .text("Base note"),
                            );
                            ui.label(format!("({} Hz)", midi_hz(edited.midi_base as f32).round()));
                        });
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Slider::new(&mut edited.cutoff, 80.0..=12000.0)
                                    .logarithmic(true)
                                    .text("Cutoff"),
                            );
                            ui.add(egui::Slider::new(&mut edited.q, 0.3..=6.0).text("Q"));
                        });
                        ui.horizontal(|ui| {
                            ui.add(egui::Slider::new(&mut edited.attack, 0.001..=1.0).text("A"));
                            ui.add(egui::Slider::new(&mut edited.decay, 0.01..=1.0).text("D"));
                            ui.add(egui::Slider::new(&mut edited.sustain, 0.0..=1.0).text("S"));
                            ui.add(egui::Slider::new(&mut edited.release, 0.01..=1.5).text("R"));
                        });
                        let dsp_changed = edited.wave != snap.wave
                            || (edited.cutoff - snap.cutoff).abs() > 0.01
                            || (edited.q - snap.q).abs() > 0.001
                            || (edited.attack - snap.attack).abs() > 0.0001
                            || (edited.decay - snap.decay).abs() > 0.0001
                            || (edited.sustain - snap.sustain).abs() > 0.0001
                            || (edited.release - snap.release).abs() > 0.0001;
                        let base_changed = edited.midi_base != snap.midi_base;
                        if dsp_changed || base_changed {
                            if let Ok(mut s) = self.audio.lock() {
                                s.tracks[sel] = edited.clone();
                            }
                            if dsp_changed {
                                rebuild_idx = Some(sel);
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.button("♪ Test").clicked() {
                                live_hit = Some(edited.midi_base);
                            }
                            if ui.button("Randomize steps").clicked() {
                                if let Ok(mut s) = self.audio.lock() {
                                    use std::time::{SystemTime, UNIX_EPOCH};
                                    let seed = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .map(|d| d.as_nanos() as u64)
                                        .unwrap_or(12345);
                                    let mut x = seed;
                                    for st in 0..16 {
                                        x = x.wrapping_mul(6364136223846793005).wrapping_add(37);
                                        s.tracks[sel].pattern[st] = (x >> 33).is_multiple_of(3);
                                    }
                                }
                            }
                        });
                    }

                    ui.separator();
                    ui.heading("Mixer");
                    {
                        let n = {
                            if let Ok(s) = self.audio.lock() {
                                s.tracks.len()
                            } else {
                                0
                            }
                        };
                        ui.horizontal(|ui| {
                            for i in 0..n {
                                ui.vertical(|ui| {
                                    let (nm, mut vol, mut pn) = {
                                        if let Ok(s) = self.audio.lock() {
                                            let t = &s.tracks[i];
                                            (t.name.clone(), t.volume, t.pan)
                                        } else {
                                            return;
                                        }
                                    };
                                    ui.label(nm);
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut vol, 0.0..=1.5)
                                                .vertical()
                                                .text("vol"),
                                        )
                                        .changed()
                                    {
                                        if let Ok(mut s) = self.audio.lock() {
                                            s.tracks[i].volume = vol;
                                        }
                                    }
                                    if ui
                                        .add(egui::Slider::new(&mut pn, -1.0..=1.0).text("pan"))
                                        .changed()
                                    {
                                        if let Ok(mut s) = self.audio.lock() {
                                            s.tracks[i].pan = pn;
                                        }
                                    }
                                });
                            }
                        });
                    }

                    ui.separator();
                    ui.heading("Piano (live fundsp saw)");
                    {
                        ui.horizontal(|ui| {
                            // two octaves C4..B5
                            let names = [
                                "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                            ];
                            for oct in 0..2 {
                                for (semi, nm) in names.iter().enumerate() {
                                    let midi = 60 + oct * 12 + semi as u8;
                                    let is_black = nm.contains('#');
                                    let btn = egui::Button::new(format!("{nm}{}", 4 + oct))
                                        .min_size(egui::vec2(40.0, 44.0))
                                        .fill(if is_black {
                                            egui::Color32::BLACK
                                        } else {
                                            egui::Color32::from_gray(220)
                                        });
                                    let resp = ui.add(btn);
                                    if resp.clicked() {
                                        live_hit = Some(midi);
                                    }
                                }
                                ui.separator();
                            }
                        });
                    }

                    ui.separator();
                    ui.heading("Master scope (cpal output)");
                    {
                        let (scope, pos) = {
                            if let Ok(s) = self.audio.lock() {
                                (s.scope.clone(), s.scope_pos)
                            } else {
                                (vec![0.0; 512], 0)
                            }
                        };
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width().max(200.0), 90.0),
                            egui::Sense::hover(),
                        );
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(18));
                        let n = scope.len();
                        let mut prev: Option<egui::Pos2> = None;
                        for k in 0..n {
                            let v = scope[(pos + k) % n].clamp(-1.0, 1.0);
                            let x = rect.min.x + rect.width() * (k as f32 / n as f32);
                            let y = rect.center().y - v * rect.height() * 0.45;
                            let p = egui::pos2(x, y);
                            if let Some(pp) = prev {
                                painter.line_segment(
                                    [pp, p],
                                    egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_GREEN),
                                );
                            }
                            prev = Some(p);
                        }
                    }
                });
            });
        });

        if let Some(idx) = rebuild_idx {
            if let Ok(mut s) = self.audio.lock() {
                if idx < s.tracks.len() {
                    s.rebuild_voice(idx);
                }
            }
        }
        if let Some(m) = live_hit {
            self.trigger_live(m);
        }
    }
}

// ---------------------------------------------------------------------------

fn main() {
    if std::env::args().any(|a| a == "--offline-test") {
        let mut s = AudioState::new();
        s.set_sample_rate(44100.0);
        s.playing = true;
        s.bpm = 128.0;
        let mut buf = vec![0.0f32; 44100 * 2 * 2]; // 2s stereo
        render_block(&mut s, &mut buf, 2);
        let peak = buf.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
        println!("offline-test: peak={peak:.3} rms={rms:.4} step={}", s.step);
        println!("first samples: {:?}", &buf[0..8.min(buf.len())]);
        if peak < 0.01 {
            eprintln!("offline-test FAILED: silence");
            std::process::exit(1);
        }
        println!("offline-test OK");
        return;
    }
    let audio = Arc::new(Mutex::new(AudioState::new()));
    // cpal audio backend: keep stream alive for the whole GUI lifetime
    let _stream = match start_audio(Arc::clone(&audio)) {
        Some(st) => {
            println!("audio started");
            Some(st)
        }
        None => {
            eprintln!(
                "no audio device; running GUI silent. (Linux needs libasound2-dev at build time)"
            );
            None
        }
    };
    let app_audio = Arc::clone(&audio);
    let options = eframe::NativeOptions::default();
    // _stream must outlive run_native -> move it in and forget lifetime via closure capture
    let _ = _stream;
    if let Err(e) = eframe::run_native(
        "thunder-daw",
        options,
        Box::new(move |_cc| Ok(Box::new(DawApp::new(app_audio)) as Box<dyn eframe::App>)),
    ) {
        eprintln!("eframe error: {e}");
    }
}
