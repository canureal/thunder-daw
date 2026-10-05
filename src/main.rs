mod engine;
mod export;
mod song;
mod ui;

use engine::{AppState, start_audio};
use std::sync::{Arc, Mutex};
use ui::AppUi;

struct DawApp {
    state: Arc<Mutex<AppState>>,
    ui: AppUi,
}

impl eframe::App for DawApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
        eframe::egui::Panel::top("transport").show(ui, |u| {
            if let Ok(mut s) = self.state.lock() {
                ui::transport(&mut self.ui, &mut s, u);
            }
        });
        eframe::egui::CentralPanel::default().show(ui, |u| {
            eframe::egui::ScrollArea::vertical().show(u, |u| {
                if let Ok(mut s) = self.state.lock() {
                    ui::channel_rack(&mut self.ui, &mut s, u);
                    u.separator();
                    ui::piano_roll(&mut self.ui, &mut s, u);
                    u.separator();
                    u.horizontal(|u| {
                        u.vertical(|u| {
                            ui::playlist(&mut self.ui, &mut s, u);
                        });
                        u.separator();
                        u.vertical(|u| {
                            ui::mixer(&mut self.ui, &mut s, u);
                        });
                    });
                    u.separator();
                    ui::browser(&mut self.ui, &mut s, u);
                    u.separator();
                    u.label(format!("status: {}", self.ui.status));
                }
            });
        });
    }
}

fn main() {
    if std::env::args().any(|a| a == "--offline-test") {
        let mut st = AppState::new();
        st.rt.song_mode = true;
        st.rt.playing = true;
        let n = 44100 * 4;
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        st.rt.process(&st.song.clone(), &mut l, &mut r);
        let peak = l
            .iter()
            .chain(r.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        let rms = (l.iter().map(|x| x * x).sum::<f32>() / l.len() as f32).sqrt();
        println!(
            "offline-test: peak={peak:.3} rms={rms:.4} tick_pos={:.1}",
            st.rt.tick_pos
        );
        if peak < 0.01 || st.rt.tick_pos < 10.0 {
            eprintln!("offline-test FAILED");
            std::process::exit(1);
        }
        println!("offline-test OK");
        return;
    }
    let state = Arc::new(Mutex::new(AppState::new()));
    let _stream = match start_audio(Arc::clone(&state)) {
        Some(st) => {
            println!("audio started");
            Some(st)
        }
        None => {
            eprintln!("no audio device; running GUI silent.");
            None
        }
    };
    let app_state = Arc::clone(&state);
    let options = eframe::NativeOptions::default();
    let _ = _stream;
    if let Err(e) = eframe::run_native(
        "thunder-daw",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(DawApp {
                state: app_state,
                ui: AppUi::default(),
            }) as Box<dyn eframe::App>)
        }),
    ) {
        eprintln!("eframe error: {e}");
    }
}
