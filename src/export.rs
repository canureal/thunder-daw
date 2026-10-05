//! Song export: WAV mixdown (hound) and Standard MIDI File (hand-rolled).

use crate::engine::EngineRt;
use crate::song::{Song, TICKS_PATTERN};

/// Render the whole arrangement (or one pattern) to interleaved stereo f32.
pub fn render_to_stereo(song: &Song, sample_rate: f32, once: bool) -> (Vec<f32>, Vec<f32>) {
    let mut rt = EngineRt::new(sample_rate);
    rt.ensure_channels(song);
    rt.song_mode = true;
    rt.ignore_loop = once;
    rt.playing = true;
    let spt = (sample_rate * 60.0 / song.bpm.max(30.0) / 16.0).max(1.0);
    let bars = song.arrangement.len().max(1) as f32;
    let total = (bars * TICKS_PATTERN as f32 * spt) as usize + (sample_rate * 2.0) as usize;
    let mut left = vec![0.0; total];
    let mut right = vec![0.0; total];
    let mut done = 0;
    while done < total && rt.playing {
        let n = 8192.min(total - done);
        rt.process(song, &mut left[done..done + n], &mut right[done..done + n]);
        done += n;
    }
    left.truncate(done);
    right.truncate(done);
    (left, right)
}

pub fn export_wav(song: &Song, path: &str) -> Result<String, String> {
    let sr = 44100.0;
    let (l, r) = render_to_stereo(song, sr, true);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for (a, b) in l.iter().zip(r.iter()) {
        w.write_sample(*a).map_err(|e| e.to_string())?;
        w.write_sample(*b).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok(format!("exported {} samples to {path}", l.len()))
}

// ---------------------------------------------------------------------------
// Minimal SMF type-0 writer: tempo, 4/4, note on/off.
// ---------------------------------------------------------------------------

fn vlq(mut v: u32, out: &mut Vec<u8>) {
    let mut buf = [0u8; 5];
    let mut n = 1;
    buf[4] = (v & 0x7f) as u8;
    v >>= 7;
    while v > 0 {
        n += 1;
        buf[5 - n] = ((v & 0x7f) as u8) | 0x80;
        v >>= 7;
    }
    out.extend_from_slice(&buf[5 - n..]);
}

pub fn export_midi(song: &Song, pattern_fallback: usize, path: &str) -> Result<String, String> {
    const PPQ: u32 = 480;
    const OUR_PER_QN: u32 = 16; // 64 ticks/bar / 4 beats
    let bars: Vec<usize> = if song.arrangement.iter().any(|&p| p >= 0) {
        song.arrangement
            .iter()
            .filter_map(|&p| (p >= 0).then_some(p as usize))
            .collect()
    } else {
        vec![pattern_fallback.min(crate::song::N_PATTERNS - 1)]
    };
    let mut track: Vec<u8> = Vec::new();
    // Tempo.
    let mpqn = (60_000_000.0 / song.bpm.max(30.0)) as u32;
    track.extend([0x00, 0xff, 0x51, 0x03]);
    track.extend(mpqn.to_be_bytes()[1..4].iter());
    // Time signature 4/4.
    track.extend([0x00, 0xff, 0x58, 0x04, 0x04, 0x02, 0x24, 0x08]);
    // Collect note events: (abs_our_tick, on/off, midi, vel).
    let mut evs: Vec<(u32, bool, u8, u8)> = Vec::new();
    for (bar, &pat) in bars.iter().enumerate() {
        let base = bar as u32 * TICKS_PATTERN;
        if let Some(pdata) = song.patterns.get(pat) {
            for notes in pdata.notes.iter() {
                for note in notes {
                    let t = base + note.tick;
                    let v = (note.vel.clamp(0.0, 1.0) * 100.0) as u8 + 10;
                    evs.push((t, true, note.midi, v.min(127)));
                    evs.push((t + note.len.max(1), false, note.midi, 64));
                }
            }
        }
    }
    evs.sort();
    let mut last = 0u32;
    for (t, on, midi, vel) in evs {
        vlq(t.saturating_sub(last) * (PPQ / OUR_PER_QN), &mut track);
        track.push(if on { 0x90 } else { 0x80 });
        track.push(midi);
        track.push(vel);
        last = t;
    }
    track.extend([0x00, 0xff, 0x2f, 0x00]);

    let mut file: Vec<u8> = Vec::new();
    file.extend(b"MThd");
    file.extend(6u32.to_be_bytes());
    file.extend(0u16.to_be_bytes());
    file.extend(1u16.to_be_bytes());
    file.extend((PPQ as u16).to_be_bytes());
    file.extend(b"MTrk");
    file.extend((track.len() as u32).to_be_bytes());
    file.extend(track);
    std::fs::write(path, file).map_err(|e| e.to_string())?;
    Ok(format!("exported {} bars to {path}", bars.len()))
}
