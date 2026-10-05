//! All egui panels: transport, channel rack, piano roll + automation,
//! playlist, mixer, sample browser.

use crate::engine::{AppState, load_sample_file, sample_into_song};
use crate::song::{
    AutoPoint, ChannelKind, KitPiece, N_PATTERNS, Note, STEPS, Song, WaveKind, eval_auto,
    tool_arpeggiate, tool_chord, tool_humanize, tool_quantize, tool_strum,
};
use eframe::egui;
use std::time::Instant;

// ---------------------------------------------------------------------------
// UI state (not part of the saved song)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScaleKind {
    Chromatic,
    Major,
    Minor,
    PentMaj,
    PentMin,
}

impl ScaleKind {
    fn name(&self) -> &'static str {
        match self {
            ScaleKind::Chromatic => "Chromatic",
            ScaleKind::Major => "Major",
            ScaleKind::Minor => "Minor",
            ScaleKind::PentMaj => "Pent Maj",
            ScaleKind::PentMin => "Pent Min",
        }
    }
    fn contains(&self, pc: u8) -> bool {
        match self {
            ScaleKind::Chromatic => true,
            ScaleKind::Major => matches!(pc, 0 | 2 | 4 | 5 | 7 | 9 | 11),
            ScaleKind::Minor => matches!(pc, 0 | 2 | 3 | 5 | 7 | 8 | 10),
            ScaleKind::PentMaj => matches!(pc, 0 | 2 | 4 | 7 | 9),
            ScaleKind::PentMin => matches!(pc, 0 | 3 | 5 | 7 | 10),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChordMode {
    Off,
    Maj,
    Min,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AutoSel {
    Vol,
    Cut,
}

#[derive(Clone, Copy)]
enum DragKind {
    Move,
    Resize,
}

pub struct AppUi {
    pub sel_ch: usize,
    pub sel_pat: usize,
    piano_bot: u8,
    snap: u32,
    scale: ScaleKind,
    chord: ChordMode,
    sel_note: Option<usize>,
    drag: Option<(usize, DragKind, i32, i32)>,
    pending_add: Option<(u32, u8)>,
    auto_sel: AutoSel,
    auto_drag: Option<usize>,
    save_path: String,
    wav_path: String,
    mid_path: String,
    sample_path: String,
    browser_dir: String,
    browser_cache: Vec<String>,
    pub status: String,
    taps: Vec<Instant>,
}

impl Default for AppUi {
    fn default() -> Self {
        Self {
            sel_ch: 4,
            sel_pat: 0,
            piano_bot: 40,
            snap: 4,
            scale: ScaleKind::Major,
            chord: ChordMode::Off,
            sel_note: None,
            drag: None,
            pending_add: None,
            auto_sel: AutoSel::Vol,
            auto_drag: None,
            save_path: "thunder-daw.json".into(),
            wav_path: "mixdown.wav".into(),
            mid_path: "song.mid".into(),
            sample_path: String::new(),
            browser_dir: "samples".into(),
            browser_cache: Vec::new(),
            status: "ready".into(),
            taps: Vec::new(),
        }
    }
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn note_label(midi: u8) -> String {
    format!(
        "{}{}",
        NOTE_NAMES[(midi % 12) as usize],
        midi as i32 / 12 - 1
    )
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn sample_files(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if let Some(ext) = p.extension().and_then(|x| x.to_str())
                && matches!(
                    ext.to_lowercase().as_str(),
                    "wav" | "mp3" | "ogg" | "flac" | "aiff" | "aif"
                )
            {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

fn home_music_dir() -> String {
    std::env::var("HOME")
        .map(|h| format!("{h}/Music/samples"))
        .unwrap_or_else(|_| "samples".into())
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

pub fn transport(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    u.horizontal(|u| {
        let mut playing = s.rt.playing;
        if u.button(if playing { "⏹ Stop" } else { "▶ Play" })
            .clicked()
        {
            playing = !playing;
            s.rt.playing = playing;
        }
        if u.button("⏮ Reset").clicked() {
            s.rt.reset_transport();
        }
        // Song / pattern mode.
        let mut song_mode = s.rt.song_mode;
        if u.selectable_label(song_mode, "Song")
            .on_hover_text("Play the playlist arrangement")
            .clicked()
        {
            song_mode = true;
        }
        if u.selectable_label(!song_mode, "Pat")
            .on_hover_text("Loop one pattern")
            .clicked()
        {
            song_mode = false;
        }
        if song_mode != s.rt.song_mode {
            s.rt.song_mode = song_mode;
            s.rt.reset_transport();
        }
        if !song_mode {
            let mut pat = ui.sel_pat;
            egui::ComboBox::from_label("")
                .selected_text(format!("P{}", pat + 1))
                .show_ui(u, |u| {
                    for p in 0..N_PATTERNS {
                        u.selectable_value(&mut pat, p, format!("P{}", p + 1));
                    }
                });
            if pat != ui.sel_pat {
                ui.sel_pat = pat;
                s.rt.pattern = pat;
                s.rt.reset_transport();
            }
        }
        let mut bpm = s.song.bpm;
        u.add(egui::Slider::new(&mut bpm, 60.0..=200.0).text("BPM"));
        if (bpm - s.song.bpm).abs() > f32::EPSILON {
            s.song.bpm = bpm;
        }
        if u.button("Tap").clicked() {
            let now = Instant::now();
            ui.taps.push(now);
            ui.taps
                .retain(|t| now.duration_since(*t).as_secs_f32() < 2.5);
            if ui.taps.len() >= 3 {
                let diffs: Vec<f32> = ui
                    .taps
                    .windows(2)
                    .map(|w| w[1].duration_since(w[0]).as_secs_f32())
                    .collect();
                let avg = diffs.iter().sum::<f32>() / diffs.len() as f32;
                if avg > 0.15 && avg < 2.0 {
                    s.song.bpm = (60.0 / avg).clamp(60.0, 200.0);
                }
            }
        }
        let mut swing = s.song.swing;
        u.add(egui::Slider::new(&mut swing, 0.0..=0.6).text("Swing"));
        if (swing - s.song.swing).abs() > f32::EPSILON {
            s.song.swing = swing;
        }
        let mut metro = s.song.metronome;
        u.toggle_value(&mut metro, "Click");
        s.song.metronome = metro;
        let mut lp = s.song.loop_song;
        u.toggle_value(&mut lp, "Loop");
        s.song.loop_song = lp;
        let mut master = s.song.master_vol;
        u.add(egui::Slider::new(&mut master, 0.0..=1.5).text("Master"));
        s.song.master_vol = master;
        // Position readout.
        let tick = s.rt.tick_pos.max(0.0) as u64;
        u.label(format!(
            "{:02}:{:01}:{:02}",
            tick / 64 + 1,
            (tick % 64) / 16 + 1,
            tick % 16
        ));
        u.add(
            egui::ProgressBar::new(s.rt.peak.clamp(0.0, 1.0))
                .text(format!("{:.0}%", s.rt.peak * 100.0)),
        );
    });
    u.horizontal(|u| {
        u.label("Project:");
        u.add(egui::TextEdit::singleline(&mut ui.save_path).desired_width(180.0));
        if u.button("Save").clicked() {
            match s.song.save(&ui.save_path.clone()) {
                Ok(()) => ui.status = format!("saved {}", ui.save_path),
                Err(e) => ui.status = format!("save failed: {e}"),
            }
        }
        if u.button("Load").clicked() {
            match Song::load(&ui.save_path.clone()) {
                Ok(mut song) => {
                    let mut missing = 0;
                    for smp in song.samples.iter_mut() {
                        match load_sample_file(&smp.path) {
                            Ok((data, sr)) => {
                                smp.data = data;
                                smp.sr = sr;
                            }
                            Err(_) => missing += 1,
                        }
                    }
                    let sr = s.rt.sr;
                    s.song = song;
                    ui.sel_ch = ui.sel_ch.min(s.song.channels.len().saturating_sub(1));
                    ui.sel_pat = ui.sel_pat.min(N_PATTERNS - 1);
                    s.rt.pattern = ui.sel_pat;
                    s.rt.set_sample_rate(sr, &s.song);
                    ui.status = if missing == 0 {
                        format!("loaded {}", ui.save_path)
                    } else {
                        format!("loaded {} ({missing} samples missing)", ui.save_path)
                    };
                }
                Err(e) => ui.status = format!("load failed: {e}"),
            }
        }
        u.label("WAV:");
        u.add(egui::TextEdit::singleline(&mut ui.wav_path).desired_width(120.0));
        if u.button("Export").clicked() {
            match crate::export::export_wav(&s.song, &ui.wav_path.clone()) {
                Ok(m) => ui.status = m,
                Err(e) => ui.status = format!("wav export failed: {e}"),
            }
        }
        u.label("MIDI:");
        u.add(egui::TextEdit::singleline(&mut ui.mid_path).desired_width(100.0));
        if u.button("Export").clicked() {
            match crate::export::export_midi(&s.song, ui.sel_pat, &ui.mid_path.clone()) {
                Ok(m) => ui.status = m,
                Err(e) => ui.status = format!("midi export failed: {e}"),
            }
        }
    });
}

pub fn channel_rack(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    u.heading("Channel rack");
    let n = s.song.channels.len();
    for i in 0..n {
        u.horizontal(|u| {
            if u.selectable_label(ui.sel_ch == i, s.song.channels[i].name.clone())
                .clicked()
            {
                ui.sel_ch = i;
            }
            u.label(s.song.channels[i].kind.label());
            let mut m = s.song.channels[i].mute;
            if u.toggle_value(&mut m, "M").changed() {
                s.song.channels[i].mute = m;
            }
            let mut so = s.song.channels[i].solo;
            if u.toggle_value(&mut so, "S").changed() {
                s.song.channels[i].solo = so;
            }
            // Kind picker.
            match s.song.channels[i].kind.clone() {
                ChannelKind::Synth { wave } => {
                    let mut w = wave;
                    egui::ComboBox::from_id_salt(format!("wave{i}"))
                        .selected_text(w.name())
                        .show_ui(u, |u| {
                            for cand in WaveKind::all() {
                                u.selectable_value(&mut w, cand, cand.name());
                            }
                        });
                    if w != wave {
                        s.song.channels[i].kind = ChannelKind::Synth { wave: w };
                        s.rt.rebuild_channel(&s.song.clone(), i);
                    }
                }
                ChannelKind::Kit { piece } => {
                    let mut p = piece;
                    egui::ComboBox::from_id_salt(format!("kit{i}"))
                        .selected_text(p.name())
                        .show_ui(u, |u| {
                            for cand in [
                                KitPiece::Kick,
                                KitPiece::Snare,
                                KitPiece::Clap,
                                KitPiece::Hat,
                            ] {
                                u.selectable_value(&mut p, cand, cand.name());
                            }
                        });
                    if p != piece {
                        s.song.channels[i].kind = ChannelKind::Kit { piece: p };
                    }
                }
                ChannelKind::Sampler { sample } => {
                    let txt = sample
                        .and_then(|idx| s.song.samples.get(idx))
                        .map(|x| x.name.clone())
                        .unwrap_or_else(|| "(empty)".into());
                    u.label(txt);
                }
            }
            if s.song.channels[i].kind.is_pitched() {
                let mut base = s.song.channels[i].midi_base;
                u.add(egui::Slider::new(&mut base, 21..=108).text("base"));
                if base != s.song.channels[i].midi_base {
                    s.song.channels[i].midi_base = base;
                }
            }
            if u.button("♪").on_hover_text("Preview").clicked() {
                preview_channel(s, i);
            }
            // Step buttons.
            let playing_step = if s.rt.playing && ui.sel_pat == s.rt.cur_pat {
                Some((s.rt.cur_tick / 4) as usize)
            } else {
                None
            };
            for step in 0..STEPS {
                let active = s.song.step_active(ui.sel_pat, i, step);
                let mut btn =
                    egui::Button::new(format!("{}", step + 1)).min_size(egui::vec2(24.0, 24.0));
                if active && playing_step == Some(step) {
                    btn = btn.fill(egui::Color32::LIGHT_GREEN);
                } else if active {
                    btn = btn.fill(egui::Color32::DARK_GREEN);
                } else if playing_step == Some(step) {
                    btn = btn.fill(egui::Color32::from_rgb(90, 70, 20));
                } else if step.is_multiple_of(4) {
                    btn = btn.fill(egui::Color32::from_rgb(55, 55, 60));
                }
                if u.add(btn).clicked() {
                    s.song.toggle_step(ui.sel_pat, i, step);
                }
            }
        });
    }
    // Borrow dance: rebuild_channel needs &Song while holding &mut AppState.
    // Handled by cloning the song for the call above (cheap: notes only).
}

fn preview_channel(s: &mut AppState, i: usize) {
    match s.song.channels[i].kind.clone() {
        ChannelKind::Synth { .. } => s.rt.live_note(s.song.channels[i].midi_base),
        ChannelKind::Kit { piece } => s.rt.live_kit(piece),
        ChannelKind::Sampler { sample } => {
            if let Some(idx) = sample {
                s.rt.live_sample(idx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Piano roll
// ---------------------------------------------------------------------------

const PIANO_ROWS: u8 = 37;
const KEY_W: f32 = 52.0;

fn piano_midi_range(bot: u8) -> (u8, u8) {
    let top = (bot + PIANO_ROWS - 1).min(108);
    (bot.max(21), top)
}

#[allow(clippy::too_many_lines)]
pub fn piano_roll(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    let ch = ui.sel_ch.min(s.song.channels.len().saturating_sub(1));
    let pat = ui.sel_pat;
    u.horizontal(|u| {
        u.heading(format!(
            "Piano roll — {} / P{}",
            s.song.channels[ch].name,
            pat + 1
        ));
        if u.button("Quant")
            .on_hover_text("Snap starts to grid")
            .clicked()
        {
            if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                tool_quantize(notes, ui.snap);
            }
            ui.status = "quantized".into();
        }
        if u.button("Human")
            .on_hover_text("Timing + velocity feel")
            .clicked()
        {
            if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(42);
                tool_humanize(notes, seed);
            }
            ui.status = "humanized".into();
        }
        if u.button("Strum").on_hover_text("Spread chords").clicked() {
            if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                tool_strum(notes, 2);
            }
            ui.status = "strummed".into();
        }
        if u.button("Arp")
            .on_hover_text("16th arpeggio from pitches")
            .clicked()
        {
            if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                tool_arpeggiate(notes);
            }
            ui.status = "arpeggiated".into();
        }
        if u.button("Chord: off").clicked() {
            ui.chord = match ui.chord {
                ChordMode::Off => ChordMode::Maj,
                ChordMode::Maj => ChordMode::Min,
                ChordMode::Min => ChordMode::Off,
            };
        }
        let chord_txt = match ui.chord {
            ChordMode::Off => "Chord: off",
            ChordMode::Maj => "Chord: Maj",
            ChordMode::Min => "Chord: min",
        };
        u.label(chord_txt);
        if u.button("Clear").clicked()
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        {
            notes.clear();
        }
        if u.button("Oct−").clicked()
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        {
            for n in notes.iter_mut() {
                n.midi = n.midi.saturating_sub(12).max(21);
            }
        }
        if u.button("Oct+").clicked()
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        {
            for n in notes.iter_mut() {
                n.midi = (n.midi + 12).min(108);
            }
        }
    });
    u.horizontal(|u| {
        u.label("Snap:");
        egui::ComboBox::from_id_salt("snap")
            .selected_text(format!("1/{}", 64 / ui.snap.max(1)))
            .show_ui(u, |u| {
                for sn in [1u32, 2, 4] {
                    u.selectable_value(&mut ui.snap, sn, format!("1/{}", 64 / sn));
                }
            });
        u.label("Scale:");
        let mut sc = ui.scale;
        egui::ComboBox::from_id_salt("scale")
            .selected_text(sc.name())
            .show_ui(u, |u| {
                for cand in [
                    ScaleKind::Chromatic,
                    ScaleKind::Major,
                    ScaleKind::Minor,
                    ScaleKind::PentMaj,
                    ScaleKind::PentMin,
                ] {
                    u.selectable_value(&mut sc, cand, cand.name());
                }
            });
        ui.scale = sc;
        let mut bot = ui.piano_bot;
        u.add(egui::Slider::new(&mut bot, 21..=72).text("Octave"));
        ui.piano_bot = bot;
    });

    let (midi_lo, midi_hi) = piano_midi_range(ui.piano_bot);
    let rows = (midi_hi - midi_lo + 1) as usize;
    let avail_w = u.available_width().max(300.0);
    let cell_w = (avail_w - KEY_W) / 64.0;
    let cell_h = 9.0;
    let h = rows as f32 * cell_h;
    let (rect, resp) = u.allocate_exact_size(egui::vec2(avail_w, h), egui::Sense::click_and_drag());
    let painter = u.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(16));

    let tick_at = |x: f32| ((x - rect.min.x - KEY_W) / cell_w).floor() as i32;
    let midi_at = |y: f32| midi_hi as i32 - ((y - rect.min.y) / cell_h).floor() as i32;
    let x_of = |tick: u32| rect.min.x + KEY_W + tick as f32 * cell_w;
    let y_of = |midi: u8| rect.min.y + (midi_hi - midi) as f32 * cell_h;

    // Scale shading + key strip.
    for r in 0..rows {
        let midi = midi_hi - r as u8;
        let y = rect.min.y + r as f32 * cell_h;
        let in_scale = ui.scale.contains(midi % 12);
        let row_col = if !in_scale {
            egui::Color32::from_gray(22)
        } else if midi.is_multiple_of(12) {
            egui::Color32::from_rgb(34, 34, 40)
        } else {
            egui::Color32::from_gray(28)
        };
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.min.x + KEY_W, y),
                egui::vec2(rect.width() - KEY_W, cell_h),
            ),
            0.0,
            row_col,
        );
        let black = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(rect.min.x, y), egui::vec2(KEY_W, cell_h)),
            0.0,
            if black {
                egui::Color32::BLACK
            } else {
                egui::Color32::from_gray(200)
            },
        );
        if midi.is_multiple_of(12) {
            painter.text(
                egui::pos2(rect.min.x + 4.0, y + cell_h * 0.5),
                egui::Align2::LEFT_CENTER,
                note_label(midi),
                egui::FontId::proportional(9.0),
                if black {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                },
            );
        }
    }
    // Grid lines.
    for t in 0u32..=64u32 {
        let x = rect.min.x + KEY_W + t as f32 * cell_w;
        let strong = t.is_multiple_of(16) || t == 0;
        let mid = t.is_multiple_of(4);
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(
                1.0_f32,
                if strong {
                    egui::Color32::from_gray(90)
                } else if mid {
                    egui::Color32::from_gray(55)
                } else {
                    egui::Color32::from_gray(38)
                },
            ),
        );
    }
    // Ghost notes from other channels.
    if let Some(pdata) = s.song.patterns.get(pat) {
        for (ci, notes) in pdata.notes.iter().enumerate() {
            if ci == ch {
                continue;
            }
            for n in notes {
                if n.midi < midi_lo || n.midi > midi_hi {
                    continue;
                }
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x_of(n.tick), y_of(n.midi) + 1.0),
                        egui::pos2(x_of(n.tick + n.len), y_of(n.midi) + cell_h - 1.0),
                    ),
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(120, 120, 120, 60),
                );
            }
        }
    }
    // Notes.
    let notes_snapshot = s
        .song
        .patterns
        .get(pat)
        .and_then(|p| p.notes.get(ch))
        .cloned()
        .unwrap_or_default();
    for (idx, n) in notes_snapshot.iter().enumerate() {
        if n.midi < midi_lo || n.midi > midi_hi {
            continue;
        }
        let col = if Some(idx) == ui.sel_note {
            egui::Color32::YELLOW
        } else {
            let g = (120.0 + n.vel * 120.0) as u8;
            egui::Color32::from_rgb(60, g, 90)
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x_of(n.tick), y_of(n.midi) + 1.0),
                egui::pos2(x_of(n.tick + n.len), y_of(n.midi) + cell_h - 1.0),
            ),
            2.0,
            col,
        );
    }
    // Playhead.
    if s.rt.playing && s.rt.cur_pat == pat {
        let x = x_of(s.rt.cur_tick.min(63));
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GREEN),
        );
    }

    // Interactions.
    let hit_note = |tick: i32, midi: i32| -> Option<usize> {
        notes_snapshot.iter().rposition(|n| {
            tick >= n.tick as i32 && tick < (n.tick + n.len) as i32 && midi == n.midi as i32
        })
    };
    if resp.drag_started() {
        ui.pending_add = None;
        if let Some(pos) = resp.hover_pos() {
            let t = tick_at(pos.x);
            let m = midi_at(pos.y);
            if (0..64).contains(&t) && m >= midi_lo as i32 && m <= midi_hi as i32 {
                if let Some(idx) = hit_note(t, m) {
                    let n = &notes_snapshot[idx];
                    let right_edge = x_of(n.tick + n.len);
                    let kind = if pos.x > right_edge - 7.0 {
                        DragKind::Resize
                    } else {
                        DragKind::Move
                    };
                    ui.drag = Some((idx, kind, t - n.tick as i32, m - n.midi as i32));
                    ui.sel_note = Some(idx);
                } else {
                    let snap_t =
                        ((t / ui.snap.max(1) as i32) * ui.snap.max(1) as i32).clamp(0, 63) as u32;
                    ui.pending_add = Some((snap_t, m as u8));
                }
            }
        }
    }
    if resp.dragged() {
        if let Some((idx, kind, gt, gm)) = ui.drag.take() {
            if let Some(pos) = resp.hover_pos() {
                let t = tick_at(pos.x);
                let m = midi_at(pos.y);
                if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
                    && let Some(n) = notes.get_mut(idx)
                {
                    match kind {
                        DragKind::Move => {
                            n.tick = (t - gt).clamp(0, 63) as u32;
                            n.midi = (m - gm).clamp(21, 108) as u8;
                        }
                        DragKind::Resize => {
                            n.len = (t - n.tick as i32 + 1).clamp(1, 64) as u32;
                        }
                    }
                }
            }
            // Re-arm for continued dragging.
            if let Some(pos) = resp.hover_pos() {
                let _ = pos;
                ui.drag = Some((idx, kind, gt, gm));
            }
        }
    } else {
        ui.drag = None;
    }
    if resp.clicked()
        && let Some((t, m)) = ui.pending_add.take()
    {
        match ui.chord {
            ChordMode::Off => {
                if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                    notes.push(Note::new(t, ui.snap.max(1), m, 0.85));
                    notes.sort_by_key(|n| n.tick);
                    ui.sel_note = notes.iter().rposition(|n| n.tick == t && n.midi == m);
                }
            }
            ChordMode::Maj | ChordMode::Min => {
                let minor = ui.chord == ChordMode::Min;
                if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                    notes.extend(tool_chord(t, m, minor));
                    notes.sort_by_key(|n| n.tick);
                }
            }
        }
    }
    if resp.clicked_by(egui::PointerButton::Secondary)
        && let Some(pos) = resp.hover_pos()
    {
        let t = tick_at(pos.x);
        let m = midi_at(pos.y);
        if let Some(idx) = hit_note(t, m)
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
            && idx < notes.len()
        {
            notes.remove(idx);
            ui.sel_note = None;
        }
    }
    if u.ctx().input(|i| i.key_pressed(egui::Key::Delete))
        && let Some(idx) = ui.sel_note.take()
        && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        && idx < notes.len()
    {
        notes.remove(idx);
    }

    velocity_lane(s, ch, pat, u);
    automation_lane(ui, s, ch, pat, u);
}

fn velocity_lane(s: &mut AppState, ch: usize, pat: usize, u: &mut egui::Ui) {
    u.label("Velocity (drag bars)");
    let avail_w = u.available_width().max(300.0);
    let h = 56.0;
    let (rect, resp) = u.allocate_exact_size(egui::vec2(avail_w, h), egui::Sense::click_and_drag());
    let painter = u.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(16));
    let notes = s
        .song
        .patterns
        .get(pat)
        .and_then(|p| p.notes.get(ch))
        .cloned()
        .unwrap_or_default();
    for n in &notes {
        let x = rect.min.x + KEY_W + (n.tick as f32 + 0.5) / 64.0 * (rect.width() - KEY_W);
        let bh = n.vel.clamp(0.0, 1.0) * (h - 6.0);
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(x - 3.0, rect.max.y - 3.0 - bh),
                egui::vec2(6.0, bh),
            ),
            1.0,
            egui::Color32::from_rgb(90, 160, 120),
        );
    }
    if resp.dragged()
        && let Some(pos) = resp.hover_pos()
    {
        let tick = (((pos.x - rect.min.x - KEY_W) / (rect.width() - KEY_W) * 64.0).round() as i32)
            .clamp(0, 63);
        let vel = (1.0 - (pos.y - rect.min.y) / h).clamp(0.05, 1.0);
        if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
            let mut best: Option<usize> = None;
            let mut best_d = 3i32;
            for (idx, n) in notes.iter().enumerate() {
                let d = (n.tick as i32 - tick).abs();
                if d < best_d {
                    best_d = d;
                    best = Some(idx);
                }
            }
            if let Some(idx) = best {
                notes[idx].vel = vel;
            }
        }
    }
}

fn automation_lane(ui: &mut AppUi, s: &mut AppState, ch: usize, pat: usize, u: &mut egui::Ui) {
    u.horizontal(|u| {
        u.label("Automation:");
        u.selectable_value(&mut ui.auto_sel, AutoSel::Vol, "Volume");
        u.selectable_value(&mut ui.auto_sel, AutoSel::Cut, "Cutoff");
        u.label("(click: add · drag: move · right-click: delete)");
    });
    let is_vol = ui.auto_sel == AutoSel::Vol;
    let from_norm = |y: f32| {
        if is_vol {
            y.clamp(0.0, 1.0) * 1.5
        } else {
            (80.0_f32.ln() + y.clamp(0.0, 1.0) * (12000.0_f32.ln() - 80.0_f32.ln())).exp()
        }
    };
    let avail_w = u.available_width().max(300.0);
    let h = 72.0;
    let (rect, resp) = u.allocate_exact_size(egui::vec2(avail_w, h), egui::Sense::click_and_drag());
    let painter = u.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(16));
    let points: Vec<AutoPoint> = {
        let pdata = &s.song.patterns[pat];
        (if is_vol {
            &pdata.auto_vol
        } else {
            &pdata.auto_cut
        })
        .get(ch)
        .cloned()
        .unwrap_or_default()
    };
    let x_of = |tick: u32| rect.min.x + KEY_W + tick as f32 / 64.0 * (rect.width() - KEY_W);
    let y_of = |v: f32| {
        let norm = if is_vol {
            (v / 1.5).clamp(0.0, 1.0)
        } else {
            ((v.max(80.0).ln() - 80.0_f32.ln()) / (12000.0_f32.ln() - 80.0_f32.ln()))
                .clamp(0.0, 1.0)
        };
        rect.max.y - 4.0 - norm * (h - 8.0)
    };
    // Curve preview.
    let mut prev: Option<egui::Pos2> = None;
    for t in 0..64u32 {
        let v = eval_auto(&points, t).unwrap_or(if is_vol {
            1.0
        } else {
            s.song.channels[ch].cutoff
        });
        let p = egui::pos2(x_of(t), y_of(v));
        if let Some(pp) = prev {
            painter.line_segment(
                [pp, p],
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(150, 120, 220)),
            );
        }
        prev = Some(p);
    }
    for p in &points {
        painter.circle_filled(
            egui::pos2(x_of(p.tick), y_of(p.value)),
            4.0,
            egui::Color32::YELLOW,
        );
    }
    let tick_at = |x: f32| {
        (((x - rect.min.x - KEY_W) / (rect.width() - KEY_W) * 64.0).round() as i32).clamp(0, 63)
            as u32
    };
    let val_at = |y: f32| from_norm(1.0 - (y - rect.min.y) / h);
    let near_idx = |points: &[AutoPoint], pos: egui::Pos2| -> Option<usize> {
        let mut best: Option<usize> = None;
        let mut best_d = f32::MAX;
        for (idx, p) in points.iter().enumerate() {
            let dx = x_of(p.tick) - pos.x;
            let dy = y_of(p.value) - pos.y;
            let d = dx * dx + dy * dy;
            if d < best_d && d < 144.0 {
                best_d = d;
                best = Some(idx);
            }
        }
        best
    };
    if resp.drag_started() {
        ui.auto_drag = None;
        if let Some(pos) = resp.hover_pos() {
            let t = tick_at(pos.x);
            if near_idx(&points, pos).is_none() {
                let pts = if is_vol {
                    &mut s.song.patterns[pat].auto_vol
                } else {
                    &mut s.song.patterns[pat].auto_cut
                };
                if let Some(list) = pts.get_mut(ch) {
                    list.push(AutoPoint {
                        tick: t,
                        value: val_at(pos.y),
                    });
                    ui.auto_drag = Some(list.len() - 1);
                }
            } else {
                ui.auto_drag = near_idx(&points, pos);
            }
        }
    }
    if resp.dragged() {
        if let Some(idx) = ui.auto_drag
            && let Some(pos) = resp.hover_pos()
        {
            let pts = if is_vol {
                &mut s.song.patterns[pat].auto_vol
            } else {
                &mut s.song.patterns[pat].auto_cut
            };
            if let Some(list) = pts.get_mut(ch)
                && let Some(p) = list.get_mut(idx)
            {
                p.tick = tick_at(pos.x);
                p.value = val_at(pos.y);
            }
        }
    } else {
        ui.auto_drag = None;
    }
    if resp.clicked_by(egui::PointerButton::Secondary)
        && let Some(pos) = resp.hover_pos()
    {
        if let Some(idx) = near_idx(&points, pos) {
            let pts = if is_vol {
                &mut s.song.patterns[pat].auto_vol
            } else {
                &mut s.song.patterns[pat].auto_cut
            };
            if let Some(list) = pts.get_mut(ch)
                && idx < list.len()
            {
                list.remove(idx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Playlist
// ---------------------------------------------------------------------------

pub fn playlist(s: &mut AppState, u: &mut egui::Ui) {
    u.heading("Playlist (arrangement)");
    let mut dirty_len: Option<usize> = None;
    for bar in 0..s.song.arrangement.len() {
        u.horizontal(|u| {
            u.label(format!("Bar {}", bar + 1));
            let mut v = s.song.arrangement[bar];
            egui::ComboBox::from_id_salt(format!("arr{bar}"))
                .selected_text(if v < 0 {
                    "—".into()
                } else {
                    format!("P{}", v + 1)
                })
                .show_ui(u, |u| {
                    u.selectable_value(&mut v, -1, "—");
                    for p in 0..N_PATTERNS as i32 {
                        u.selectable_value(&mut v, p, format!("P{}", p + 1));
                    }
                });
            s.song.arrangement[bar] = v;
            if u.button("↑").clicked() && bar > 0 {
                s.song.arrangement.swap(bar, bar - 1);
            }
            if u.button("✕").clicked() {
                dirty_len = Some(bar);
            }
        });
    }
    if let Some(bar) = dirty_len {
        s.song.arrangement.remove(bar);
    }
    u.horizontal(|u| {
        if u.button("＋ Bar").clicked() {
            s.song.arrangement.push(-1);
        }
        u.label(format!("{} bars", s.song.arrangement.len()));
    });
}

// ---------------------------------------------------------------------------
// Mixer
// ---------------------------------------------------------------------------

pub fn mixer(s: &mut AppState, u: &mut egui::Ui) {
    u.heading("Mixer");
    u.horizontal(|u| {
        for i in 0..s.song.channels.len() {
            u.vertical(|u| {
                u.label(s.song.channels[i].name.clone());
                let mut vol = s.song.channels[i].volume;
                if u.add(
                    egui::Slider::new(&mut vol, 0.0..=1.5)
                        .vertical()
                        .text("vol"),
                )
                .changed()
                {
                    s.song.channels[i].volume = vol;
                }
                let mut pn = s.song.channels[i].pan;
                if u.add(egui::Slider::new(&mut pn, -1.0..=1.0).text("pan"))
                    .changed()
                {
                    s.song.channels[i].pan = pn;
                }
                let mut cut = s.song.channels[i].cutoff;
                if u.add(
                    egui::Slider::new(&mut cut, 80.0..=12000.0)
                        .logarithmic(true)
                        .text("cut"),
                )
                .changed()
                {
                    s.song.channels[i].cutoff = cut;
                    s.rt.set_channel_cutoff(i, cut);
                }
                let mut dl = s.song.channels[i].delay_send;
                if u.add(egui::Slider::new(&mut dl, 0.0..=1.0).text("dly"))
                    .changed()
                {
                    s.song.channels[i].delay_send = dl;
                }
                let mut rv = s.song.channels[i].reverb_send;
                if u.add(egui::Slider::new(&mut rv, 0.0..=1.0).text("verb"))
                    .changed()
                {
                    s.song.channels[i].reverb_send = rv;
                }
            });
        }
        u.separator();
        u.vertical(|u| {
            u.label("Master");
            let mut dt = s.song.delay_time;
            if u.add(egui::Slider::new(&mut dt, 0.05..=1.0).text("dly t"))
                .changed()
            {
                s.song.delay_time = dt;
            }
            let mut dm = s.song.delay_mix;
            if u.add(egui::Slider::new(&mut dm, 0.0..=0.8).text("dly x"))
                .changed()
            {
                s.song.delay_mix = dm;
            }
            let mut rm = s.song.reverb_mix;
            if u.add(egui::Slider::new(&mut rm, 0.0..=0.8).text("vrb x"))
                .changed()
            {
                s.song.reverb_mix = rm;
            }
        });
    });
    // Scope.
    let scope = s.rt.scope_ordered();
    let (rect, _) = u.allocate_exact_size(
        egui::vec2(u.available_width().max(200.0), 80.0),
        egui::Sense::hover(),
    );
    let painter = u.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(18));
    let n = scope.len();
    let mut prev: Option<egui::Pos2> = None;
    for (k, v) in scope.iter().enumerate() {
        let vv = v.clamp(-1.0, 1.0);
        let x = rect.min.x + rect.width() * (k as f32 / n as f32);
        let y = rect.center().y - vv * rect.height() * 0.45;
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

// ---------------------------------------------------------------------------
// Sample browser
// ---------------------------------------------------------------------------

pub fn browser(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    u.heading("Browser / sampler");
    u.horizontal(|u| {
        u.label("Dir:");
        if u.add(egui::TextEdit::singleline(&mut ui.browser_dir).desired_width(200.0))
            .changed()
        {
            ui.browser_cache = sample_files(&ui.browser_dir);
        }
        if u.button("↻").clicked() {
            ui.browser_cache = sample_files(&ui.browser_dir);
        }
        if u.button("~/Music").clicked() {
            ui.browser_dir = home_music_dir();
            ui.browser_cache = sample_files(&ui.browser_dir);
        }
    });
    if ui.browser_cache.is_empty() {
        ui.browser_cache = sample_files(&ui.browser_dir);
    }
    for path in ui.browser_cache.clone() {
        u.horizontal(|u| {
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            u.label(name);
            if u.button("▶").on_hover_text("Preview").clicked() {
                match sample_into_song(s, &path) {
                    Ok(idx) => {
                        s.rt.live_sample(idx);
                        ui.status = format!("preview {path}");
                    }
                    Err(e) => ui.status = format!("preview failed: {e}"),
                }
            }
            if u.button("→ Ch")
                .on_hover_text("Load into selected channel")
                .clicked()
            {
                match sample_into_song(s, &path) {
                    Ok(idx) => {
                        s.song.channels[ui.sel_ch].kind =
                            crate::song::ChannelKind::Sampler { sample: Some(idx) };
                        ui.status = format!("loaded to channel: {path}");
                    }
                    Err(e) => ui.status = format!("load failed: {e}"),
                }
            }
        });
    }
    u.horizontal(|u| {
        u.label("Path:");
        u.add(egui::TextEdit::singleline(&mut ui.sample_path).desired_width(220.0));
        if u.button("Load → Ch").clicked() {
            let path = ui.sample_path.clone();
            match sample_into_song(s, &path) {
                Ok(idx) => {
                    s.song.channels[ui.sel_ch].kind =
                        crate::song::ChannelKind::Sampler { sample: Some(idx) };
                    ui.status = format!("loaded to channel: {path}");
                }
                Err(e) => ui.status = format!("load failed: {e}"),
            }
        }
    });
    u.label(format!("{} samples in project", s.song.samples.len()));
}
