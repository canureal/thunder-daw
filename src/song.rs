//! Song model: FL-style global channels, patterns of notes + automation,
//! playlist arrangement, JSON save/load.

use serde::{Deserialize, Serialize};

pub const TICKS_PER_STEP: u32 = 4; // 64th-note resolution
pub const STEPS: usize = 16;
pub const TICKS_PATTERN: u32 = 64; // one 4/4 bar
pub const N_PATTERNS: usize = 8;
pub const SAVE_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveKind {
    Sine,
    Triangle,
    Saw,
    Square,
    Noise,
}

impl WaveKind {
    pub fn all() -> [WaveKind; 5] {
        use WaveKind::*;
        [Sine, Triangle, Saw, Square, Noise]
    }
    pub fn name(&self) -> &'static str {
        match self {
            WaveKind::Sine => "Sine",
            WaveKind::Triangle => "Tri",
            WaveKind::Saw => "Saw",
            WaveKind::Square => "Square",
            WaveKind::Noise => "Noise",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KitPiece {
    Kick,
    Snare,
    Clap,
    Hat,
}

impl KitPiece {
    pub fn name(&self) -> &'static str {
        match self {
            KitPiece::Kick => "Kick",
            KitPiece::Snare => "Snare",
            KitPiece::Clap => "Clap",
            KitPiece::Hat => "Hat",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelKind {
    Synth { wave: WaveKind },
    Kit { piece: KitPiece },
    Sampler { sample: Option<usize> },
}

impl ChannelKind {
    pub fn label(&self) -> String {
        match self {
            ChannelKind::Synth { wave } => format!("Synth/{}", wave.name()),
            ChannelKind::Kit { piece } => format!("Kit/{}", piece.name()),
            ChannelKind::Sampler { .. } => "Sampler".to_string(),
        }
    }
    pub fn is_pitched(&self) -> bool {
        matches!(
            self,
            ChannelKind::Synth { .. } | ChannelKind::Sampler { .. }
        )
    }
}

/// One user sample. PCM is reloaded from `path` on project load
/// (never stored in the JSON).
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SampleRef {
    pub name: String,
    pub path: String,
    #[serde(skip)]
    pub data: Vec<f32>,
    #[serde(skip)]
    pub sr: f32,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Note {
    pub tick: u32,
    pub len: u32,
    pub midi: u8,
    pub vel: f32,
}

impl Note {
    pub fn new(tick: u32, len: u32, midi: u8, vel: f32) -> Self {
        Self {
            tick: tick.min(TICKS_PATTERN - 1),
            len: len.max(1),
            midi: midi.clamp(21, 108),
            vel: vel.clamp(0.05, 1.0),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct AutoPoint {
    pub tick: u32,
    pub value: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub kind: ChannelKind,
    pub midi_base: u8,
    pub volume: f32,
    pub pan: f32,
    pub cutoff: f32,
    pub q: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub mute: bool,
    pub solo: bool,
    pub delay_send: f32,
    pub reverb_send: f32,
}

impl Default for ChannelKind {
    fn default() -> Self {
        ChannelKind::Synth {
            wave: WaveKind::Saw,
        }
    }
}

/// Everything needed to build one fundsp voice, copied out of a [`Channel`].
/// Keeps voice construction honest instead of passing 6 loose DSP arguments.
#[derive(Clone, Copy)]
pub struct VoiceSpec {
    pub wave: WaveKind,
    pub q: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Channel {
    pub fn voice_spec(&self) -> VoiceSpec {
        VoiceSpec {
            wave: match self.kind {
                ChannelKind::Synth { wave } => wave,
                // Kit/sampler channels still get a pool so live preview works.
                _ => WaveKind::Sine,
            },
            q: self.q,
            attack: self.attack,
            decay: self.decay,
            sustain: self.sustain,
            release: self.release,
        }
    }
}

impl Default for Channel {
    /// Sane DSP defaults: a zeroed filter (cutoff 0 / Q 0) NaNs fundsp's
    /// SVF and poisons the whole mix bus, so defaults must be playable.
    fn default() -> Self {
        Self {
            name: "Channel".into(),
            kind: ChannelKind::default(),
            midi_base: 60,
            volume: 0.8,
            pan: 0.0,
            cutoff: 2000.0,
            q: 1.0,
            attack: 0.005,
            decay: 0.1,
            sustain: 0.8,
            release: 0.1,
            mute: false,
            solo: false,
            delay_send: 0.0,
            reverb_send: 0.15,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Pattern {
    /// One note list per channel (channel indices align with Song::channels).
    pub notes: Vec<Vec<Note>>,
    pub auto_vol: Vec<Vec<AutoPoint>>,
    pub auto_cut: Vec<Vec<AutoPoint>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Song {
    pub save_version: u32,
    pub title: String,
    pub bpm: f32,
    pub swing: f32,
    pub master_vol: f32,
    pub metronome: bool,
    pub loop_song: bool,
    pub delay_time: f32,
    pub delay_mix: f32,
    pub reverb_mix: f32,
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
    /// One entry per bar: pattern index, or -1 for silence.
    pub arrangement: Vec<i32>,
    pub samples: Vec<SampleRef>,
}

impl Song {
    /// Step-grid view derived from notes (single source of truth = notes).
    pub fn step_active(&self, pat: usize, ch: usize, step: usize) -> bool {
        let t0 = step as u32 * TICKS_PER_STEP;
        self.patterns
            .get(pat)
            .and_then(|p| p.notes.get(ch))
            .map(|ns| {
                ns.iter()
                    .any(|n| n.tick >= t0 && n.tick < t0 + TICKS_PER_STEP)
            })
            .unwrap_or(false)
    }

    /// Toggle a step: remove notes starting in the step window, else add base note.
    pub fn toggle_step(&mut self, pat: usize, ch: usize, step: usize) {
        let t0 = step as u32 * TICKS_PER_STEP;
        let base = self.channels.get(ch).map(|c| c.midi_base).unwrap_or(60);
        if let Some(notes) = self.patterns.get_mut(pat).and_then(|p| p.notes.get_mut(ch)) {
            if notes
                .iter()
                .any(|n| n.tick >= t0 && n.tick < t0 + TICKS_PER_STEP)
            {
                notes.retain(|n| n.tick < t0 || n.tick >= t0 + TICKS_PER_STEP);
            } else {
                notes.push(Note::new(t0, TICKS_PER_STEP, base, 0.9));
                notes.sort_by_key(|n| n.tick);
            }
        }
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load(path: &str) -> Result<Song, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut song: Song = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if song.save_version != SAVE_VERSION {
            return Err(format!("unsupported project version {}", song.save_version));
        }
        // Normalize lengths so old/short files still work.
        song.patterns.resize_with(N_PATTERNS, Pattern::default);
        let nch = song.channels.len();
        for p in &mut song.patterns {
            p.notes.resize_with(nch, Vec::new);
            p.auto_vol.resize_with(nch, Vec::new);
            p.auto_cut.resize_with(nch, Vec::new);
        }
        Ok(song)
    }
}

/// Linear automation eval. Returns None when there are no points.
pub fn eval_auto(points: &[AutoPoint], tick: u32) -> Option<f32> {
    if points.is_empty() {
        return None;
    }
    let mut pts = points.to_vec();
    pts.sort_by_key(|p| p.tick);
    if tick <= pts[0].tick {
        return Some(pts[0].value);
    }
    for w in pts.windows(2) {
        if tick >= w[0].tick && tick <= w[1].tick {
            let span = (w[1].tick - w[0].tick).max(1) as f32;
            let t = (tick - w[0].tick) as f32 / span;
            return Some(w[0].value + (w[1].value - w[0].value) * t);
        }
    }
    Some(pts.last().unwrap().value)
}

// ---------------------------------------------------------------------------
// Note editing tools (pure functions, unit-testable)
// ---------------------------------------------------------------------------

pub fn tool_quantize(notes: &mut [Note], grid: u32) {
    for n in notes.iter_mut() {
        n.tick = (n.tick + grid / 2) / grid * grid;
        n.tick = n.tick.min(TICKS_PATTERN - 1);
    }
    notes.sort_by_key(|n| n.tick);
}

pub fn tool_humanize(notes: &mut [Note], seed: u64) {
    let mut x = seed | 1;
    let mut rnd = move || {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(37);
        (x >> 33) as u32
    };
    for n in notes.iter_mut() {
        let dt = (rnd() % 5) as i32 - 2;
        n.tick = (n.tick as i32 + dt).clamp(0, TICKS_PATTERN as i32 - 1) as u32;
        let dv = ((rnd() % 17) as f32 - 8.0) / 100.0;
        n.vel = (n.vel + dv).clamp(0.05, 1.0);
    }
    notes.sort_by_key(|n| n.tick);
}

/// Spread chord clusters (notes within 2 ticks) by `amount` ticks each.
pub fn tool_strum(notes: &mut [Note], amount: u32) {
    notes.sort_by_key(|n| n.tick);
    let mut i = 0;
    while i < notes.len() {
        let t = notes[i].tick;
        let mut j = i;
        while j < notes.len() && notes[j].tick.saturating_sub(t) <= 2 {
            j += 1;
        }
        for (k, n) in notes[i..j].iter_mut().enumerate() {
            n.tick = (t + k as u32 * amount).min(TICKS_PATTERN - 1);
        }
        i = j.max(i + 1);
    }
}

/// Rewrite notes as an ascending 16th-note arpeggio cycling their pitches.
pub fn tool_arpeggiate(notes: &mut Vec<Note>) {
    if notes.is_empty() {
        return;
    }
    let mut pitches: Vec<u8> = notes.iter().map(|n| n.midi).collect();
    pitches.sort_unstable();
    pitches.dedup();
    notes.clear();
    for (i, p) in pitches.iter().cycle().take(STEPS).enumerate() {
        notes.push(Note::new(
            i as u32 * TICKS_PER_STEP,
            TICKS_PER_STEP,
            *p,
            0.85,
        ));
    }
}

pub fn tool_chord(tick: u32, root: u8, minor: bool) -> Vec<Note> {
    let third = if minor { 3 } else { 4 };
    [0, third, 7]
        .iter()
        .map(|iv| Note::new(tick, TICKS_PER_STEP * 4, root.saturating_add(*iv), 0.85))
        .collect()
}

// ---------------------------------------------------------------------------
// Demo song
// ---------------------------------------------------------------------------

fn synth_ch(
    name: &str,
    wave: WaveKind,
    base: u8,
    vol: f32,
    pan: f32,
    cutoff: f32,
    adsr: [f32; 4],
) -> Channel {
    Channel {
        name: name.into(),
        kind: ChannelKind::Synth { wave },
        midi_base: base,
        volume: vol,
        pan,
        cutoff,
        q: 1.0,
        attack: adsr[0],
        decay: adsr[1],
        sustain: adsr[2],
        release: adsr[3],
        delay_send: 0.0,
        reverb_send: 0.15,
        ..Default::default()
    }
}

fn kit_ch(name: &str, piece: KitPiece, vol: f32, pan: f32) -> Channel {
    Channel {
        name: name.into(),
        kind: ChannelKind::Kit { piece },
        midi_base: 60,
        volume: vol,
        pan,
        cutoff: 12000.0,
        delay_send: 0.0,
        reverb_send: 0.1,
        ..Default::default()
    }
}

fn steps(pat: &mut Pattern, ch: usize, on: &[usize], midi: u8) {
    for &s in on {
        pat.notes[ch].push(Note::new(
            s as u32 * TICKS_PER_STEP,
            TICKS_PER_STEP,
            midi,
            0.9,
        ));
    }
}

pub fn demo_song() -> Song {
    let channels = vec![
        kit_ch("Kick", KitPiece::Kick, 1.0, 0.0),
        kit_ch("Snare", KitPiece::Snare, 0.8, 0.0),
        kit_ch("Clap", KitPiece::Clap, 0.6, 0.15),
        kit_ch("Hat", KitPiece::Hat, 0.5, 0.2),
        synth_ch(
            "Bass",
            WaveKind::Saw,
            33,
            0.85,
            -0.1,
            520.0,
            [0.008, 0.1, 0.7, 0.1],
        ),
        synth_ch(
            "Lead",
            WaveKind::Square,
            69,
            0.55,
            0.0,
            2400.0,
            [0.01, 0.12, 0.6, 0.12],
        ),
        synth_ch(
            "Pad",
            WaveKind::Triangle,
            57,
            0.5,
            0.0,
            1200.0,
            [0.2, 0.3, 0.7, 0.4],
        ),
        Channel {
            name: "Sampler".into(),
            kind: ChannelKind::Sampler { sample: None },
            midi_base: 60,
            volume: 0.9,
            reverb_send: 0.1,
            ..Default::default()
        },
    ];
    let nch = channels.len();
    let mut patterns: Vec<Pattern> = (0..N_PATTERNS)
        .map(|_| Pattern {
            notes: vec![Vec::new(); nch],
            auto_vol: vec![Vec::new(); nch],
            auto_cut: vec![Vec::new(); nch],
        })
        .collect();

    // Pattern 0: full groove.
    steps(&mut patterns[0], 0, &[0, 4, 8, 12], 36);
    steps(&mut patterns[0], 1, &[4, 12], 40);
    steps(&mut patterns[0], 3, &[0, 2, 4, 6, 8, 10, 12, 14], 42);
    for (t, m) in [(0, 33), (16, 33), (32, 29), (48, 31)] {
        patterns[0].notes[4].push(Note::new(t, 8, m, 0.9));
    }
    for (t, l, m, v) in [
        (0, 8, 69, 0.9),
        (8, 4, 72, 0.9),
        (12, 4, 74, 0.85),
        (16, 8, 76, 0.9),
        (24, 4, 74, 0.85),
        (28, 4, 72, 0.8),
        (32, 8, 69, 0.9),
        (40, 4, 67, 0.85),
        (44, 4, 69, 0.9),
        (48, 4, 64, 0.85),
        (52, 4, 67, 0.8),
        (56, 8, 69, 0.9),
    ] {
        patterns[0].notes[5].push(Note::new(t, l, m, v));
    }
    patterns[0].notes[6].push(Note::new(0, 32, 57, 0.7));
    patterns[0].notes[6].push(Note::new(0, 32, 60, 0.6));
    patterns[0].notes[6].push(Note::new(0, 32, 64, 0.6));
    patterns[0].notes[6].push(Note::new(32, 32, 55, 0.7));
    patterns[0].notes[6].push(Note::new(32, 32, 59, 0.6));
    patterns[0].notes[6].push(Note::new(32, 32, 62, 0.6));

    // Pattern 1: drums only.
    steps(&mut patterns[1], 0, &[0, 4, 8, 12], 36);
    steps(&mut patterns[1], 1, &[4, 12], 40);
    steps(&mut patterns[1], 2, &[12], 39);
    steps(&mut patterns[1], 3, &[0, 2, 4, 6, 8, 10, 12, 14], 42);

    // Pattern 2: bass + lead, sparse drums.
    steps(&mut patterns[2], 0, &[0, 8], 36);
    steps(&mut patterns[2], 3, &[2, 6, 10, 14], 42);
    for (t, m) in [(0, 33), (16, 36), (32, 31), (48, 29)] {
        patterns[2].notes[4].push(Note::new(t, 8, m, 0.9));
    }
    for (t, l, m, v) in [
        (0, 8, 81, 0.9),
        (16, 8, 79, 0.85),
        (32, 8, 76, 0.9),
        (48, 8, 74, 0.9),
    ] {
        patterns[2].notes[5].push(Note::new(t, l, m, v));
    }

    // Pattern 3: break (hat + pad).
    steps(&mut patterns[3], 3, &[0, 2, 4, 6, 8, 10, 12, 14, 15], 42);
    patterns[3].notes[6].push(Note::new(0, 32, 57, 0.7));
    patterns[3].notes[6].push(Note::new(0, 32, 60, 0.6));
    patterns[3].notes[6].push(Note::new(0, 32, 64, 0.6));
    patterns[3].notes[6].push(Note::new(32, 32, 53, 0.7));
    patterns[3].notes[6].push(Note::new(32, 32, 57, 0.6));
    patterns[3].notes[6].push(Note::new(32, 32, 60, 0.6));

    Song {
        save_version: SAVE_VERSION,
        title: "thunder demo".into(),
        bpm: 128.0,
        swing: 0.0,
        master_vol: 0.9,
        metronome: false,
        loop_song: true,
        delay_time: 0.32,
        delay_mix: 0.18,
        reverb_mix: 0.22,
        channels,
        patterns,
        arrangement: vec![0, 0, 1, 2, 0, 3, 2, 1],
        samples: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_interp_midpoint() {
        let pts = vec![
            AutoPoint {
                tick: 0,
                value: 0.0,
            },
            AutoPoint {
                tick: 10,
                value: 1.0,
            },
        ];
        assert!((eval_auto(&pts, 5).unwrap() - 0.5).abs() < 1e-6);
        assert!((eval_auto(&pts, 0).unwrap() - 0.0).abs() < 1e-6);
        assert!((eval_auto(&pts, 99).unwrap() - 1.0).abs() < 1e-6);
        assert!(eval_auto(&[], 5).is_none());
    }

    #[test]
    fn quantize_snaps_to_grid() {
        let mut notes = vec![Note::new(5, 4, 60, 0.9), Note::new(13, 4, 62, 0.9)];
        tool_quantize(&mut notes, 4);
        assert_eq!(notes[0].tick, 4);
        assert_eq!(notes[1].tick, 12);
    }

    #[test]
    fn step_toggle_roundtrip() {
        let mut song = demo_song();
        assert!(song.step_active(0, 0, 0));
        song.toggle_step(0, 0, 0);
        assert!(!song.step_active(0, 0, 0));
        song.toggle_step(0, 0, 0);
        assert!(song.step_active(0, 0, 0));
    }

    #[test]
    fn save_load_roundtrip() {
        let song = demo_song();
        let path = std::env::temp_dir().join("thunder-test.json");
        let path = path.to_str().unwrap();
        song.save(path).unwrap();
        let back = Song::load(path).unwrap();
        assert_eq!(back.title, song.title);
        assert_eq!(back.channels.len(), song.channels.len());
        assert_eq!(back.patterns.len(), N_PATTERNS);
        assert_eq!(
            back.patterns[0].notes[5].len(),
            song.patterns[0].notes[5].len()
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn arp_produces_16_steps() {
        let mut notes = vec![Note::new(0, 4, 60, 0.9), Note::new(16, 4, 67, 0.9)];
        tool_arpeggiate(&mut notes);
        assert_eq!(notes.len(), STEPS);
        assert!(notes.windows(2).all(|w| w[0].tick <= w[1].tick));
    }
}
