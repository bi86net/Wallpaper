#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod rotation;
mod service;
mod settings;
mod ui;
mod used;
use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};

const VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");

enum Event {
    RotationStarted,
    Rotation(Result<String, String>, bool),
}
struct App {
    settings: settings::Settings,
    autostart: bool,
    status: String,
    error: String,
    rx: Receiver<Event>,
    minimize: bool,
    rotation: Sender<rotation::Command>,
    switching: bool,
    can_previous: bool,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ui::configure(&cc.egui_ctx);
        let mut fonts = egui::FontDefinitions::default();
        for path in [
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ] {
            if let Ok(data) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("cjk".into(), egui::FontData::from_owned(data).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "cjk".into());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        let (tx, rx) = mpsc::channel();
        let (mut settings, mut error) = match settings::Settings::load() {
            Ok(s) => (s, String::new()),
            Err(e) => (
                settings::Settings {
                    initialized: true,
                    ..Default::default()
                },
                format!("读取设置失败：{e}"),
            ),
        };
        let autostart = match settings::launcher().and_then(|l| {
            if !settings.initialized {
                l.enable()?;
                settings.initialized = true;
                settings.save()?;
            }
            Ok(l.is_enabled()?)
        }) {
            Ok(v) => v,
            Err(e) => {
                error = format!("自启设置失败：{e}");
                false
            }
        };
        settings.interval_minutes = settings.interval_minutes.clamp(1, 1440);
        let rotation = rotation::start(tx.clone(), cc.egui_ctx.clone());
        let app = Self {
            rotation,
            switching: false,
            can_previous: false,
            settings,
            autostart,
            status: "点击下一张壁纸，或开启定时更换。".into(),
            error,
            rx,
            minimize: std::env::args().any(|a| a == "--minimized"),
        };
        app.sync_rotation();
        app
    }
    fn sync_rotation(&self) {
        let _ = self
            .rotation
            .send(rotation::Command::Configure(rotation::Schedule {
                enabled: self.settings.rotation_enabled,
                minutes: self.settings.interval_minutes,
                categories: self.settings.selected_categories(),
            }));
    }
    fn switch_wallpaper(&mut self, command: rotation::Command) {
        match self.rotation.send(command) {
            Ok(()) => {
                self.switching = true;
                self.error.clear();
                self.status = "正在更换壁纸…".into();
            }
            Err(_) => self.error = "壁纸后台已停止，请重启程序".into(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.minimize {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.minimize = false;
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::RotationStarted => {
                    self.switching = true;
                }
                Event::Rotation(result, previous) => {
                    self.switching = false;
                    self.can_previous = previous;
                    match result {
                        Ok(s) => {
                            self.status = s;
                            self.error.clear();
                        }
                        Err(e) => {
                            self.status = "更换失败，可以重试".into();
                            self.error = e;
                        }
                    }
                }
            }
        }
        self.draw_ui(ctx);
    }
}
fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("bundled application icon must be a valid PNG")
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        &format!("光景 Wallpaper · {VERSION}"),
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(app_icon())
                .with_inner_size([440.0, 560.0])
                .with_min_inner_size([380.0, 400.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
