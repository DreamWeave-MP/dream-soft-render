// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic egui scenes shared by the frame benchmark and the golden-image tests.
//!
//! They mimic a handheld settings/importer UI: forms, monospace previews, and a
//! floating window with a shadow, so the renderer sees realistic glyph quads,
//! feathered edge triangles, rounded-corner fans, and translucent shadows.

use dream_soft_render::{RenderFrame, RenderOutcome, SoftwareRenderer};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;

#[derive(Clone, Copy, Debug)]
pub enum Scene {
    Form,
    Preview,
    Window,
}

impl Scene {
    pub const ALL: [Self; 3] = [Self::Form, Self::Preview, Self::Window];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Form => "form",
            Self::Preview => "preview",
            Self::Window => "window",
        }
    }

    pub fn ui(self, ui: &mut egui::Ui, state: &mut SceneState) {
        match self {
            Self::Form => form_ui(ui, state),
            Self::Preview => preview_ui(ui),
            Self::Window => window_ui(ui, state),
        }
    }
}

pub struct SceneState {
    ini_path: String,
    cfg_path: String,
    game_files: bool,
    fonts: bool,
    no_archives: bool,
    encoding: usize,
    volume: f32,
}

impl Default for SceneState {
    fn default() -> Self {
        Self {
            ini_path: "/storage/roms/ports/morrowind/Morrowind.ini".to_owned(),
            cfg_path: "/storage/.config/openmw/openmw.cfg".to_owned(),
            game_files: true,
            fonts: false,
            no_archives: false,
            encoding: 2,
            volume: 0.6,
        }
    }
}

const ENCODINGS: [&str; 3] = ["win1250", "win1251", "win1252"];

fn form_ui(ui: &mut egui::Ui, state: &mut SceneState) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.heading("Dream INI Importer");
        ui.label("Import Morrowind.ini settings into an OpenMW configuration.");
        ui.separator();
        egui::Grid::new("paths").num_columns(3).show(ui, |ui| {
            ui.label("Morrowind.ini");
            ui.text_edit_singleline(&mut state.ini_path);
            let _ = ui.button("Browse…");
            ui.end_row();
            ui.label("openmw.cfg");
            ui.text_edit_singleline(&mut state.cfg_path);
            let _ = ui.button("Browse…");
            ui.end_row();
        });
        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.checkbox(&mut state.game_files, "Import game files");
            ui.checkbox(&mut state.fonts, "Import fonts");
            ui.checkbox(&mut state.no_archives, "Skip archives");
            egui::ComboBox::from_label("Encoding")
                .selected_text(ENCODINGS[state.encoding])
                .show_ui(ui, |ui| {
                    for (index, name) in ENCODINGS.iter().enumerate() {
                        ui.selectable_value(&mut state.encoding, index, *name);
                    }
                });
            ui.horizontal(|ui| {
                ui.radio_value(&mut state.encoding, 0, "Central European");
                ui.radio_value(&mut state.encoding, 1, "Cyrillic");
                ui.radio_value(&mut state.encoding, 2, "Western");
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let _ = ui.button("Preview");
            let _ = ui.button("Save As…");
            let _ = ui.add_enabled(false, egui::Button::new("Update openmw.cfg"));
            let _ = ui.button("Quit");
        });
        ui.separator();
        ui.collapsing("Warnings (2)", |ui| {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Content file Tribunal.esm has no timestamp",
            );
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "Archive Bloodmoon.bsa already covered",
            );
        });
        ui.label("Controller: A select · B back · X preview · Y save · Start update · Select quit");
    });
}

fn preview_text() -> String {
    let mut text = String::new();
    for index in 0..48 {
        match index % 6 {
            0 => text.push_str("data=\"/storage/roms/ports/morrowind/Data Files\"\n"),
            1 => text.push_str(&format!("content=Mod{index:02}_Patch_For_Purists.esp\n")),
            2 => text.push_str(&format!(
                "fallback=Weather_Clear_Sky_Sunrise_Color,{},{},{}\n",
                index * 3,
                index * 5,
                index * 7
            )),
            3 => text.push_str("fallback-archive=Tribunal.bsa\n"),
            4 => text.push_str("# comment preserved from the source cfg\n"),
            _ => text.push_str(&format!("fallback=Water_Map_Alpha,{index}\n")),
        }
    }
    text
}

fn preview_ui(ui: &mut egui::Ui) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Preview");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _ = ui.button("Close");
                let _ = ui.button("Copy");
            });
        });
        egui::Frame::canvas(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(preview_text()).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
    });
}

fn window_ui(ui: &mut egui::Ui, state: &mut SceneState) {
    form_ui(ui, state);
    egui::Window::new("Choose encoding")
        .default_pos(egui::pos2(150.0, 110.0))
        .default_width(330.0)
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("The INI text encoding decides how names are decoded.");
            for (index, name) in ENCODINGS.iter().enumerate() {
                ui.radio_value(&mut state.encoding, index, *name);
            }
            ui.add(egui::Slider::new(&mut state.volume, 0.0..=1.0).text("Preview scale"));
            ui.add(egui::ProgressBar::new(0.42).show_percentage());
            ui.horizontal(|ui| {
                let _ = ui.button("OK");
                let _ = ui.button("Cancel");
            });
        });
}

/// Frames rendered before a scene counts as settled; long enough for window fade-in.
pub const SETTLE_FRAMES: usize = 60;

/// Renders `scene` until egui's layout and texture uploads settle, returning the renderer.
pub fn settled_renderer(scene: Scene) -> (SoftwareRenderer, egui::Context, SceneState) {
    settled_renderer_after(scene, SETTLE_FRAMES)
}

pub fn settled_renderer_after(
    scene: Scene,
    settle_frames: usize,
) -> (SoftwareRenderer, egui::Context, SceneState) {
    let context = egui::Context::default();
    let mut renderer = SoftwareRenderer::default();
    // Every benchmark frame is identical; measure rasterization rather than the skip.
    renderer.set_skip_unchanged_frames(false);
    let mut state = SceneState::default();
    for _ in 0..settle_frames {
        render_frame(&mut renderer, &context, scene, &mut state, false);
    }
    (renderer, context, state)
}

pub fn render_frame(
    renderer: &mut SoftwareRenderer,
    context: &egui::Context,
    scene: Scene,
    state: &mut SceneState,
    measure: bool,
) -> RenderOutcome {
    let frame = RenderFrame {
        context,
        log: None,
        log_frame: measure,
        log_render_stats: false,
        hitch_log_threshold: None,
        frame_index: 0,
        repaint_request_due_before_frame: false,
        synthetic_workload: None,
    };
    renderer
        .render(WIDTH, HEIGHT, &frame, |ui| scene.ui(ui, state))
        .expect("scene renders")
}

/// Renders one frame with deep stats enabled and returns the renderer's log lines.
pub fn stats_lines(
    renderer: &mut SoftwareRenderer,
    context: &egui::Context,
    scene: Scene,
    state: &mut SceneState,
) -> Vec<String> {
    let lines = std::cell::RefCell::new(Vec::new());
    let log = |line: &str| lines.borrow_mut().push(line.to_owned());
    let frame = RenderFrame {
        context,
        log: Some(&log),
        log_frame: true,
        log_render_stats: true,
        hitch_log_threshold: None,
        frame_index: 0,
        repaint_request_due_before_frame: false,
        synthetic_workload: None,
    };
    renderer
        .render(WIDTH, HEIGHT, &frame, |ui| scene.ui(ui, state))
        .expect("scene renders");
    lines.into_inner()
}

/// FNV-1a 64 over the surface bytes; stable across platforms and Rust versions.
pub fn surface_hash(renderer: &SoftwareRenderer) -> u64 {
    renderer
        .surface()
        .pixels
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}
