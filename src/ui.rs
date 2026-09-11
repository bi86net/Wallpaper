use crate::{App, VERSION, rotation, service::Category, settings};
use eframe::egui::{self, Color32, RichText, Stroke};

const BACKGROUND: Color32 = Color32::from_rgb(246, 247, 251);
const TEXT: Color32 = Color32::from_rgb(29, 35, 51);
const MUTED: Color32 = Color32::from_rgb(112, 120, 139);
const BORDER: Color32 = Color32::from_rgb(228, 232, 240);
const PRIMARY: Color32 = Color32::from_rgb(80, 91, 218);
const TINT: Color32 = Color32::from_rgb(237, 239, 255);

pub fn configure(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = TINT;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, PRIMARY);
    style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.hovered.bg_fill = TINT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, PRIMARY);
    style.visuals.widgets.active.bg_fill = TINT;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, PRIMARY);
    style.spacing.item_spacing = egui::vec2(8.0, 10.0);
    style.spacing.button_padding = egui::vec2(8.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    ctx.set_style(style);
}
fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(12.0).color(MUTED));
}
fn panel() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(12)
        .inner_margin(16)
}
fn segment(ui: &mut egui::Ui, text: &str, selected: bool, width: f32) -> bool {
    ui.add_sized(
        [width, 34.0],
        egui::Button::new(RichText::new(text).color(if selected { PRIMARY } else { MUTED }))
            .fill(if selected { TINT } else { Color32::WHITE })
            .stroke(Stroke::new(
                1.0_f32,
                if selected {
                    Color32::from_rgb(204, 209, 251)
                } else {
                    BORDER
                },
            ))
            .corner_radius(7),
    )
    .clicked()
}
fn switch(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(38.0, 22.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    ui.painter().rect_filled(
        rect,
        11,
        if *value {
            PRIMARY
        } else {
            Color32::from_rgb(203, 209, 220)
        },
    );
    let position = ui.ctx().animate_bool(response.id, *value);
    ui.painter().circle_filled(
        egui::pos2(rect.left() + 11.0 + position * 16.0, rect.center().y),
        8.0,
        Color32::WHITE,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(3.0),
            13,
            Stroke::new(1.0_f32, PRIMARY),
            egui::StrokeKind::Outside,
        );
    }
    response.on_hover_text(label).changed()
}

impl App {
    fn save_ui_settings(&mut self) {
        self.sync_rotation();
        if let Err(e) = self.settings.save() {
            self.error = format!("保存设置失败：{e}");
        }
    }
    pub fn draw_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(20))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("光景").size(24.0).strong());
                            ui.label(RichText::new("WALLPAPER").size(11.0).color(MUTED));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("最小化").clicked() {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(
                                            true,
                                        ));
                                    }
                                },
                            );
                        });
                        hint(ui, "4K 壁纸，让桌面焕然一新");
                        ui.add_space(4.0);
                        panel().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("壁纸分类 · 可多选").strong());
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 8.0) / 2.0;
                                for category in [Category::Landscape, Category::Beauty] {
                                    if segment(
                                        ui,
                                        &format!(
                                            "{}{}",
                                            if self
                                                .settings
                                                .selected_categories()
                                                .contains(&category)
                                            {
                                                "✓ "
                                            } else {
                                                ""
                                            },
                                            category.label()
                                        ),
                                        self.settings.selected_categories().contains(&category),
                                        width,
                                    ) && self.settings.toggle_category(category)
                                    {
                                        self.save_ui_settings();
                                    }
                                }
                            });
                            hint(ui, "多选时随机获取，至少保留一项");
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 8.0) / 2.0;
                                if ui
                                    .add_enabled(
                                        !self.switching && self.can_previous,
                                        egui::Button::new("上一张")
                                            .min_size(egui::vec2(width, 40.0))
                                            .corner_radius(8),
                                    )
                                    .on_disabled_hover_text("暂无上一张，或正在更换")
                                    .clicked()
                                {
                                    self.switch_wallpaper(rotation::Command::Previous);
                                }
                                if ui
                                    .add_enabled(
                                        !self.switching,
                                        egui::Button::new(
                                            RichText::new(if self.switching {
                                                "更换中…"
                                            } else {
                                                "下一张 →"
                                            })
                                            .color(Color32::WHITE),
                                        )
                                        .fill(PRIMARY)
                                        .min_size(egui::vec2(width, 40.0))
                                        .corner_radius(8),
                                    )
                                    .clicked()
                                {
                                    self.switch_wallpaper(rotation::Command::Next);
                                }
                            });
                        });
                        panel().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let mut changed = false;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("自动更换").strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        changed |= switch(
                                            ui,
                                            &mut self.settings.rotation_enabled,
                                            "自动更换壁纸",
                                        );
                                    },
                                );
                            });
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 24.0) / 4.0;
                                for minutes in [1, 5, 10] {
                                    if segment(
                                        ui,
                                        &format!("{minutes} 分钟"),
                                        !self.settings.custom_interval
                                            && self.settings.interval_minutes == minutes,
                                        width,
                                    ) {
                                        self.settings.interval_minutes = minutes;
                                        self.settings.custom_interval = false;
                                        changed = true;
                                    }
                                }
                                if segment(ui, "自定义", self.settings.custom_interval, width) {
                                    self.settings.custom_interval = true;
                                    changed = true;
                                }
                            });
                            if self.settings.custom_interval {
                                ui.horizontal(|ui| {
                                    ui.label("更换间隔");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(
                                                &mut self.settings.interval_minutes,
                                            )
                                            .range(1..=1440)
                                            .suffix(" 分钟"),
                                        )
                                        .on_hover_text("可输入 1–1440 分钟")
                                        .changed();
                                });
                            }
                            if changed {
                                self.save_ui_settings();
                            }
                            hint(
                                ui,
                                if self.settings.rotation_enabled {
                                    format!(
                                        "每 {} 分钟更换 · 最小化后继续运行",
                                        self.settings.interval_minutes
                                    )
                                } else {
                                    "已暂停自动更换，可随时手动切换".into()
                                },
                            );
                            ui.add_space(2.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("开机启动").strong());
                                let mut enabled = self.autostart;
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if switch(ui, &mut enabled, "开机启动并最小化") {
                                            match settings::launcher().and_then(|l| {
                                                if enabled {
                                                    l.enable()?;
                                                } else {
                                                    l.disable()?;
                                                }
                                                Ok(())
                                            }) {
                                                Ok(()) => {
                                                    self.autostart = enabled;
                                                    self.status = if enabled {
                                                        "已开启开机启动"
                                                    } else {
                                                        "已关闭开机启动"
                                                    }
                                                    .into();
                                                }
                                                Err(e) => self.error = format!("自启设置失败：{e}"),
                                            }
                                        }
                                    },
                                );
                            });
                            hint(ui, "登录后最小化，自动恢复上次设置");
                        });
                        ui.horizontal_wrapped(|ui| {
                            if self.switching {
                                ui.spinner();
                            }
                            let summary = if self.switching {
                                "正在获取并应用壁纸…"
                            } else if !self.error.is_empty() {
                                "操作未完成，请查看详情"
                            } else if self.status.starts_with("已更换壁纸：") {
                                "壁纸已更新"
                            } else {
                                &self.status
                            };
                            ui.label(RichText::new(summary).size(12.0).color(MUTED))
                                .on_hover_text(&self.status);
                        });
                        if !self.error.is_empty() {
                            egui::CollapsingHeader::new("错误详情").show(ui, |ui| {
                                ui.colored_label(ui.visuals().error_fg_color, &self.error);
                            });
                        }
                        ui.horizontal(|ui| {
                            hint(ui, "Wallhaven · SFW");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    hint(ui, format!("v{VERSION}"));
                                },
                            );
                        });
                    });
            });
    }
}
