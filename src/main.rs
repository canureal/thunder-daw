// GUI app: no console window on Windows double-click. Terminal users still
// see stdout/stderr in their own console; only release builds detach.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

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

/// FL Studio look: near-black blue-gray, green LEDs and selections.
fn fl_visuals() -> eframe::egui::Visuals {
    let mut v = eframe::egui::Visuals::dark();
    let bg = eframe::egui::Color32::from_rgb(24, 27, 33);
    let panel = eframe::egui::Color32::from_rgb(18, 20, 25);
    v.override_text_color = Some(eframe::egui::Color32::from_rgb(225, 228, 232));
    v.window_fill = panel;
    v.panel_fill = panel;
    v.faint_bg_color = bg;
    v.extreme_bg_color = eframe::egui::Color32::from_rgb(12, 13, 16);
    v.selection.bg_fill = eframe::egui::Color32::from_rgb(46, 120, 52);
    v.widgets.noninteractive.bg_fill = bg;
    v.widgets.inactive.bg_fill = eframe::egui::Color32::from_rgb(44, 49, 58);
    v.widgets.hovered.bg_fill = eframe::egui::Color32::from_rgb(58, 64, 76);
    v.widgets.active.bg_fill = eframe::egui::Color32::from_rgb(52, 110, 58);
    v
}

impl eframe::App for DawApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
        // Space toggles transport, like FL. Not while typing in a text box.
        if !ui.ctx().egui_wants_keyboard_input()
            && ui.ctx().input(|i| i.key_pressed(eframe::egui::Key::Space))
            && let Ok(mut s) = self.state.lock()
        {
            s.rt.playing = !s.rt.playing;
        }
        eframe::egui::Panel::top("transport").show(ui, |u| {
            if let Ok(mut s) = self.state.lock() {
                ui::transport(&mut self.ui, &mut s, u);
            }
        });
        if self.ui.show_browser {
            eframe::egui::Panel::left("browser").show(ui, |u| {
                eframe::egui::ScrollArea::vertical().show(u, |u| {
                    if let Ok(mut s) = self.state.lock() {
                        ui::browser(&mut self.ui, &mut s, u);
                    }
                });
            });
        }
        eframe::egui::Panel::bottom("hint").show(ui, |u| {
            u.horizontal(|u| {
                u.label(format!("status: {}", self.ui.status));
            });
        });
        eframe::egui::CentralPanel::default().show(ui, |u| {
            eframe::egui::ScrollArea::vertical().show(u, |u| {
                if let Ok(mut s) = self.state.lock() {
                    ui::tutorial(&mut self.ui, &mut s, u);
                    if self.ui.show_rack {
                        ui::channel_rack(&mut self.ui, &mut s, u);
                        u.separator();
                    }
                    if self.ui.show_piano {
                        ui::piano_roll(&mut self.ui, &mut s, u);
                        u.separator();
                    }
                    if self.ui.show_playlist || self.ui.show_mixer {
                        u.horizontal(|u| {
                            if self.ui.show_playlist {
                                u.vertical(|u| {
                                    ui::playlist(&mut self.ui, &mut s, u);
                                });
                            }
                            if self.ui.show_playlist && self.ui.show_mixer {
                                u.separator();
                            }
                            if self.ui.show_mixer {
                                u.vertical(|u| {
                                    ui::mixer(&mut s, u);
                                });
                            }
                        });
                        u.separator();
                    }
                }
            });
        });
    }
}

fn main() {
    if std::env::args().any(|a| a == "--offline-test") {
        let mut st = AppState::new();
        // The default project is blank; the self-test needs sound to find.
        st.song = song::demo_song();
        let sr = st.rt.sr;
        st.rt.set_sample_rate(sr, &st.song);
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
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(fl_visuals());
            Ok(Box::new(DawApp {
                state: app_state,
                ui: AppUi::default(),
            }) as Box<dyn eframe::App>)
        }),
    ) {
        eprintln!("eframe error: {e}");
    }
}
