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
    fn contains(&self, pc: u8, root: u8) -> bool {
        let rel = (12 + pc as i32 - (root % 12) as i32) % 12;
        match self {
            ScaleKind::Chromatic => true,
            ScaleKind::Major => matches!(rel, 0 | 2 | 4 | 5 | 7 | 9 | 11),
            ScaleKind::Minor => matches!(rel, 0 | 2 | 3 | 5 | 7 | 8 | 10),
            ScaleKind::PentMaj => matches!(rel, 0 | 2 | 4 | 7 | 9),
            ScaleKind::PentMin => matches!(rel, 0 | 3 | 5 | 7 | 10),
        }
    }
    /// Nearest pitch at or below `midi` that sits in the scale.
    fn snap_midi(&self, midi: u8, root: u8) -> u8 {
        if *self == ScaleKind::Chromatic {
            return midi;
        }
        let mut m = midi;
        for _ in 0..12 {
            if self.contains(m % 12, root) {
                return m;
            }
            m = m.saturating_sub(1).max(21);
        }
        midi
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChordMode {
    Off,
    Maj,
    Min,
}

/// FL-style piano roll tools. Draw places notes, Erase removes them with
/// left-click, Hear plays pitches without writing anything.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum PianoTool {
    #[default]
    Draw,
    Erase,
    Hear,
}

impl PianoTool {
    fn name(&self) -> &'static str {
        match self {
            PianoTool::Draw => "Draw",
            PianoTool::Erase => "Erase",
            PianoTool::Hear => "Hear",
        }
    }
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
    // FL-style window visibility (toolbar view toggles).
    pub show_browser: bool,
    pub show_rack: bool,
    pub show_piano: bool,
    pub show_playlist: bool,
    pub show_mixer: bool,
    tool: PianoTool,
    piano_bot: u8,
    snap: u32,
    scale: ScaleKind,
    root: u8,
    snap_scale: bool,
    chord: ChordMode,
    sel_note: Option<usize>,
    prop_note: Option<usize>,
    drag: Option<(usize, DragKind, i32, i32)>,
    pending_add: Option<(u32, u8)>,
    auto_sel: AutoSel,
    auto_drag: Option<usize>,
    auto_pressed: bool,
    save_path: String,
    wav_path: String,
    mid_path: String,
    sample_path: String,
    browser_dir: String,
    browser_cache: Vec<String>,
    pub status: String,
    taps: Vec<Instant>,
    show_tutorial: bool,
}

/// Marker file: first run shows the tutorial, afterwards it stays hidden.
fn tutorial_marker() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|p| p.join("thunder-daw").join("tutorial_seen"))
}

fn tutorial_seen() -> bool {
    tutorial_marker().map(|p| p.exists()).unwrap_or(true)
}

fn set_tutorial_seen(seen: bool) {
    if let Some(p) = tutorial_marker() {
        if seen {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            std::fs::write(p, "1").ok();
        } else {
            std::fs::remove_file(p).ok();
        }
    }
}

impl Default for AppUi {
    fn default() -> Self {
        Self {
            sel_ch: 4,
            sel_pat: 0,
            show_browser: true,
            show_rack: true,
            show_piano: true,
            show_playlist: true,
            show_mixer: true,
            tool: PianoTool::Draw,
            piano_bot: 40,
            snap: 4,
            scale: ScaleKind::Major,
            root: 0,
            snap_scale: false,
            chord: ChordMode::Off,
            sel_note: None,
            prop_note: None,
            drag: None,
            pending_add: None,
            auto_sel: AutoSel::Vol,
            auto_drag: None,
            auto_pressed: false,
            save_path: "thunder-daw.json".into(),
            wav_path: "mixdown.wav".into(),
            mid_path: "song.mid".into(),
            sample_path: String::new(),
            browser_dir: "samples".into(),
            browser_cache: Vec::new(),
            status: "press Play or open the Tutorial".into(),
            taps: Vec::new(),
            show_tutorial: !tutorial_seen(),
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
    dirs::audio_dir()
        .map(|p| p.join("samples").to_string_lossy().into_owned())
        .unwrap_or_else(|| "samples".into())
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

pub fn transport(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    u.horizontal(|u| {
        // FL window toggles: browser, rack, piano, playlist, mixer.
        let mut v = ui.show_browser;
        if u.toggle_value(&mut v, "Files")
            .on_hover_text("Browser (F8)")
            .changed()
        {
            ui.show_browser = v;
        }
        let mut v = ui.show_rack;
        if u.toggle_value(&mut v, "Rack")
            .on_hover_text("Channel rack (F6)")
            .changed()
        {
            ui.show_rack = v;
        }
        let mut v = ui.show_piano;
        if u.toggle_value(&mut v, "Piano")
            .on_hover_text("Piano roll (F7)")
            .changed()
        {
            ui.show_piano = v;
        }
        let mut v = ui.show_playlist;
        if u.toggle_value(&mut v, "List")
            .on_hover_text("Playlist (F5)")
            .changed()
        {
            ui.show_playlist = v;
        }
        let mut v = ui.show_mixer;
        if u.toggle_value(&mut v, "Mix")
            .on_hover_text("Mixer (F9)")
            .changed()
        {
            ui.show_mixer = v;
        }
        u.separator();
        let mut playing = s.rt.playing;
        if u.button(if playing { "⏹ Stop" } else { "▶ Play" })
            .on_hover_text("Spacebar works too")
            .clicked()
        {
            playing = !playing;
            s.rt.playing = playing;
        }
        if u.button("Tutorial")
            .on_hover_text("First-time walkthrough")
            .clicked()
        {
            ui.show_tutorial = !ui.show_tutorial;
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

/// Snap a pitch into the scale when snap-to-scale is on, else pass through.
fn scale_checked(ui: &AppUi, midi: u8) -> u8 {
    if ui.snap_scale {
        ui.scale.snap_midi(midi, ui.root)
    } else {
        midi
    }
}

/// Add a single note or a chord stamp, then report it in the status line.
fn place_note(ui: &mut AppUi, s: &mut AppState, ch: usize, pat: usize, tick: u32, midi: u8) {
    match ui.chord {
        ChordMode::Off => {
            if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                notes.push(Note::new(tick, ui.snap.max(1), midi, 0.85));
                notes.sort_by_key(|n| n.tick);
                ui.sel_note = notes.iter().rposition(|n| n.tick == tick && n.midi == midi);
                ui.status = format!("note {} at tick {tick}", note_label(midi));
            }
        }
        ChordMode::Maj | ChordMode::Min => {
            let minor = ui.chord == ChordMode::Min;
            if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch) {
                notes.extend(tool_chord(tick, midi, minor));
                notes.sort_by_key(|n| n.tick);
                ui.status = format!(
                    "{} chord at tick {tick}",
                    if minor { "minor" } else { "major" }
                );
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn piano_roll(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    let ch = ui.sel_ch.min(s.song.channels.len().saturating_sub(1));
    let pat = ui.sel_pat;
    u.horizontal(|u| {
        u.heading(format!(
            "Piano roll : {} / P{}",
            s.song.channels[ch].name,
            pat + 1
        ));
        // FL-style tool modes: draw writes, erase deletes, hear only plays.
        for tool in [PianoTool::Draw, PianoTool::Erase, PianoTool::Hear] {
            if u.selectable_label(ui.tool == tool, tool.name())
                .on_hover_text(match tool {
                    PianoTool::Draw => "Draw: click adds a note, drag moves it (P)",
                    PianoTool::Erase => "Erase: left-click deletes notes (D)",
                    PianoTool::Hear => "Hear: click plays pitches, writes nothing (Y)",
                })
                .clicked()
            {
                ui.tool = tool;
            }
        }
        // Target channel: switch instrument without leaving the piano roll.
        let mut target = ch;
        egui::ComboBox::from_id_salt("target_ch")
            .selected_text(s.song.channels[ch].name.clone())
            .show_ui(u, |u| {
                for (i, c) in s.song.channels.iter().enumerate() {
                    let tick = s.song.patterns[pat]
                        .notes
                        .get(i)
                        .map(|ns| !ns.is_empty())
                        .unwrap_or(false);
                    u.selectable_value(
                        &mut target,
                        i,
                        format!("{}{}", if tick { "● " } else { "" }, c.name),
                    );
                }
            });
        if target != ch {
            ui.sel_ch = target;
            ui.sel_note = None;
            ui.prop_note = None;
            return;
        }
        if u.button("Quant")
            .on_hover_text("Snap starts to grid")
            .clicked()
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        {
            tool_quantize(notes, ui.snap);
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
        if u.button("Strum").on_hover_text("Spread chords").clicked()
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        {
            tool_strum(notes, 2);
            ui.status = "strummed".into();
        }
        if u.button("Arp")
            .on_hover_text("16th arpeggio from pitches")
            .clicked()
            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
        {
            tool_arpeggiate(notes);
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
        u.label("Root:");
        let mut root = ui.root;
        egui::ComboBox::from_id_salt("root")
            .selected_text(NOTE_NAMES[(root % 12) as usize])
            .show_ui(u, |u| {
                for r in 0u8..12 {
                    u.selectable_value(&mut root, r, NOTE_NAMES[r as usize]);
                }
            });
        ui.root = root;
        let mut snap_scale = ui.snap_scale;
        if u.toggle_value(&mut snap_scale, "ScaleSnap")
            .on_hover_text("Force added and moved notes into the scale")
            .changed()
        {
            ui.snap_scale = snap_scale;
        }
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
        let in_scale = ui.scale.contains(midi % 12, ui.root);
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
            // FL preview keyboard: the key strip plays instead of editing.
            if pos.x < rect.min.x + KEY_W {
                if m >= midi_lo as i32 && m <= midi_hi as i32 {
                    s.rt.live_note(m as u8);
                }
            } else if (0..64).contains(&t) && m >= midi_lo as i32 && m <= midi_hi as i32 {
                if let Some(idx) = hit_note(t, m) {
                    // Erase tool deletes on press, like FL's delete tool.
                    if ui.tool == PianoTool::Erase
                        && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
                        && idx < notes.len()
                    {
                        notes.remove(idx);
                        ui.sel_note = None;
                    } else if ui.tool == PianoTool::Hear {
                        // Hear tool only plays the pitch.
                        s.rt.live_note(m as u8);
                    } else {
                        let n = &notes_snapshot[idx];
                        let right_edge = x_of(n.tick + n.len);
                        let kind = if pos.x > right_edge - 7.0 {
                            DragKind::Resize
                        } else {
                            DragKind::Move
                        };
                        ui.drag = Some((idx, kind, t - n.tick as i32, m - n.midi as i32));
                        ui.sel_note = Some(idx);
                    }
                } else if ui.tool == PianoTool::Draw {
                    let snap_t =
                        ((t / ui.snap.max(1) as i32) * ui.snap.max(1) as i32).clamp(0, 63) as u32;
                    ui.pending_add = Some((snap_t, scale_checked(ui, m as u8)));
                } else if ui.tool == PianoTool::Hear {
                    s.rt.live_note(m as u8);
                }
            }
        }
    }
    if resp.dragged()
        && let Some((idx, kind, gt, gm)) = ui.drag
        && let Some(pos) = resp.hover_pos()
    {
        let t = tick_at(pos.x);
        let m = midi_at(pos.y);
        if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
            && let Some(n) = notes.get_mut(idx)
        {
            match kind {
                DragKind::Move => {
                    n.tick = (t - gt).clamp(0, 63) as u32;
                    let target = (m - gm).clamp(21, 108) as u8;
                    n.midi = if ui.snap_scale {
                        ui.scale.snap_midi(target, ui.root)
                    } else {
                        target
                    };
                }
                DragKind::Resize => {
                    n.len = (t - n.tick as i32 + 1).clamp(1, 64) as u32;
                }
            }
        }
    } else {
        ui.drag = None;
    }
    if resp.clicked() {
        if let Some((t, m)) = ui.pending_add.take() {
            // Press-drag-release path: press already chose the cell.
            place_note(ui, s, ch, pat, t, m);
        } else if let Some(pos) = resp.hover_pos() {
            // Pure click path: no press event fired, hit-test here instead.
            let t = tick_at(pos.x);
            let m = midi_at(pos.y);
            if pos.x < rect.min.x + KEY_W {
                if m >= midi_lo as i32 && m <= midi_hi as i32 {
                    s.rt.live_note(m as u8);
                }
            } else if (0..64).contains(&t) && m >= midi_lo as i32 && m <= midi_hi as i32 {
                match ui.tool {
                    PianoTool::Draw => {
                        if let Some(idx) = hit_note(t, m) {
                            ui.sel_note = Some(idx);
                        } else {
                            let snap_t = ((t / ui.snap.max(1) as i32) * ui.snap.max(1) as i32)
                                .clamp(0, 63) as u32;
                            let mp = scale_checked(ui, m as u8);
                            place_note(ui, s, ch, pat, snap_t, mp);
                        }
                    }
                    PianoTool::Erase => {
                        if let Some(idx) = hit_note(t, m)
                            && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
                            && idx < notes.len()
                        {
                            notes.remove(idx);
                            ui.sel_note = None;
                        }
                    }
                    PianoTool::Hear => {
                        s.rt.live_note(m as u8);
                    }
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
    if resp.double_clicked()
        && let Some(pos) = resp.hover_pos()
    {
        let t = tick_at(pos.x);
        let m = midi_at(pos.y);
        if (0..64).contains(&t) && m >= midi_lo as i32 && m <= midi_hi as i32 {
            ui.prop_note = hit_note(t, m);
            ui.sel_note = ui.prop_note;
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

    // Note properties dialog (FL opens one on double-click).
    if let Some(idx) = ui.prop_note {
        let count = s.song.patterns[pat]
            .notes
            .get(ch)
            .map(|ns| ns.len())
            .unwrap_or(0);
        if idx >= count {
            ui.prop_note = None;
        } else {
            let mut close = false;
            let mut remove = false;
            egui::Window::new("Note")
                .collapsible(false)
                .show(u.ctx(), |w| {
                    if let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
                        && let Some(n) = notes.get_mut(idx)
                    {
                        w.label(format!("{} @ tick {}", note_label(n.midi), n.tick));
                        w.add(egui::Slider::new(&mut n.vel, 0.05..=1.0).text("Velocity"));
                        let mut len = n.len;
                        w.add(egui::Slider::new(&mut len, 1..=64).text("Length"));
                        n.len = len;
                        let mut midi = n.midi;
                        w.add(egui::Slider::new(&mut midi, 21..=108).text("Pitch"));
                        n.midi = midi;
                        if w.button("Delete note").clicked() {
                            remove = true;
                        }
                        if w.button("Close").clicked() {
                            close = true;
                        }
                    } else {
                        close = true;
                    }
                });
            if remove
                && let Some(notes) = s.song.patterns[pat].notes.get_mut(ch)
                && idx < notes.len()
            {
                notes.remove(idx);
                ui.sel_note = None;
            }
            if remove || close {
                ui.prop_note = None;
            }
        }
    }
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
        ui.auto_pressed = true;
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
    // Pure clicks never start a drag, so add the point here instead.
    // A press that already added one sets auto_pressed: don't double up.
    if resp.clicked() {
        if ui.auto_pressed {
            ui.auto_pressed = false;
        } else if ui.auto_drag.is_none()
            && let Some(pos) = resp.hover_pos()
        {
            let t = tick_at(pos.x);
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
                ui.status = "automation point added".into();
            }
        }
    }
    if resp.clicked_by(egui::PointerButton::Secondary)
        && let Some(pos) = resp.hover_pos()
        && let Some(idx) = near_idx(&points, pos)
    {
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

// ---------------------------------------------------------------------------
// Tutorial
// ---------------------------------------------------------------------------

/// First-run walkthrough. Opens on the very first launch, then stays hidden
/// unless reopened from the toolbar. "Load demo" fills the blank project
/// with the example groove so new users hear something immediately.
pub fn tutorial(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    if !ui.show_tutorial {
        return;
    }
    let mut close = false;
    egui::Window::new("Welcome to thunder-daw")
        .collapsible(false)
        .show(u.ctx(), |w| {
            w.label("Make your first beat in two minutes:");
            w.label("1. Channel rack: click the numbered boxes to place kick, snare, hat hits.");
            w.label("2. Press Play (or Spacebar) to hear your pattern loop.");
            w.label("3. Click Bass or Lead, then click in the piano roll to draw notes.");
            w.label("4. Playlist: add bars and pick a pattern per bar to build a song.");
            w.label("5. Mixer: set levels, pan, echo and reverb sends.");
            w.label("6. Transport: save the project, export a WAV mixdown or MIDI file.");
            w.horizontal(|w| {
                if w.button("Load demo song").clicked() {
                    s.song = crate::song::demo_song();
                    let sr = s.rt.sr;
                    s.rt.set_sample_rate(sr, &s.song);
                    ui.sel_ch = 4;
                    ui.sel_pat = 0;
                    s.rt.pattern = 0;
                    ui.status = "demo loaded: press Play".into();
                    close = true;
                }
                if w.button("Start blank").clicked() {
                    close = true;
                }
            });
            let mut show_on_start = !tutorial_seen();
            if w.checkbox(&mut show_on_start, "Show on startup").changed() {
                set_tutorial_seen(!show_on_start);
            }
        });
    if close {
        ui.show_tutorial = false;
    }
}

// ---------------------------------------------------------------------------
// Playlist
// ---------------------------------------------------------------------------

/// FL pattern colors, one per pattern slot.
const CLIP_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(88, 160, 88),
    egui::Color32::from_rgb(88, 130, 180),
    egui::Color32::from_rgb(180, 130, 80),
    egui::Color32::from_rgb(150, 100, 170),
    egui::Color32::from_rgb(180, 90, 90),
    egui::Color32::from_rgb(90, 160, 160),
    egui::Color32::from_rgb(150, 170, 80),
    egui::Color32::from_rgb(130, 130, 140),
];

pub fn playlist(ui: &mut AppUi, s: &mut AppState, u: &mut egui::Ui) {
    u.heading("Playlist (arrangement)");
    // Clip lane: one block per bar, colored by pattern. Click selects the
    // pattern, double-click opens it in the piano roll.
    let bars = s.song.arrangement.len();
    if bars > 0 {
        let avail_w = u.available_width().max(200.0);
        let cell_w = (avail_w / bars.max(1) as f32).clamp(24.0, 90.0);
        let h = 44.0;
        let (rect, resp) =
            u.allocate_exact_size(egui::vec2(cell_w * bars as f32, h), egui::Sense::click());
        let painter = u.painter_at(rect);
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(16));
        let play_bar = if s.rt.playing && s.rt.song_mode {
            Some((s.rt.tick_pos.max(0.0) / 64.0) as usize)
        } else {
            None
        };
        for bar in 0..bars {
            let x0 = rect.min.x + bar as f32 * cell_w;
            let pat = s.song.arrangement[bar];
            let col = if pat < 0 {
                egui::Color32::from_gray(30)
            } else {
                CLIP_COLORS[(pat as usize) % 8]
            };
            let selected = pat >= 0 && pat as usize == ui.sel_pat;
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x0 + 1.0, rect.min.y + 2.0),
                    egui::vec2(cell_w - 2.0, h - 4.0),
                ),
                3.0,
                if play_bar == Some(bar) {
                    egui::Color32::LIGHT_GREEN
                } else {
                    col
                },
            );
            if selected {
                painter.rect_stroke(
                    egui::Rect::from_min_size(
                        egui::pos2(x0 + 1.0, rect.min.y + 2.0),
                        egui::vec2(cell_w - 2.0, h - 4.0),
                    ),
                    3.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                    egui::StrokeKind::Outside,
                );
            }
            painter.text(
                egui::pos2(x0 + cell_w * 0.5, rect.center().y),
                egui::Align2::CENTER_CENTER,
                if pat < 0 {
                    "—".to_string()
                } else {
                    format!("P{}", pat + 1)
                },
                egui::FontId::proportional(12.0),
                egui::Color32::BLACK,
            );
        }
        if resp.clicked()
            && let Some(pos) = resp.hover_pos()
        {
            let bar = ((pos.x - rect.min.x) / cell_w).floor() as usize;
            if bar < bars {
                let pat = s.song.arrangement[bar];
                if pat >= 0 {
                    ui.sel_pat = pat as usize;
                    s.rt.pattern = pat as usize;
                }
            }
        }
        if resp.double_clicked()
            && let Some(pos) = resp.hover_pos()
        {
            let bar = ((pos.x - rect.min.x) / cell_w).floor() as usize;
            if bar < bars {
                let pat = s.song.arrangement[bar];
                if pat >= 0 {
                    ui.sel_pat = pat as usize;
                    s.rt.pattern = pat as usize;
                    ui.show_piano = true;
                    ui.status = format!("editing P{}", pat + 1);
                }
            }
        }
    }
    // Bar editing row.
    let mut dirty_len: Option<usize> = None;
    u.horizontal(|u| {
        u.label(format!("{} bars", s.song.arrangement.len()));
        if u.button("+ Bar").clicked() {
            s.song.arrangement.push(-1);
        }
        if u.button("- Bar").clicked() && !s.song.arrangement.is_empty() {
            dirty_len = Some(s.song.arrangement.len() - 1);
        }
        let mut v = ui.sel_pat as i32;
        egui::ComboBox::from_id_salt("arr_write")
            .selected_text(format!("Write P{}", v + 1))
            .show_ui(u, |u| {
                for p in 0..N_PATTERNS as i32 {
                    u.selectable_value(&mut v, p, format!("P{}", p + 1));
                }
            });
        if v as usize != ui.sel_pat {
            ui.sel_pat = v as usize;
            s.rt.pattern = v as usize;
        }
        if u.button("Fill").clicked() {
            for slot in s.song.arrangement.iter_mut() {
                if *slot < 0 {
                    *slot = v;
                }
            }
        }
    });
    if let Some(bar) = dirty_len {
        s.song.arrangement.remove(bar);
    }
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
                let peak = s.rt.ch_peaks.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                let (mrect, _) = u.allocate_exact_size(egui::vec2(58.0, 7.0), egui::Sense::hover());
                let mp = u.painter_at(mrect);
                mp.rect_filled(mrect, 1.0, egui::Color32::from_gray(28));
                if peak > 0.003 {
                    let col = if peak > 0.9 {
                        egui::Color32::from_rgb(220, 70, 60)
                    } else if peak > 0.65 {
                        egui::Color32::from_rgb(220, 180, 60)
                    } else {
                        egui::Color32::from_rgb(90, 200, 100)
                    };
                    mp.rect_filled(
                        egui::Rect::from_min_size(mrect.min, egui::vec2(mrect.width() * peak, 7.0)),
                        1.0,
                        col,
                    );
                }
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
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
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
