// egui UI 相关方法（draw_axis_gizmo 与 update_egui）
// 从 main.rs 抽离，便于维护
//
// 面板按功能分为若干小节（header / 状态 / 控制 / 添加黑洞 / 添加天体 / 帮助），
// 每个小节一个方法，避免巨型函数与末尾"写回"样板。
// 添加黑洞/天体时做实时安全校验：位置不安全（瞬间合并/被吞噬/碰撞）时
// 按钮禁用并给出原因，可一键"自动避让"到安全位置。

use super::physics;
use super::App;
use super::SpawnParams;
use super::UiLang;

#[cfg(target_family = "wasm")]
fn set_panel_visible(visible: bool) {
    if let Some(window) = web_sys::window() {
        let js_val = js_sys::Reflect::get(&window, &"__setPanelVisible".into())
            .unwrap_or_else(|_| wasm_bindgen::JsValue::UNDEFINED);
        if let Some(func) = js_val.dyn_ref::<js_sys::Function>() {
            let _ = func.call1(&window, &wasm_bindgen::JsValue::from_bool(visible));
        }
    }
}

#[cfg(target_family = "wasm")]
use wasm_bindgen::JsCast;

macro_rules! t {
    ($self:ident, $zh:expr, $en:expr) => {
        match $self.ui_lang {
            UiLang::Zh => $zh,
            UiLang::En => $en,
        }
    };
}

/// 生成添加安全性提示文本（按当前语言）
fn spawn_warning_text(lang: UiLang, err: &physics::SpawnError) -> String {
    match *err {
        physics::SpawnError::AtCapacity { max } => match lang {
            UiLang::Zh => format!("已达数量上限 ({})", max),
            UiLang::En => format!("At capacity ({})", max),
        },
        physics::SpawnError::TooCloseToBlackHole { index, required } => match lang {
            UiLang::Zh => format!("⚠ 距黑洞 {} 过近（需 ≥ {:.2}）", index + 1, required),
            UiLang::En => format!("⚠ Too close to BH {} (need ≥ {:.2})", index + 1, required),
        },
        physics::SpawnError::TooCloseToBody { index, required } => match lang {
            UiLang::Zh => format!("⚠ 距天体 {} 过近（需 ≥ {:.2}）", index + 1, required),
            UiLang::En => format!("⚠ Too close to body {} (need ≥ {:.2})", index + 1, required),
        },
    }
}

/// 显示生成安全性状态（绿=可添加，红=冲突原因）
fn spawn_status_label(ui: &mut egui::Ui, lang: UiLang, check: &Result<(), physics::SpawnError>) {
    match check {
        Ok(()) => {
            ui.colored_label(
                egui::Color32::from_rgb(100, 255, 120),
                match lang {
                    UiLang::Zh => "✓ 位置安全，可添加",
                    UiLang::En => "✓ Safe to add",
                },
            );
        }
        Err(err) => {
            ui.colored_label(
                egui::Color32::from_rgb(255, 120, 120),
                spawn_warning_text(lang, err),
            );
        }
    }
}

impl App {
    /// 在画面左上角绘制三轴坐标系参考（空间定向）
    /// 将世界坐标系的 X/Y/Z 轴投影到屏幕，显示当前相机朝向
    fn draw_axis_gizmo(&self) {
        use egui::{Color32, Stroke, Vec2};

        // 取相机 orientation 的逆变换（共轭），把世界轴变换到相机局部空间
        // 相机局部: +X=右, +Y=上, +Z=朝向目标(前方)
        let orient_inv = self.camera.orientation.inverse();
        let world_axes: [(nalgebra::Vector3<f32>, Color32, &str); 3] = [
            (
                nalgebra::Vector3::new(1.0, 0.0, 0.0),
                Color32::from_rgb(255, 80, 80),
                "X",
            ),
            (
                nalgebra::Vector3::new(0.0, 1.0, 0.0),
                Color32::from_rgb(80, 255, 80),
                "Y",
            ),
            (
                nalgebra::Vector3::new(0.0, 0.0, 1.0),
                Color32::from_rgb(80, 130, 255),
                "Z",
            ),
        ];

        egui::Area::new(egui::Id::new("axis_gizmo"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
            .order(egui::Order::Foreground)
            .show(&self.egui_ctx, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(90.0, 90.0), egui::Sense::hover());
                let painter = ui.painter();
                let center = rect.center();
                let axis_len = 30.0;

                // 绘制底色圆
                painter.circle_filled(center, axis_len + 6.0, Color32::from_black_alpha(120));

                // 计算每个轴的屏幕投影
                // 按深度（local.z）排序，远的先画
                let mut projected: Vec<(Vec2, Color32, &str, f32)> = world_axes
                    .iter()
                    .map(|(axis, color, label)| {
                        let local = orient_inv * axis;
                        // 屏幕: x=local.x(右), y=-local.y(屏幕Y向下，上为负)
                        let screen = Vec2::new(local.x, -local.y) * axis_len;
                        (screen, *color, *label, local.z)
                    })
                    .collect();
                projected.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap());

                for (screen, color, label, _depth) in &projected {
                    let end = center + *screen;
                    painter.line_segment([center, end], Stroke::new(2.5_f32, *color));
                    // 轴标签放在轴末端外侧
                    let dir = if screen.length() > 0.001 {
                        screen.normalized()
                    } else {
                        Vec2::ZERO
                    };
                    let label_pos = end + dir * 8.0;
                    painter.text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        *label,
                        egui::FontId::proportional(13.0),
                        *color,
                    );
                }

                // 中心点
                painter.circle_filled(center, 2.5, Color32::from_rgb(220, 220, 220));
            });
    }

    /// 面板标题栏：标题 + 语言切换 + 隐藏按钮
    fn ui_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(t!(self, "🌌 黑洞模拟", "🌌 BH Sim"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if ui
                    .small_button(t!(self, "◀", "◀"))
                    .on_hover_text(t!(self, "隐藏面板", "Hide panel"))
                    .clicked()
                {
                    self.ui_show_panel = false;
                    #[cfg(target_family = "wasm")]
                    set_panel_visible(false);
                }
                ui.add_space(4.0);
                let zh_selected = matches!(self.ui_lang, UiLang::Zh);
                let en_selected = matches!(self.ui_lang, UiLang::En);
                if ui
                    .selectable_label(zh_selected, t!(self, "中", "Zh"))
                    .on_hover_text(t!(self, "切换到中文", "Switch to Chinese"))
                    .clicked()
                {
                    self.ui_lang = UiLang::Zh;
                }
                if ui
                    .selectable_label(en_selected, t!(self, "EN", "EN"))
                    .on_hover_text(t!(self, "切换到英文", "Switch to English"))
                    .clicked()
                {
                    self.ui_lang = UiLang::En;
                }
            });
        });
    }

    /// 状态信息小节：模拟状态 / 时间 / 数量 / 黑洞列表
    fn ui_status_section(&self, ui: &mut egui::Ui) {
        let time = self.sim.time;
        let phase = self.sim.phase_string(matches!(self.ui_lang, UiLang::En));
        let bh_count = self.sim.black_hole_count();

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!(self, "━━━ 状态信息 ━━━", "━━━ Status ━━━"))
                .strong()
                .color(egui::Color32::from_rgb(120, 180, 255)),
        );

        ui.horizontal(|ui| {
            ui.label(t!(self, "状态:", "Status:"));
            ui.colored_label(egui::Color32::from_rgb(100, 255, 120), &phase);
        });
        ui.label(format!(
            "{} {:.2} {}",
            t!(self, "模拟时间:", "Sim Time:"),
            time,
            t!(self, "s", "s")
        ));
        ui.label(format!(
            "{} {}",
            t!(self, "黑洞数量:", "Black Holes:"),
            bh_count
        ));
        ui.label(format!(
            "{} {}",
            t!(self, "天体数量:", "Bodies:"),
            self.sim.bodies.len()
        ));
        ui.label(format!(
            "{} {}",
            t!(self, "碎片粒子:", "Debris:"),
            self.sim.debris.len()
        ));
        if bh_count >= 2 {
            ui.label(t!(
                self,
                "引力波: 连续波模型（双黑洞）",
                "Grav. Waves: Continuous (BBH)"
            ));
        }

        // 列出所有黑洞
        if bh_count > 0 {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(t!(self, "── 黑洞列表 ──", "── Black Holes ──"))
                    .size(12.0)
                    .weak(),
            );
            for (i, bh) in self.sim.black_holes.iter().enumerate() {
                ui.label(format!(
                    "  {}: M={:.2}  pos=({:.1},{:.1},{:.1})",
                    i + 1,
                    bh.mass,
                    bh.pos.x,
                    bh.pos.y,
                    bh.pos.z
                ));
            }
        }
    }

    /// 模拟控制小节：暂停/重置、显示开关、网格配置、模拟速度
    fn ui_controls_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!(self, "━━━ 模拟控制 ━━━", "━━━ Controls ━━━"))
                .strong()
                .color(egui::Color32::from_rgb(120, 180, 255)),
        );

        ui.horizontal(|ui| {
            if ui
                .button(if self.sim.paused {
                    t!(self, "▶  继续", "▶  Resume")
                } else {
                    t!(self, "⏸ 暂停", "⏸ Pause")
                })
                .clicked()
            {
                self.sim.paused = !self.sim.paused;
            }
            if ui.button(t!(self, "🔄 重置", "🔄 Reset")).clicked() {
                self.sim.reset();
                self.spawn_bh = SpawnParams::default_black_hole();
                self.spawn_body = SpawnParams::default_body();
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.ui_show_background, t!(self, "背景", "Background"));
            ui.checkbox(&mut self.ui_show_bodies, t!(self, "天体", "Bodies"));
        });
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.sim.show_gravity_waves,
                t!(self, "引力波", "Grav. Waves"),
            );
            ui.checkbox(&mut self.ui_show_trails, t!(self, "轨迹预测", "Trajectory"));
        });

        if self.sim.show_gravity_waves {
            ui.add_space(4.0);
            ui.separator();
            ui.label(
                egui::RichText::new(t!(self, "网格配置", "Grid Settings"))
                    .strong()
                    .color(egui::Color32::from_rgb(120, 180, 255)),
            );

            let mut grid_size_i = self.sim.grid_size as i32;
            ui.add(
                egui::Slider::new(&mut grid_size_i, 6..=50)
                    .text(t!(self, "格点数量", "Grid Size"))
                    .step_by(1.0),
            );

            let mut grid_spacing_f = self.sim.grid_spacing;
            ui.add(
                egui::Slider::new(&mut grid_spacing_f, 2.0..=30.0)
                    .text(t!(self, "格点间距", "Grid Spacing"))
                    .step_by(0.5),
            );

            ui.checkbox(
                &mut self.sim.tendex_three_planes,
                t!(self, "仅三正交面", "3 Planes Only"),
            );

            self.sim
                .set_grid_params(grid_size_i as usize, grid_spacing_f);
        }

        ui.add_space(4.0);

        ui.add(
            egui::Slider::new(&mut self.sim.sim_speed, 0.1..=50.0)
                .text(t!(self, "模拟速度", "Sim Speed"))
                .step_by(0.05),
        );
    }

    /// 添加黑洞小节：参数编辑 + 实时安全校验 + 添加/自动避让
    fn ui_add_black_hole_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!(self, "━━━ 添加黑洞 ━━━", "━━━ Add Black Hole ━━━"))
                .strong()
                .color(egui::Color32::from_rgb(255, 180, 100)),
        );

        ui.add(
            egui::Slider::new(&mut self.spawn_bh.mass, 0.1..=10.0)
                .text(t!(self, "质量", "Mass"))
                .step_by(0.1),
        );

        ui.label(t!(self, "位置:", "Position:"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.spawn_bh.pos.x).speed(0.2).prefix("x: "));
            ui.add(egui::DragValue::new(&mut self.spawn_bh.pos.y).speed(0.2).prefix("y: "));
            ui.add(egui::DragValue::new(&mut self.spawn_bh.pos.z).speed(0.2).prefix("z: "));
        });

        ui.label(t!(self, "速度:", "Velocity:"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.spawn_bh.vel.x).speed(0.05).prefix("vx: "));
            ui.add(egui::DragValue::new(&mut self.spawn_bh.vel.y).speed(0.05).prefix("vy: "));
            ui.add(egui::DragValue::new(&mut self.spawn_bh.vel.z).speed(0.05).prefix("vz: "));
        });

        // 实时安全校验：防止新黑洞与已有黑洞瞬间合并 / 撕裂已有天体
        let candidate = self.spawn_bh.to_black_hole();
        let check = self.sim.check_black_hole_spawn(&candidate);
        spawn_status_label(ui, self.ui_lang, &check);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let resp = ui.add_enabled(
                check.is_ok(),
                egui::Button::new(
                    egui::RichText::new(t!(self, "➕ 添加黑洞", "➕ Add Black Hole"))
                        .size(14.0)
                        .strong(),
                ),
            );
            let resp = if let Err(err) = &check {
                resp.on_disabled_hover_text(spawn_warning_text(self.ui_lang, err))
            } else {
                resp.on_hover_text(t!(
                    self,
                    "添加后自动暂停，可查看轨迹预测",
                    "Auto-pauses after adding"
                ))
            };
            if resp.clicked() {
                self.sim.paused = true;
                if let Err(err) = self.sim.add_black_hole(candidate) {
                    eprintln!("添加黑洞失败: {:?}", err);
                }
            }
            if ui
                .button(t!(self, "🛡 自动避让", "🛡 Safe Pos"))
                .on_hover_text(t!(
                    self,
                    "将位置调整到安全距离外",
                    "Move to a safe distance"
                ))
                .clicked()
            {
                self.spawn_bh.pos = self.sim.safe_black_hole_pos(&candidate);
            }
        });

        if self.sim.black_hole_count() >= physics::MAX_BH {
            ui.label(
                egui::RichText::new(t!(self, "已达最大黑洞数 (8)", "Max black holes (8)"))
                    .color(egui::Color32::RED)
                    .small(),
            );
        }
    }

    /// 添加天体小节：参数编辑 + 实时安全校验 + 添加/自动避让
    fn ui_add_body_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!(self, "━━━ 添加天体 ━━━", "━━━ Add Body ━━━"))
                .strong()
                .color(egui::Color32::from_rgb(100, 255, 180)),
        );
        ui.label(
            egui::RichText::new(t!(
                self,
                "天体被洛希极限撕裂后形成吸积盘",
                "Tidal disruption forms accretion disk"
            ))
            .weak()
            .small(),
        );

        ui.add(
            egui::Slider::new(&mut self.spawn_body.mass, 0.05..=5.0)
                .text(t!(self, "质量", "Mass"))
                .step_by(0.05),
        );

        ui.label(t!(self, "位置:", "Position:"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.spawn_body.pos.x).speed(0.2).prefix("x: "));
            ui.add(egui::DragValue::new(&mut self.spawn_body.pos.y).speed(0.2).prefix("y: "));
            ui.add(egui::DragValue::new(&mut self.spawn_body.pos.z).speed(0.2).prefix("z: "));
        });

        ui.label(t!(self, "速度:", "Velocity:"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.spawn_body.vel.x).speed(0.05).prefix("vx: "));
            ui.add(egui::DragValue::new(&mut self.spawn_body.vel.y).speed(0.05).prefix("vy: "));
            ui.add(egui::DragValue::new(&mut self.spawn_body.vel.z).speed(0.05).prefix("vz: "));
        });

        // 实时安全校验：防止天体落入黑洞视界/洛希极限或被已有天体撞碎
        let candidate = self.spawn_body.to_body();
        let check = self.sim.check_body_spawn(&candidate);
        spawn_status_label(ui, self.ui_lang, &check);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let resp = ui.add_enabled(
                check.is_ok(),
                egui::Button::new(
                    egui::RichText::new(t!(self, "➕ 添加天体", "➕ Add Body"))
                        .size(14.0)
                        .strong(),
                ),
            );
            let resp = if let Err(err) = &check {
                resp.on_disabled_hover_text(spawn_warning_text(self.ui_lang, err))
            } else {
                resp.on_hover_text(t!(
                    self,
                    "添加后自动暂停，可查看轨迹预测",
                    "Auto-pauses after adding"
                ))
            };
            if resp.clicked() {
                self.sim.paused = true;
                if let Err(err) = self.sim.add_body(candidate) {
                    eprintln!("添加天体失败: {:?}", err);
                }
            }
            if ui
                .button(t!(self, "🛡 自动避让", "🛡 Safe Pos"))
                .on_hover_text(t!(
                    self,
                    "将位置调整到安全距离外",
                    "Move to a safe distance"
                ))
                .clicked()
            {
                self.spawn_body.pos = self.sim.safe_body_pos(&candidate);
            }
        });

        if self.sim.bodies.len() >= physics::MAX_BODIES {
            ui.label(
                egui::RichText::new(t!(self, "已达最大天体数 (16)", "Max bodies (16)"))
                    .color(egui::Color32::RED)
                    .small(),
            );
        }
    }

    /// 操作说明小节
    fn ui_help_section(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!(self, "━━━ 操作说明 ━━━", "━━━ Controls ━━━"))
                .strong()
                .color(egui::Color32::from_rgb(120, 180, 255)),
        );
        ui.horizontal(|ui| {
            ui.label(t!(self, "左键: 旋转", "LMB: Rotate"));
            ui.label(t!(self, "滚轮: 缩放", "Wheel: Zoom"));
        });
        ui.horizontal(|ui| {
            ui.label(t!(self, "WASD: 平移", "WASD: Pan"));
            ui.label(t!(self, "QE: 上下", "QE: Up/Down"));
        });
        ui.horizontal(|ui| {
            ui.label(t!(self, "方向键: 旋转", "Arrows: Rotate"));
            ui.label(t!(self, "SPACE: 暂停", "SPACE: Pause"));
        });
    }

    /// 底部状态栏：暂停状态 / 相位 / 时间 / 各类数量
    fn ui_status_bar(&self) {
        let paused = self.sim.paused;
        let phase = self.sim.phase_string(matches!(self.ui_lang, UiLang::En));
        let time = self.sim.time;
        let bh_count = self.sim.black_hole_count();
        let n_bodies = self.sim.bodies.len();
        let n_debris = self.sim.debris.len();

        egui::TopBottomPanel::bottom("状态栏").show(&self.egui_ctx, |ui| {
            ui.horizontal(|ui| {
                let status_color = if paused {
                    egui::Color32::from_rgb(255, 200, 80)
                } else {
                    egui::Color32::from_rgb(100, 255, 120)
                };
                ui.colored_label(status_color, "●");
                ui.label(if paused {
                    t!(self, "已暂停", "Paused")
                } else {
                    t!(self, "运行中", "Running")
                });
                ui.separator();
                ui.label(&phase);
                ui.separator();
                ui.label(format!(
                    "{} {:.1}{}",
                    t!(self, "时间:", "Time:"),
                    time,
                    t!(self, "s", "s")
                ));
                ui.separator();
                ui.label(format!("{} {}", t!(self, "黑洞:", "BH:"), bh_count));
                ui.separator();
                ui.label(format!("{} {}", t!(self, "天体:", "Body:"), n_bodies));
                ui.separator();
                ui.label(format!("{} {}", t!(self, "碎片:", "Debris:"), n_debris));
            });
        });
    }

    pub(crate) fn update_egui(&mut self, window: &winit::window::Window) {
        let raw_input = if let Some(state) = self.egui_state.as_mut() {
            state.take_egui_input(window)
        } else {
            egui::RawInput::default()
        };

        self.egui_ctx.begin_pass(raw_input);

        // 左上角三轴坐标系参考（空间定向）
        self.draw_axis_gizmo();

        // 面板隐藏时，右上角显示悬浮切换按钮
        if !self.ui_show_panel {
            let btn_text = t!(self, "面板 ▶", "Panel ▶");
            let ctx = self.egui_ctx.clone();
            egui::Area::new(egui::Id::new("panel_toggle"))
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 8.0))
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    let frame = egui::Frame::window(&ctx.style());
                    frame.show(ui, |ui| {
                        if ui.button(btn_text).clicked() {
                            self.ui_show_panel = true;
                            #[cfg(target_family = "wasm")]
                            set_panel_visible(true);
                        }
                    });
                });
        }

        if self.ui_show_panel {
            let screen_width = self.egui_ctx.screen_rect().width();
            let panel_width = if screen_width < 768.0 {
                screen_width * 0.8
            } else {
                260.0
            };
            let ctx = self.egui_ctx.clone();
            egui::SidePanel::right("控制面板")
                .default_width(panel_width)
                .min_width(200.0)
                .resizable(true)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.add_space(8.0);
                            self.ui_header(ui);
                            self.ui_status_section(ui);
                            self.ui_controls_section(ui);
                            self.ui_add_black_hole_section(ui);
                            self.ui_add_body_section(ui);
                            self.ui_help_section(ui);
                        });
                });
        }

        self.ui_status_bar();
    }
}
