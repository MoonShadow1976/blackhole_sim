// egui UI 相关方法（draw_axis_gizmo 与 update_egui）
// 从 main.rs 抽离，便于维护

use super::physics;
use super::App;
use super::UiLang;

macro_rules! t {
    ($self:ident, $zh:expr, $en:expr) => {
        match $self.ui_lang {
            UiLang::Zh => $zh,
            UiLang::En => $en,
        }
    };
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

    pub(crate) fn update_egui(&mut self, window: &winit::window::Window) {
        let raw_input = if let Some(state) = self.egui_state.as_mut() {
            state.take_egui_input(window)
        } else {
            egui::RawInput::default()
        };

        self.egui_ctx.begin_pass(raw_input);

        // 左上角三轴坐标系参考（空间定向）
        self.draw_axis_gizmo();

        let time = self.sim.time;
        let phase = self.sim.phase_string(matches!(self.ui_lang, UiLang::En));
        let bh_count = self.sim.black_hole_count();

        let mut show_waves = self.ui_show_waves;
        let mut three_planes = self.sim.tendex_three_planes;
        let mut show_background = self.ui_show_background;
        let mut show_bodies = self.ui_show_bodies;
        let mut show_trails = self.ui_show_trails;
        let mut sim_speed = self.ui_sim_speed;
        let mut paused = self.ui_paused;
        let mut reset = false;
        let mut add_bh = false;

        let mut add_mass = self.ui_add_mass;
        let mut add_px = self.ui_add_pos_x;
        let mut add_py = self.ui_add_pos_y;
        let mut add_pz = self.ui_add_pos_z;
        let mut add_vx = self.ui_add_vel_x;
        let mut add_vy = self.ui_add_vel_y;
        let mut add_vz = self.ui_add_vel_z;

        let mut add_body = false;
        let mut body_mass = self.ui_body_mass;
        let mut body_px = self.ui_body_pos_x;
        let mut body_py = self.ui_body_pos_y;
        let mut body_pz = self.ui_body_pos_z;
        let mut body_vx = self.ui_body_vel_x;
        let mut body_vy = self.ui_body_vel_y;
        let mut body_vz = self.ui_body_vel_z;

        egui::SidePanel::right("控制面板")
            .min_width(300.0)
            .resizable(true)
            .show(&self.egui_ctx, |ui| {
                ui.add_space(8.0);

                // 标题 + 语言切换（右上角）
                ui.horizontal(|ui| {
                    ui.heading(t!(self, "🌌 黑洞模拟系统", "🌌 Black Hole Sim"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        let mut lang = self.ui_lang;
                        let zh_selected = matches!(lang, UiLang::Zh);
                        let en_selected = matches!(lang, UiLang::En);
                        if ui
                            .selectable_label(zh_selected, "中文")
                            .on_hover_text(t!(self, "切换到中文", "Switch to Chinese"))
                            .clicked()
                        {
                            lang = UiLang::Zh;
                        }
                        if ui
                            .selectable_label(en_selected, "EN")
                            .on_hover_text(t!(self, "切换到英文", "Switch to English"))
                            .clicked()
                        {
                            lang = UiLang::En;
                        }
                        self.ui_lang = lang;
                    });
                });

                ui.label(
                    egui::RichText::new(t!(self, "N体黑洞碰撞模拟", "N-Body Black Hole Collision"))
                        .weak()
                        .small(),
                );
                ui.add_space(8.0);
                ui.separator();

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
                        .button(if paused {
                            t!(self, "▶  继续", "▶  Resume")
                        } else {
                            t!(self, "⏸ 暂停", "⏸ Pause")
                        })
                        .clicked()
                    {
                        paused = !paused;
                    }
                    if ui.button(t!(self, "🔄 重置", "🔄 Reset")).clicked() {
                        reset = true;
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut show_background, t!(self, "背景", "Background"));
                    ui.checkbox(&mut show_bodies, t!(self, "天体", "Bodies"));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut show_waves, t!(self, "引力波", "Grav. Waves"));
                    ui.checkbox(&mut show_trails, t!(self, "轨迹预测", "Trajectory"));
                });

                if show_waves {
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

                    ui.checkbox(&mut three_planes, t!(self, "仅三正交面", "3 Planes Only"));

                    self.sim
                        .set_grid_params(grid_size_i as usize, grid_spacing_f);
                }

                ui.add_space(4.0);

                ui.add(
                    egui::Slider::new(&mut sim_speed, 0.1..=50.0)
                        .text(t!(self, "模拟速度", "Sim Speed"))
                        .step_by(0.05),
                );

                ui.add_space(4.0);
                ui.separator();

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(t!(self, "━━━ 添加黑洞 ━━━", "━━━ Add Black Hole ━━━"))
                        .strong()
                        .color(egui::Color32::from_rgb(255, 180, 100)),
                );

                ui.add(
                    egui::Slider::new(&mut add_mass, 0.1..=10.0)
                        .text(t!(self, "质量", "Mass"))
                        .step_by(0.1),
                );

                ui.label(t!(self, "位置:", "Position:"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut add_px).speed(0.2).prefix("x: "));
                    ui.add(egui::DragValue::new(&mut add_py).speed(0.2).prefix("y: "));
                    ui.add(egui::DragValue::new(&mut add_pz).speed(0.2).prefix("z: "));
                });

                ui.label(t!(self, "速度:", "Velocity:"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut add_vx).speed(0.05).prefix("vx: "));
                    ui.add(egui::DragValue::new(&mut add_vy).speed(0.05).prefix("vy: "));
                    ui.add(egui::DragValue::new(&mut add_vz).speed(0.05).prefix("vz: "));
                });

                ui.add_space(6.0);
                if ui
                    .button(
                        egui::RichText::new(t!(
                            self,
                            "➕ 添加黑洞 (暂停)",
                            "➕ Add Black Hole (Paused)"
                        ))
                        .size(14.0)
                        .strong(),
                    )
                    .clicked()
                    && self.sim.black_hole_count() < 8
                {
                    add_bh = true;
                }
                if self.sim.black_hole_count() >= 8 {
                    ui.label(
                        egui::RichText::new(t!(self, "已达最大黑洞数 (8)", "Max black holes (8)"))
                            .color(egui::Color32::RED)
                            .small(),
                    );
                }

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
                    egui::Slider::new(&mut body_mass, 0.05..=5.0)
                        .text(t!(self, "质量", "Mass"))
                        .step_by(0.05),
                );

                ui.label(t!(self, "位置:", "Position:"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut body_px).speed(0.2).prefix("x: "));
                    ui.add(egui::DragValue::new(&mut body_py).speed(0.2).prefix("y: "));
                    ui.add(egui::DragValue::new(&mut body_pz).speed(0.2).prefix("z: "));
                });

                ui.label(t!(self, "速度:", "Velocity:"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut body_vx)
                            .speed(0.05)
                            .prefix("vx: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut body_vy)
                            .speed(0.05)
                            .prefix("vy: "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut body_vz)
                            .speed(0.05)
                            .prefix("vz: "),
                    );
                });

                ui.add_space(6.0);
                if ui
                    .button(
                        egui::RichText::new(t!(self, "➕ 添加天体 (暂停)", "➕ Add Body (Paused)"))
                            .size(14.0)
                            .strong(),
                    )
                    .clicked()
                {
                    add_body = true;
                }

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
            });

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
                ui.label(format!(
                    "{} {}",
                    t!(self, "天体:", "Body:"),
                    self.sim.bodies.len()
                ));
                ui.separator();
                ui.label(format!(
                    "{} {}",
                    t!(self, "碎片:", "Debris:"),
                    self.sim.debris.len()
                ));
            });
        });

        self.ui_show_waves = show_waves;
        self.ui_show_background = show_background;
        self.ui_show_bodies = show_bodies;
        self.ui_show_trails = show_trails;
        self.ui_sim_speed = sim_speed;
        self.ui_paused = paused;
        self.ui_reset = reset;

        self.ui_add_mass = add_mass;
        self.ui_add_pos_x = add_px;
        self.ui_add_pos_y = add_py;
        self.ui_add_pos_z = add_pz;
        self.ui_add_vel_x = add_vx;
        self.ui_add_vel_y = add_vy;
        self.ui_add_vel_z = add_vz;

        self.ui_body_mass = body_mass;
        self.ui_body_pos_x = body_px;
        self.ui_body_pos_y = body_py;
        self.ui_body_pos_z = body_pz;
        self.ui_body_vel_x = body_vx;
        self.ui_body_vel_y = body_vy;
        self.ui_body_vel_z = body_vz;

        self.sim.show_gravity_waves = show_waves;
        self.sim.tendex_three_planes = three_planes;
        self.sim.sim_speed = sim_speed;
        self.sim.paused = paused;

        if add_bh {
            self.sim.paused = true;
            self.ui_paused = true;
            self.sim.add_black_hole(physics::BlackHole {
                mass: add_mass,
                pos: nalgebra::Vector3::new(add_px, add_py, add_pz),
                vel: nalgebra::Vector3::new(add_vx, add_vy, add_vz),
            });
        }

        if add_body {
            self.sim.paused = true;
            self.ui_paused = true;
            self.sim.add_body(physics::CelestialBody {
                mass: body_mass,
                pos: nalgebra::Vector3::new(body_px, body_py, body_pz),
                vel: nalgebra::Vector3::new(body_vx, body_vy, body_vz),
                hardness: 1.0, // 默认岩石材质
            });
        }

        if reset {
            self.sim.reset();
            self.ui_add_mass = 2.0;
            self.ui_add_pos_x = 7.5;
            self.ui_add_pos_y = 0.0;
            self.ui_add_pos_z = 0.0;
            self.ui_add_vel_x = 0.0;
            self.ui_add_vel_y = 0.0;
            self.ui_add_vel_z = -0.3;
        }
    }
}
