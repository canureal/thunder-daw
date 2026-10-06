//! Realtime engine: cpal backend, fundsp synth voices (polyphonic pools),
//! PCM kit + sampler playback, delay/reverb sends, sample-clock scheduler.

use crate::song::{ChannelKind, KitPiece, Song, TICKS_PATTERN, VoiceSpec, WaveKind, eval_auto};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude::{
    AudioUnit, Shared, Wave, adsr_live, dc, highpass_hz, lowpass, midi_hz, pan, reverb_stereo, saw,
    shared, sine, square, triangle, var, white,
};
use std::sync::{Arc, Mutex};

pub const VOICES_PER_CH: usize = 8;
const MAX_PLAYHEADS: usize = 32;
const SCOPE_LEN: usize = 512;

struct VoiceRt {
    unit: Box<dyn AudioUnit + Send>,
    pitch: Shared,
    gate: Shared,
    cutoff: Shared,
    vel: f32,
    off: usize,
    age: u64,
}

const LIVE_VOICE_SPEC: VoiceSpec = VoiceSpec {
    wave: WaveKind::Saw,
    q: 1.0,
    attack: 0.005,
    decay: 0.08,
    sustain: 0.7,
    release: 0.1,
};

fn make_voice(
    spec: VoiceSpec,
    pitch: &Shared,
    gate: &Shared,
    cutoff: &Shared,
    sample_rate: f64,
) -> Box<dyn AudioUnit + Send> {
    let VoiceSpec {
        wave,
        q,
        attack: a,
        decay: d,
        sustain: s,
        release: r,
    } = spec;
    // Degenerate Q (<= 0) NaNs the SVF: clamp hard.
    let q = q.clamp(0.1, 8.0);
    // Voice = (pitch-driven osc through audio-rate lowpass) * live ADSR, stereo.
    // Cutoff rides a Shared var: automation moves it with no graph rebuilds.
    let unit: Box<dyn AudioUnit + Send> = match wave {
        WaveKind::Sine => Box::new(
            ((((var(pitch) >> sine::<f32>()) | var(cutoff) | dc(q)) >> lowpass::<f32>())
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Triangle => Box::new(
            ((((var(pitch) >> triangle()) | var(cutoff) | dc(q)) >> lowpass::<f32>())
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Saw => Box::new(
            ((((var(pitch) >> saw()) | var(cutoff) | dc(q)) >> lowpass::<f32>())
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Square => Box::new(
            ((((var(pitch) >> square()) | var(cutoff) | dc(q)) >> lowpass::<f32>())
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
        WaveKind::Noise => Box::new(
            ((((white() >> highpass_hz(1500.0_f32, 0.7_f32)) | var(cutoff) | dc(q))
                >> lowpass::<f32>())
                * (var(gate) >> adsr_live(a, d, s, r)))
                >> pan(0.0),
        ),
    };
    let mut unit = unit;
    unit.set_sample_rate(sample_rate);
    unit.allocate();
    unit
}

// ---------------------------------------------------------------------------
// Built-in kit: synthesized PCM one-shots rendered at the engine sample rate
// ---------------------------------------------------------------------------

fn lcg(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(37);
    ((*seed >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn normalize(buf: &mut [f32]) {
    let peak = buf.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-6);
    let g = 0.95 / peak;
    for x in buf.iter_mut() {
        *x *= g;
    }
}

fn render_kick(sr: f32) -> Vec<f32> {
    let n = (sr * 0.4) as usize;
    let mut out = vec![0.0; n];
    let mut phase = 0.0;
    let mut seed = 12345;
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let f = 42.0 + 118.0 * (-t * 30.0).exp();
        phase += 2.0 * std::f32::consts::PI * f / sr;
        *x = phase.sin() * (-t * 8.0).exp();
        if t < 0.012 {
            *x += 0.4 * lcg(&mut seed) * (-t * 400.0).exp();
        }
    }
    normalize(&mut out);
    out
}

fn render_snare(sr: f32) -> Vec<f32> {
    let n = (sr * 0.25) as usize;
    let mut out = vec![0.0; n];
    let mut seed = 777;
    let mut prev = 0.0;
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let tone = (2.0 * std::f32::consts::PI * 185.0 * t).sin() * (-t * 22.0).exp() * 0.5;
        let nz = lcg(&mut seed);
        let hp = nz - prev;
        prev = nz;
        *x = tone + hp * (-t * 16.0).exp() * 0.55;
    }
    normalize(&mut out);
    out
}

fn render_clap(sr: f32) -> Vec<f32> {
    let n = (sr * 0.3) as usize;
    let mut out = vec![0.0; n];
    let mut seed = 999;
    let mut prev = 0.0;
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let nz = lcg(&mut seed);
        let hp = nz - prev;
        prev = nz;
        let mut v = 0.0;
        for &tb in &[0.0, 0.012, 0.024] {
            if t >= tb {
                v += (-(t - tb) * 45.0).exp();
            }
        }
        *x = hp * v * 0.5;
    }
    normalize(&mut out);
    out
}

fn render_hat(sr: f32) -> Vec<f32> {
    let n = (sr * 0.09) as usize;
    let mut out = vec![0.0; n];
    let mut seed = 31337;
    let mut prev = 0.0;
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let nz = lcg(&mut seed);
        let hp = nz - prev;
        prev = nz;
        *x = hp * (-t * 55.0).exp() * 0.5;
    }
    normalize(&mut out);
    out
}

fn render_click(sr: f32, freq: f32) -> Vec<f32> {
    let n = (sr * 0.05) as usize;
    let mut out = vec![0.0; n];
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        *x = (2.0 * std::f32::consts::PI * freq * t).sin() * (-t * 120.0).exp() * 0.6;
    }
    out
}

/// Load an audio file (wav/mp3/ogg/...) into mono f32 + native sample rate.
pub fn load_sample_file(path: &str) -> Result<(Vec<f32>, f32), String> {
    let wave = Wave::load(path).map_err(|e| format!("{e}"))?;
    if wave.is_empty() {
        return Err("empty audio file".into());
    }
    let n = wave
        .len()
        .min((wave.sample_rate() as usize).saturating_mul(30).max(1));
    let nch = wave.channels().max(1);
    let mut mono = vec![0.0; n];
    for (i, m) in mono.iter_mut().enumerate() {
        let mut v = 0.0;
        for c in 0..nch {
            v += wave.channel(c).get(i).copied().unwrap_or(0.0);
        }
        *m = v / nch as f32;
    }
    Ok((mono, wave.sample_rate() as f32))
}

// ---------------------------------------------------------------------------
// FX: manual stereo feedback delay + fundsp Schroeder reverb send
// ---------------------------------------------------------------------------

struct DelayLine {
    buf: Vec<(f32, f32)>,
    pos: usize,
}

impl DelayLine {
    fn new(sr: f32) -> Self {
        Self {
            buf: vec![(0.0, 0.0); (sr * 2.0) as usize + 1],
            pos: 0,
        }
    }
    fn process(&mut self, sr: f32, time: f32, inl: f32, inr: f32) -> (f32, f32) {
        if self.buf.is_empty() {
            return (0.0, 0.0);
        }
        let tap = ((time.clamp(0.02, 1.9) * sr) as usize).min(self.buf.len() - 1);
        let rp = (self.pos + self.buf.len() - tap) % self.buf.len();
        let (dl, dr) = self.buf[rp];
        let fb = 0.35;
        self.buf[self.pos] = (inl + dl * fb, inr + dr * fb);
        self.pos = (self.pos + 1) % self.buf.len();
        (dl, dr)
    }
}

fn pan_gains(pan: f32, vol: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (vol * angle.cos(), vol * angle.sin())
}

// ---------------------------------------------------------------------------
// Realtime state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum PlayKind {
    Kit(KitPiece),
    Sample(usize),
}

struct PlayHead {
    kind: PlayKind,
    pos: f32,
    gain: f32,
}

struct MetroHead {
    accent: bool,
    pos: usize,
}

enum SongEvent {
    NoteOn {
        ch: usize,
        midi: u8,
        vel: f32,
        len_ticks: u32,
    },
    Click {
        accent: bool,
    },
}

pub struct EngineRt {
    pub sr: f32,
    voices: Vec<Vec<VoiceRt>>,
    playheads: Vec<PlayHead>,
    metro_heads: Vec<MetroHead>,
    kit: [Vec<f32>; 4],
    delay: DelayLine,
    reverb: Box<dyn AudioUnit + Send>,
    metro_acc: Vec<f32>,
    metro_reg: Vec<f32>,
    sample_pos: u64,
    pub tick_pos: f64,
    pub cur_pat: usize,
    pub cur_tick: u32,
    pub playing: bool,
    pub song_mode: bool,
    pub pattern: usize,
    /// Export helper: stop at arrangement end even when the song loops.
    pub ignore_loop: bool,
    pub peak: f32,
    /// Per-channel post-fader peak, decaying. Feeds the mixer meters.
    pub ch_peaks: Vec<f32>,
    pub scope: Vec<f32>,
    scope_pos: usize,
    scope_tick: usize,
    auto_vol: Vec<f32>,
    age: u64,
    live_voice: VoiceRt,
    live_off: usize,
}

impl EngineRt {
    pub fn new(sr: f32) -> Self {
        let pitch = shared(midi_hz(69.0_f32));
        let gate = shared(0.0);
        let cutoff = shared(2600.0);
        let live_voice = VoiceRt {
            unit: make_voice(LIVE_VOICE_SPEC, &pitch, &gate, &cutoff, sr as f64),
            pitch,
            gate,
            cutoff,
            vel: 1.0,
            off: 0,
            age: 0,
        };
        let mut reverb: Box<dyn AudioUnit + Send> = Box::new(reverb_stereo(10.0, 2.5, 0.5));
        reverb.set_sample_rate(sr as f64);
        reverb.allocate();
        let mut rt = Self {
            sr,
            voices: Vec::new(),
            playheads: Vec::new(),
            metro_heads: Vec::new(),
            kit: [vec![], vec![], vec![], vec![]],
            delay: DelayLine::new(sr),
            reverb,
            metro_acc: render_click(sr, 2600.0),
            metro_reg: render_click(sr, 2000.0),
            sample_pos: 0,
            tick_pos: 0.0,
            cur_pat: 0,
            cur_tick: 0,
            playing: false,
            song_mode: true,
            pattern: 0,
            ignore_loop: false,
            peak: 0.0,
            ch_peaks: Vec::new(),
            scope: vec![0.0; SCOPE_LEN],
            scope_pos: 0,
            scope_tick: 0,
            auto_vol: Vec::new(),
            age: 0,
            live_voice,
            live_off: 0,
        };
        rt.render_kit();
        rt
    }

    fn render_kit(&mut self) {
        self.kit = [
            render_kick(self.sr),
            render_snare(self.sr),
            render_clap(self.sr),
            render_hat(self.sr),
        ];
    }

    pub fn set_sample_rate(&mut self, sr: f32, song: &Song) {
        self.sr = sr;
        self.render_kit();
        self.metro_acc = render_click(sr, 2600.0);
        self.metro_reg = render_click(sr, 2000.0);
        self.delay = DelayLine::new(sr);
        let mut reverb: Box<dyn AudioUnit + Send> = Box::new(reverb_stereo(10.0, 2.5, 0.5));
        reverb.set_sample_rate(sr as f64);
        reverb.allocate();
        self.reverb = reverb;
        self.voices.clear();
        self.ensure_channels(song);
        self.reset_transport();
    }

    /// (Re)build voice pools to match the song's channels.
    pub fn ensure_channels(&mut self, song: &Song) {
        if self.voices.len() == song.channels.len() {
            return;
        }
        self.voices.clear();
        for ch in &song.channels {
            let mut pool = Vec::with_capacity(VOICES_PER_CH);
            for _ in 0..VOICES_PER_CH {
                let pitch = shared(midi_hz(ch.midi_base as f32));
                let gate = shared(0.0);
                let cutoff = shared(ch.cutoff);
                let unit = make_voice(ch.voice_spec(), &pitch, &gate, &cutoff, self.sr as f64);
                pool.push(VoiceRt {
                    unit,
                    pitch,
                    gate,
                    cutoff,
                    vel: 1.0,
                    off: 0,
                    age: 0,
                });
            }
            self.voices.push(pool);
        }
        self.auto_vol = vec![1.0; song.channels.len()];
        self.ch_peaks = vec![0.0; song.channels.len()];
    }

    /// Rebuild one channel's pool (wave / Q / ADSR changed).
    pub fn rebuild_channel(&mut self, song: &Song, idx: usize) {
        if idx >= song.channels.len() {
            return;
        }
        if idx >= self.voices.len() {
            self.ensure_channels(song);
            return;
        }
        let spec = song.channels[idx].voice_spec();
        let cutoff_val = song.channels[idx].cutoff;
        for v in self.voices[idx].iter_mut() {
            v.unit = make_voice(spec, &v.pitch, &v.gate, &v.cutoff, self.sr as f64);
            v.cutoff.set_value(cutoff_val);
            v.off = 0;
        }
    }

    /// First free voice, else steal the oldest-sounding one.
    fn alloc_voice(&mut self, ch: usize) -> Option<&mut VoiceRt> {
        let pool = self.voices.get_mut(ch)?;
        let idx = pool
            .iter()
            .position(|v| v.off == 0)
            .or_else(|| {
                pool.iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
            })
            .unwrap_or(0);
        self.age = self.age.wrapping_add(1);
        let v = &mut pool[idx];
        v.age = self.age;
        Some(v)
    }

    fn push_playhead(&mut self, kind: PlayKind, gain: f32) {
        if self.playheads.len() >= MAX_PLAYHEADS {
            self.playheads.remove(0);
        }
        self.playheads.push(PlayHead {
            kind,
            pos: 0.0,
            gain,
        });
    }

    pub fn live_note(&mut self, midi: u8) {
        self.live_voice.pitch.set_value(midi_hz(midi as f32));
        self.live_voice.gate.set_value(1.0);
        self.live_off = (self.sr * 0.4) as usize;
    }

    pub fn live_kit(&mut self, piece: KitPiece) {
        self.push_playhead(PlayKind::Kit(piece), 0.9);
    }

    pub fn live_sample(&mut self, idx: usize) {
        self.push_playhead(PlayKind::Sample(idx), 0.9);
    }

    /// Live cutoff control (mixer slider): no graph rebuild needed.
    pub fn set_channel_cutoff(&mut self, idx: usize, v: f32) {
        if let Some(pool) = self.voices.get(idx) {
            for voice in pool.iter() {
                voice.cutoff.set_value(v);
            }
        }
    }

    /// Scope buffer in chronological order for the UI.
    pub fn scope_ordered(&self) -> Vec<f32> {
        let n = self.scope.len();
        (0..n)
            .map(|k| self.scope[(self.scope_pos + k) % n])
            .collect()
    }

    fn pattern_at_tick(&self, song: &Song, abs_tick: u64) -> Option<usize> {
        if self.song_mode {
            let bar = (abs_tick / TICKS_PATTERN as u64) as usize;
            let p = *song.arrangement.get(bar)?;
            if p < 0 { None } else { Some(p as usize) }
        } else {
            Some(self.pattern)
        }
    }

    pub fn process(&mut self, song: &Song, out_l: &mut [f32], out_r: &mut [f32]) {
        self.ensure_channels(song);
        let n = out_l.len().min(out_r.len());
        if n == 0 {
            return;
        }
        let spt = (self.sr * 60.0 / song.bpm.max(30.0) / 16.0).max(1.0) as f64;

        // Collect this block's events on the integer sample clock (no float drift).
        let mut events: Vec<(usize, SongEvent)> = Vec::new();
        if self.playing {
            let k_start = (self.sample_pos as f64 / spt).ceil() as u64;
            let k_end = ((self.sample_pos + n as u64) as f64 / spt).ceil() as u64;
            for k in k_start..k_end {
                let fire = (k as f64 * spt - self.sample_pos as f64).round() as usize;
                let fire = fire.min(n.saturating_sub(1));
                let in_pat = (k % TICKS_PATTERN as u64) as u32;
                if let Some(pat) = self.pattern_at_tick(song, k) {
                    self.cur_pat = pat;
                    self.cur_tick = in_pat;
                    let Some(pdata) = song.patterns.get(pat) else {
                        continue;
                    };
                    for (ci, _ch) in song.channels.iter().enumerate() {
                        if let Some(pts) = pdata.auto_vol.get(ci)
                            && let Some(v) = eval_auto(pts, in_pat)
                            && ci < self.auto_vol.len()
                        {
                            self.auto_vol[ci] = v.clamp(0.0, 1.5);
                        }
                        if let Some(pts) = pdata.auto_cut.get(ci)
                            && let Some(v) = eval_auto(pts, in_pat)
                            && ci < self.voices.len()
                        {
                            let v = v.clamp(40.0, 18000.0);
                            for voice in self.voices[ci].iter() {
                                voice.cutoff.set_value(v);
                            }
                        }
                        if let Some(notes) = pdata.notes.get(ci) {
                            for note in notes.iter().filter(|nn| nn.tick == in_pat) {
                                let mut off = fire;
                                if in_pat % 8 == 4 {
                                    off = (off as f64
                                        + song.swing.clamp(0.0, 0.75) as f64 * 2.0 * spt)
                                        as usize;
                                }
                                events.push((
                                    off.min(n.saturating_sub(1)),
                                    SongEvent::NoteOn {
                                        ch: ci,
                                        midi: note.midi,
                                        vel: note.vel,
                                        len_ticks: note.len.max(1),
                                    },
                                ));
                            }
                        }
                    }
                    if song.metronome && in_pat.is_multiple_of(16) {
                        events.push((
                            fire,
                            SongEvent::Click {
                                accent: in_pat == 0,
                            },
                        ));
                    }
                }
            }
            events.sort_by_key(|(o, _)| *o);
        }

        let mut ei = 0;
        for i in 0..n {
            while ei < events.len() && events[ei].0 <= i {
                match events[ei].1 {
                    SongEvent::NoteOn {
                        ch,
                        midi,
                        vel,
                        len_ticks,
                    } => self.fire_note(song, ch, midi, vel, len_ticks, spt),
                    SongEvent::Click { accent } => {
                        if self.metro_heads.len() < 8 {
                            self.metro_heads.push(MetroHead { accent, pos: 0 });
                        }
                    }
                }
                ei += 1;
            }
            // Envelopes always tick so live preview works while stopped.
            self.tick_gates();
            if self.live_off > 0 {
                self.live_off -= 1;
                if self.live_off == 0 {
                    self.live_voice.gate.set_value(0.0);
                }
            }
            let (l, r) = self.render_sample(song);
            out_l[i] = l;
            out_r[i] = r;
        }
        if self.playing {
            self.sample_pos += n as u64;
            self.tick_pos = self.sample_pos as f64 / spt;
            if self.song_mode && !song.arrangement.is_empty() {
                let total = song.arrangement.len() as u64 * TICKS_PATTERN as u64;
                let end_sample = (total as f64 * spt) as u64;
                if self.sample_pos >= end_sample {
                    if song.loop_song && !self.ignore_loop {
                        self.sample_pos = 0;
                    } else {
                        self.playing = false;
                    }
                }
            }
        }
    }

    fn tick_gates(&mut self) {
        for pool in self.voices.iter_mut() {
            for v in pool.iter_mut() {
                if v.off > 0 {
                    v.off -= 1;
                    if v.off == 0 {
                        v.gate.set_value(0.0);
                    }
                }
            }
        }
    }

    fn fire_note(&mut self, song: &Song, ch: usize, midi: u8, vel: f32, len_ticks: u32, spt: f64) {
        let Some(channel) = song.channels.get(ch) else {
            return;
        };
        let solo_any = song.channels.iter().any(|c| c.solo);
        if solo_any && !channel.solo {
            return;
        }
        if !solo_any && channel.mute {
            return;
        }
        let off = ((len_ticks as f64 * spt) as usize).max(1);
        match channel.kind {
            ChannelKind::Synth { .. } => {
                if ch < self.voices.len()
                    && let Some(v) = self.alloc_voice(ch)
                {
                    v.pitch.set_value(midi_hz(midi as f32));
                    v.gate.set_value(1.0);
                    v.vel = vel.clamp(0.05, 1.0);
                    v.off = off;
                }
            }
            ChannelKind::Kit { piece } => {
                self.push_playhead(PlayKind::Kit(piece), vel);
            }
            ChannelKind::Sampler { sample } => {
                if let Some(idx) = sample {
                    self.push_playhead(PlayKind::Sample(idx), vel);
                }
            }
        }
    }

    fn render_sample(&mut self, song: &Song) -> (f32, f32) {
        let mut l = 0.0f32;
        let mut r = 0.0f32;
        let mut send = 0.0f32;

        for (ci, channel) in song.channels.iter().enumerate() {
            let vol_auto = self.auto_vol.get(ci).copied().unwrap_or(1.0);
            let fader = channel.volume * vol_auto;
            let (lg, rg) = pan_gains(channel.pan, fader);
            let mut cl = 0.0f32;
            let mut cr = 0.0f32;
            if ci < self.voices.len() {
                for v in self.voices[ci].iter_mut() {
                    let (vl, vr) = v.unit.get_stereo();
                    cl += vl * lg * v.vel;
                    cr += vr * rg * v.vel;
                }
            }
            l += cl;
            r += cr;
            send += (cl + cr) * 0.5 * (channel.delay_send + channel.reverb_send);
            if ci < self.ch_peaks.len() {
                let p = cl.abs().max(cr.abs());
                self.ch_peaks[ci] = (self.ch_peaks[ci] * 0.94).max(p);
            }
        }
        {
            let (vl, vr) = self.live_voice.unit.get_stereo();
            l += vl * 0.8;
            r += vr * 0.8;
        }
        // PCM playheads (kit / sampler), linear-interpolated.
        let mut i = 0;
        while i < self.playheads.len() {
            let done = {
                let h = &mut self.playheads[i];
                let (data, ratio, sr_ok) = match h.kind {
                    PlayKind::Kit(p) => {
                        let b = match p {
                            KitPiece::Kick => &self.kit[0],
                            KitPiece::Snare => &self.kit[1],
                            KitPiece::Clap => &self.kit[2],
                            KitPiece::Hat => &self.kit[3],
                        };
                        (b.as_slice(), 1.0, true)
                    }
                    PlayKind::Sample(idx) => match song.samples.get(idx) {
                        Some(s) if !s.data.is_empty() && s.sr > 0.0 => {
                            (s.data.as_slice(), s.sr / self.sr, true)
                        }
                        _ => (&[][..], 1.0, false),
                    },
                };
                if !sr_ok || (h.pos as usize) >= data.len() {
                    true
                } else {
                    let p0 = h.pos as usize;
                    let frac = h.pos - p0 as f32;
                    let v0 = data[p0];
                    let v1 = data.get(p0 + 1).copied().unwrap_or(0.0);
                    let v = (v0 + (v1 - v0) * frac) * h.gain;
                    l += v * 0.9;
                    r += v * 0.9;
                    send += v * 0.2;
                    h.pos += ratio;
                    false
                }
            };
            if done {
                self.playheads.swap_remove(i);
            } else {
                i += 1;
            }
        }
        // Metronome overlay.
        let mut i = 0;
        while i < self.metro_heads.len() {
            let done = {
                let h = &mut self.metro_heads[i];
                let buf = if h.accent {
                    &self.metro_acc
                } else {
                    &self.metro_reg
                };
                if h.pos >= buf.len() {
                    true
                } else {
                    let v = buf[h.pos] * 0.5;
                    l += v;
                    r += v;
                    h.pos += 1;
                    false
                }
            };
            if done {
                self.metro_heads.swap_remove(i);
            } else {
                i += 1;
            }
        }

        // Sends: manual stereo delay + fundsp Schroeder reverb.
        let (dl, dr) = self.delay.process(self.sr, song.delay_time, send, send);
        let (rl, rr) = self.reverb.filter_stereo(send, send);
        l += dl * song.delay_mix + rl * song.reverb_mix;
        r += dr * song.delay_mix + rr * song.reverb_mix;

        l *= song.master_vol * 0.22;
        r *= song.master_vol * 0.22;
        l = l.tanh();
        r = r.tanh();
        self.peak = (self.peak * 0.9995).max(l.abs().max(r.abs()));
        self.scope_tick += 1;
        if self.scope_tick.is_multiple_of(4) {
            self.scope[self.scope_pos] = (l + r) * 0.5;
            self.scope_pos = (self.scope_pos + 1) % self.scope.len();
        }
        (l, r)
    }

    pub fn reset_transport(&mut self) {
        self.sample_pos = 0;
        self.tick_pos = 0.0;
        self.cur_tick = 0;
        for pool in self.voices.iter_mut() {
            for v in pool.iter_mut() {
                v.gate.set_value(0.0);
                v.off = 0;
            }
        }
        self.live_voice.gate.set_value(0.0);
        self.live_off = 0;
        self.playheads.clear();
        self.metro_heads.clear();
        for p in self.ch_peaks.iter_mut() {
            *p = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------
// App state + cpal backend
// ---------------------------------------------------------------------------

pub struct AppState {
    pub song: Song,
    pub rt: EngineRt,
}

impl AppState {
    pub fn new() -> Self {
        let song = crate::song::demo_song();
        let mut rt = EngineRt::new(44100.0);
        rt.ensure_channels(&song);
        Self { song, rt }
    }
}

/// Load a file into the song's sample bank (deduplicated by path).
pub fn sample_into_song(s: &mut AppState, path: &str) -> Result<usize, String> {
    if path.trim().is_empty() {
        return Err("empty path".into());
    }
    if let Some(idx) = s.song.samples.iter().position(|x| x.path == path) {
        return Ok(idx);
    }
    let (data, sr) = load_sample_file(path)?;
    let name = std::path::Path::new(path)
        .file_name()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    s.song.samples.push(crate::song::SampleRef {
        name,
        path: path.into(),
        data,
        sr,
    });
    Ok(s.song.samples.len() - 1)
}

pub fn start_audio(state: Arc<Mutex<AppState>>) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let supported = device.default_output_config().ok()?;
    let sr = supported.sample_rate() as f64;
    {
        if let Ok(mut s) = state.lock() {
            let AppState { song, rt } = &mut *s;
            rt.set_sample_rate(sr as f32, song);
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
                    move |data: &mut [f32], _| audio_callback(&st, data, channels),
                    err_fn,
                    None,
                )
                .ok(),
            cpal::SampleFormat::I16 => device
                .build_output_stream(
                    cfg,
                    move |data: &mut [i16], _| {
                        audio_callback(&st, data, channels);
                    },
                    err_fn,
                    None,
                )
                .ok(),
            cpal::SampleFormat::U16 => device
                .build_output_stream(
                    cfg,
                    move |data: &mut [u16], _| {
                        audio_callback(&st, data, channels);
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

/// Render into f32 staging, then convert per format.
fn audio_callback<T>(st: &Arc<Mutex<AppState>>, data: &mut [T], channels: usize)
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    if channels == 0 {
        return;
    }
    let frames = data.len() / channels;
    thread_local! {
        static ST: std::cell::RefCell<(Vec<f32>, Vec<f32>)> =
            const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
    }
    ST.with(|cell| {
        let (bl, br) = &mut *cell.borrow_mut();
        bl.resize(frames, 0.0);
        br.resize(frames, 0.0);
        if let Ok(mut s) = st.try_lock() {
            let AppState { song, rt } = &mut *s;
            rt.process(song, &mut bl[..], &mut br[..]);
        } else {
            bl.fill(0.0);
            br.fill(0.0);
        }
        for (f, chunk) in data.chunks_mut(channels).enumerate() {
            for (c, x) in chunk.iter_mut().enumerate() {
                let s = if c % 2 == 0 { bl[f] } else { br[f] };
                *x = T::from_sample(s);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test: a fresh engine (all gates closed) must be bit-finite.
    /// A zeroed filter default once NaN'd the SVF and poisoned the mix bus.
    #[test]
    fn fresh_engine_at_rest_is_finite() {
        let song = crate::song::demo_song();
        let mut rt = EngineRt::new(44100.0);
        rt.ensure_channels(&song);
        for _ in 0..44100 {
            let (l, r) = rt.render_sample(&song);
            assert!(l.is_finite() && r.is_finite(), "NaN at rest");
        }
    }

    #[test]
    fn scheduler_fires_demo_kick() {
        let song = crate::song::demo_song();
        let mut rt = EngineRt::new(44100.0);
        rt.ensure_channels(&song);
        assert_eq!(rt.voices.len(), song.channels.len());
        rt.song_mode = true;
        rt.playing = true;
        let mut l = vec![0.0; 44100];
        let mut r = vec![0.0; 44100];
        rt.process(&song, &mut l, &mut r);
        assert!(rt.tick_pos > 10.0, "sequencer did not advance");
        let peak = l
            .iter()
            .chain(r.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        assert!(peak > 0.01, "silent engine, peak={peak}");
        assert!(
            l.iter().chain(r.iter()).all(|x| x.is_finite()),
            "NaN in output"
        );
    }
}
