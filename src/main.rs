// 黑洞碰撞模拟 - 主入口
// 使用 winit 创建窗口，wgpu 进行 3D 渲染，egui 提供控制面板

mod camera;
mod geometry;
mod physics;
mod renderer;

use std::time::Instant;

use camera::OrbitCamera;
use physics::Simulation;
use renderer::Renderer;

use egui_wgpu::Renderer as EguiRenderer;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

/// 应用状态
struct App {
    renderer: Option<Renderer>,
    camera: OrbitCamera,
    sim: Simulation,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    egui_renderer: Option<EguiRenderer>,
    last_frame: Option<Instant>,
    mouse_pressed: bool,
    last_mouse_pos: Option<PhysicalPosition<f64>>,
    window: Option<&'static Window>,
    // UI 临时状态
    ui_show_waves: bool,
    ui_sim_speed: f32,
    ui_paused: bool,
    ui_reset: bool,
    // 添加黑洞的参数
    ui_add_mass: f32,
    ui_add_pos_x: f32,
    ui_add_pos_y: f32,
    ui_add_pos_z: f32,
    ui_add_vel_x: f32,
    ui_add_vel_y: f32,
    ui_add_vel_z: f32,
    // 添加天体的参数
    ui_body_mass: f32,
    ui_body_pos_x: f32,
    ui_body_pos_y: f32,
    ui_body_pos_z: f32,
    ui_body_vel_x: f32,
    ui_body_vel_y: f32,
    ui_body_vel_z: f32,
    // 轨迹预测
    ui_show_trails: bool,
}

impl App {
    fn new() -> Self {
        let sim = Simulation::new();
        Self {
            renderer: None,
            camera: OrbitCamera::new(),
            ui_show_waves: sim.show_gravity_waves,
            ui_sim_speed: sim.sim_speed,
            ui_paused: sim.paused,
            sim,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            egui_renderer: None,
            last_frame: None,
            mouse_pressed: false,
            last_mouse_pos: None,
            window: None,
            ui_reset: false,
            ui_add_mass: 1.5,
            ui_add_pos_x: 10.0,
            ui_add_pos_y: 0.0,
            ui_add_pos_z: 0.0,
            ui_add_vel_x: 0.0,
            ui_add_vel_y: 0.0,
            ui_add_vel_z: 0.5,
            ui_body_mass: 0.3,
            ui_body_pos_x: 6.0,
            ui_body_pos_y: 0.5,
            ui_body_pos_z: 0.0,
            ui_body_vel_x: 0.0,
            ui_body_vel_y: 0.0,
            ui_body_vel_z: 0.6,
            ui_show_trails: true,
        }
    }

    /// 在画面左上角绘制三轴坐标系参考（空间定向）
    /// 将世界坐标系的 X/Y/Z 轴投影到屏幕，显示当前相机朝向
    fn draw_axis_gizmo(&self) {
        use egui::{Color32, Stroke, Vec2};

        // 取相机 orientation 的逆变换（共轭），把世界轴变换到相机局部空间
        // 相机局部: +X=右, +Y=上, +Z=朝向目标(前方)
        let orient_inv = self.camera.orientation.inverse();
        let world_axes: [(nalgebra::Vector3<f32>, Color32, &str); 3] = [
            (nalgebra::Vector3::new(1.0, 0.0, 0.0), Color32::from_rgb(255, 80, 80), "X"),
            (nalgebra::Vector3::new(0.0, 1.0, 0.0), Color32::from_rgb(80, 255, 80), "Y"),
            (nalgebra::Vector3::new(0.0, 0.0, 1.0), Color32::from_rgb(80, 130, 255), "Z"),
        ];

        egui::Area::new(egui::Id::new("axis_gizmo"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
            .order(egui::Order::Foreground)
            .show(&self.egui_ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(90.0, 90.0),
                    egui::Sense::hover(),
                );
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
                    painter.line_segment(
                        [center, end],
                        Stroke::new(2.5, *color),
                    );
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

    fn update_egui(&mut self, window: &Window) {
        let raw_input = if let Some(state) = self.egui_state.as_mut() {
            state.take_egui_input(window)
        } else {
            egui::RawInput::default()
        };

        self.egui_ctx.begin_pass(raw_input);

        // 左上角三轴坐标系参考（空间定向）
        self.draw_axis_gizmo();

        let time = self.sim.time;
        let phase = self.sim.phase_string();
        let wave_count = self.sim.waves.len();
        let bh_count = self.sim.black_hole_count();

        let mut show_waves = self.ui_show_waves;
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
                    ui.heading("🌌 黑洞模拟系统");
                    ui.label(
                        egui::RichText::new("N-Body Black Hole Simulation")
                            .weak()
                            .small(),
                    );
                    ui.add_space(8.0);
                    ui.separator();

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("━━━ 状态信息 ━━━")
                            .strong()
                            .color(egui::Color32::from_rgb(120, 180, 255)),
                    );

                    ui.horizontal(|ui| {
                        ui.label("状态:");
                        ui.colored_label(egui::Color32::from_rgb(100, 255, 120), &phase);
                    });
                    ui.label(format!("模拟时间: {:.2} s", time));
                    ui.label(format!("黑洞数量: {}", bh_count));
                    ui.label(format!("天体数量: {}", self.sim.bodies.len()));
                    ui.label(format!("碎片粒子: {}", self.sim.debris.len()));
                    ui.label(format!("引力波数量: {}", wave_count));

                    // 列出所有黑洞
                    if bh_count > 0 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("── 黑洞列表 ──")
                                .size(12.0)
                                .weak(),
                        );
                        for (i, bh) in self.sim.black_holes.iter().enumerate() {
                            ui.label(format!(
                                "  {}: M={:.2}  pos=({:.1},{:.1},{:.1})",
                                i + 1, bh.mass, bh.pos.x, bh.pos.y, bh.pos.z
                            ));
                        }
                    }

                    ui.add_space(4.0);
                    ui.separator();

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("━━━ 模拟控制 ━━━")
                            .strong()
                            .color(egui::Color32::from_rgb(120, 180, 255)),
                    );

                    ui.horizontal(|ui| {
                        if ui.button(if paused { "▶  继续" } else { "⏸ 暂停" }).clicked() {
                            paused = !paused;
                        }
                        if ui.button("🔄 重置").clicked() {
                            reset = true;
                        }
                    });

                    ui.add_space(4.0);
                    ui.checkbox(&mut show_waves, "显示引力波");
                    ui.checkbox(&mut show_trails, "显示轨迹预测");
                    ui.add_space(4.0);

                    ui.add(
                        egui::Slider::new(&mut sim_speed, 0.1..=20.0)
                            .text("模拟速度")
                            .step_by(0.05),
                    );

                    ui.add_space(4.0);
                    ui.separator();

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("━━━ 添加黑洞 ━━━")
                            .strong()
                            .color(egui::Color32::from_rgb(255, 180, 100)),
                    );

                    ui.add(
                        egui::Slider::new(&mut add_mass, 0.1..=5.0)
                            .text("质量")
                            .step_by(0.1),
                    );

                    ui.label("位置:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut add_px).speed(0.2).prefix("x: "));
                        ui.add(egui::DragValue::new(&mut add_py).speed(0.2).prefix("y: "));
                        ui.add(egui::DragValue::new(&mut add_pz).speed(0.2).prefix("z: "));
                    });

                    ui.label("速度:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut add_vx).speed(0.05).prefix("vx: "));
                        ui.add(egui::DragValue::new(&mut add_vy).speed(0.05).prefix("vy: "));
                        ui.add(egui::DragValue::new(&mut add_vz).speed(0.05).prefix("vz: "));
                    });

                    ui.add_space(6.0);
                    if ui.button(egui::RichText::new("➕ 添加黑洞 (暂停)").size(14.0).strong())
                        .clicked()
                    {
                        if self.sim.black_hole_count() < 8 {
                            add_bh = true;
                        }
                    }
                    if self.sim.black_hole_count() >= 8 {
                        ui.label(
                            egui::RichText::new("已达最大黑洞数 (8)")
                                .color(egui::Color32::RED)
                                .small(),
                        );
                    }

                    ui.add_space(4.0);
                    ui.separator();

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("━━━ 添加天体 ━━━")
                            .strong()
                            .color(egui::Color32::from_rgb(100, 255, 180)),
                    );
                    ui.label(
                        egui::RichText::new("天体被洛希极限撕裂后形成吸积盘")
                            .weak()
                            .small(),
                    );

                    ui.add(
                        egui::Slider::new(&mut body_mass, 0.05..=2.0)
                            .text("质量")
                            .step_by(0.05),
                    );

                    ui.label("位置:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut body_px).speed(0.2).prefix("x: "));
                        ui.add(egui::DragValue::new(&mut body_py).speed(0.2).prefix("y: "));
                        ui.add(egui::DragValue::new(&mut body_pz).speed(0.2).prefix("z: "));
                    });

                    ui.label("速度:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut body_vx).speed(0.05).prefix("vx: "));
                        ui.add(egui::DragValue::new(&mut body_vy).speed(0.05).prefix("vy: "));
                        ui.add(egui::DragValue::new(&mut body_vz).speed(0.05).prefix("vz: "));
                    });

                    ui.add_space(6.0);
                    if ui.button(egui::RichText::new("➕ 添加天体 (暂停)").size(14.0).strong())
                        .clicked()
                    {
                        add_body = true;
                    }

                    ui.add_space(4.0);
                    ui.separator();

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("━━━ 操作说明 ━━━")
                            .strong()
                            .color(egui::Color32::from_rgb(120, 180, 255)),
                    );
                    ui.label("🖱  左键拖拽: 旋转视角");
                    ui.label("🖱  滚轮: 缩放");
                    ui.label("⌨  WASD: 平移");
                    ui.label("⌨  QE: 上下平移");
                    ui.label("⌨  方向键: 旋转");
                    ui.label("⌨  SPACE: 暂停/继续");
                    ui.label("⌨  ESC: 退出");
                });

        egui::TopBottomPanel::bottom("状态栏").show(&self.egui_ctx, |ui| {
            ui.horizontal(|ui| {
                let status_color = if paused {
                    egui::Color32::from_rgb(255, 200, 80)
                } else {
                    egui::Color32::from_rgb(100, 255, 120)
                };
                ui.colored_label(status_color, "●");
                ui.label(if paused { "已暂停" } else { "运行中" });
                ui.separator();
                ui.label(&phase);
                ui.separator();
                ui.label(format!("时间: {:.1}s", time));
                ui.separator();
                ui.label(format!("黑洞: {}", bh_count));
                ui.separator();
                ui.label(format!("天体: {}", self.sim.bodies.len()));
                ui.separator();
                ui.label(format!("碎片: {}", self.sim.debris.len()));
                ui.separator();
                ui.label(format!("波: {}", wave_count));
            });
        });

        self.ui_show_waves = show_waves;
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
            self.ui_add_mass = 1.5;
        }
    }

    fn handle_keyboard(&mut self, key: KeyCode) {
        let pan_step = 0.3;
        let rot_step = 0.05;
        match key {
            KeyCode::KeyA => self.camera.pan(nalgebra::Vector3::new(-pan_step, 0.0, 0.0)),
            KeyCode::KeyD => self.camera.pan(nalgebra::Vector3::new(pan_step, 0.0, 0.0)),
            KeyCode::KeyW => self.camera.pan(nalgebra::Vector3::new(0.0, 0.0, -pan_step)),
            KeyCode::KeyS => self.camera.pan(nalgebra::Vector3::new(0.0, 0.0, pan_step)),
            KeyCode::KeyQ => self.camera.pan(nalgebra::Vector3::new(0.0, pan_step, 0.0)),
            KeyCode::KeyE => self.camera.pan(nalgebra::Vector3::new(0.0, -pan_step, 0.0)),
            KeyCode::ArrowLeft => self.camera.rotate_yaw(-rot_step),
            KeyCode::ArrowRight => self.camera.rotate_yaw(rot_step),
            KeyCode::ArrowUp => self.camera.rotate_pitch(-rot_step),
            KeyCode::ArrowDown => self.camera.rotate_pitch(rot_step),
            KeyCode::Space => {
                self.ui_paused = !self.ui_paused;
                self.sim.paused = self.ui_paused;
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attrs = Window::default_attributes()
            .with_title("Black Hole Collision Simulation")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        let window = event_loop
            .create_window(window_attrs)
            .expect("无法创建窗口");
        let scale_factor = window.scale_factor();
        let window: &'static Window = Box::leak(Box::new(window));

        let renderer = Renderer::new(window);

        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(scale_factor as f32),
            None,
            None,
        );

        let egui_renderer =
            EguiRenderer::new(&renderer.device, renderer.config.format, None, 1, true);

        // 设置 egui 字体（加载系统中文字体）
        setup_chinese_font(&self.egui_ctx);

        // 设置 egui 样式
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(20, 22, 30);
        visuals.extreme_bg_color = egui::Color32::from_rgb(12, 14, 20);
        self.egui_ctx.set_visuals(visuals);

        self.renderer = Some(renderer);
        self.egui_state = Some(egui_state);
        self.egui_renderer = Some(egui_renderer);
        self.window = Some(window);
        self.last_frame = Some(Instant::now());
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        // 先让 egui 处理事件
        let mut egui_consumed = false;
        if let Some(state) = self.egui_state.as_mut() {
            let resp = state.on_window_event(window, &event);
            if resp.consumed {
                egui_consumed = true;
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        if key == KeyCode::Escape {
                            event_loop.exit();
                            return;
                        }
                        if !egui_consumed {
                            self.handle_keyboard(key);
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if !egui_consumed {
                    self.mouse_pressed = state == ElementState::Pressed;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if !egui_consumed && self.mouse_pressed {
                    if let Some(last) = self.last_mouse_pos {
                        let dx = position.x - last.x;
                        let dy = position.y - last.y;
                        let sensitivity = 0.005;
                        self.camera.rotate_yaw(dx as f32 * sensitivity);
                        self.camera.rotate_pitch(dy as f32 * sensitivity);
                    }
                }
                self.last_mouse_pos = Some(position);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed {
                    let zoom_amount = match delta {
                        MouseScrollDelta::LineDelta(_, y) => -y * 0.8,
                        MouseScrollDelta::PixelDelta(pos) => -pos.y as f32 * 0.05,
                    };
                    self.camera.zoom(zoom_amount);
                }
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(physical_size.width, physical_size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = self
                    .last_frame
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .unwrap_or(0.016);
                self.last_frame = Some(now);

                self.sim.update(dt.min(0.05));

                // 相机 target 跟随所有天体的质心
                if !self.sim.black_holes.is_empty() {
                    let total_mass: f32 = self.sim.black_holes.iter().map(|bh| bh.mass).sum();
                    if total_mass > 0.0 {
                        let com: nalgebra::Vector3<f32> = self.sim.black_holes.iter()
                            .map(|bh| bh.pos * bh.mass)
                            .sum::<nalgebra::Vector3<f32>>() / total_mass;
                        self.camera.target = com;
                    }
                }

                let window_ref: &'static Window = match self.window.as_ref() {
                    Some(&w) => w,
                    None => return,
                };

                // 更新 egui（使用 begin_pass，不调用 run）
                self.update_egui(window_ref);

                let wave_objects = self.sim.get_wave_objects();
                let show_waves = self.sim.show_gravity_waves;
                let time = self.sim.time;

                let bh_data: Vec<(nalgebra::Vector3<f32>, f32)> = self.sim.black_holes
                    .iter()
                    .map(|bh| (bh.pos, bh.mass))
                    .collect();
                let body_data = self.sim.get_body_render_data();
                let debris_data = self.sim.get_debris_render_data();

                // 计算轨迹预测（暂停时显示，N 尽可能大）
                let trail_instances: Vec<renderer::TrailInstance> = if self.ui_show_trails && self.ui_paused {
                    let mut instances = Vec::new();
                    // 模拟 60 秒，每步 0.05s（1200 步）
                    let steps = 2400;
                    let dt_step = 0.03;
                    let (bh_trails, body_trails) = self.sim.predict_trajectories(steps, dt_step);

                    // 黑洞轨迹：橙色 (color_type=0)，方形 (shape_type=0)
                    for trail in &bh_trails {
                        let n = trail.len();
                        for (i, pos) in trail.iter().enumerate() {
                            let fade = i as f32 / n.max(1) as f32;
                            instances.push(renderer::TrailInstance {
                                pos: [pos.x, pos.y, pos.z],
                                color_type: 0.0,
                                shape_type: 0.0,
                                fade,
                                _pad: [0.0; 1],
                            });
                        }
                    }
                    // 天体轨迹：青色 (color_type=1)，三角形 (shape_type=1)
                    for trail in &body_trails {
                        let n = trail.len();
                        for (i, pos) in trail.iter().enumerate() {
                            let fade = i as f32 / n.max(1) as f32;
                            instances.push(renderer::TrailInstance {
                                pos: [pos.x, pos.y, pos.z],
                                color_type: 1.0,
                                shape_type: 1.0,
                                fade,
                                _pad: [0.0; 1],
                            });
                        }
                    }

                    // 如果正在配置黑洞参数，预览新黑洞轨迹（粉紫色 color_type=3），方形 (shape_type=0)
                    let preview_bh = physics::BlackHole {
                        mass: self.ui_add_mass,
                        pos: nalgebra::Vector3::new(self.ui_add_pos_x, self.ui_add_pos_y, self.ui_add_pos_z),
                        vel: nalgebra::Vector3::new(self.ui_add_vel_x, self.ui_add_vel_y, self.ui_add_vel_z),
                    };
                    let (preview_bh_trails, _) = self.sim.predict_trajectories_with_black_hole(&preview_bh, steps, dt_step);
                    if let Some(preview) = preview_bh_trails.last() {
                        let n = preview.len();
                        for (i, pos) in preview.iter().enumerate() {
                            let fade = i as f32 / n.max(1) as f32;
                            instances.push(renderer::TrailInstance {
                                pos: [pos.x, pos.y, pos.z],
                                color_type: 3.0,
                                shape_type: 0.0,
                                fade,
                                _pad: [0.0; 1],
                            });
                        }
                    }

                    // 如果正在配置天体参数，预览新天体轨迹（黄色 color_type=2），三角形 (shape_type=1)
                    let preview_body = physics::CelestialBody {
                        mass: self.ui_body_mass,
                        pos: nalgebra::Vector3::new(self.ui_body_pos_x, self.ui_body_pos_y, self.ui_body_pos_z),
                        vel: nalgebra::Vector3::new(self.ui_body_vel_x, self.ui_body_vel_y, self.ui_body_vel_z),
                        hardness: 1.0,
                    };
                    let (_, preview_trails) = self.sim.predict_trajectories_with_body(&preview_body, steps, dt_step);
                    if let Some(preview) = preview_trails.last() {
                        let n = preview.len();
                        for (i, pos) in preview.iter().enumerate() {
                            let fade = i as f32 / n.max(1) as f32;
                            instances.push(renderer::TrailInstance {
                                pos: [pos.x, pos.y, pos.z],
                                color_type: 2.0,
                                shape_type: 1.0,
                                fade,
                                _pad: [0.0; 1],
                            });
                        }
                    }

                    // 限制总数到 4000
                    instances.truncate(4000);
                    instances
                } else {
                    Vec::new()
                };

                // 渲染 3D 场景
                let (output, view) = {
                    let Some(renderer) = self.renderer.as_mut() else {
                        return;
                    };
                    let preview_black_hole = if self.ui_paused {
                        Some((
                            nalgebra::Vector3::new(self.ui_add_pos_x, self.ui_add_pos_y, self.ui_add_pos_z),
                            self.ui_add_mass,
                        ))
                    } else {
                        None
                    };
                    let preview_body = if self.ui_paused {
                        Some((
                            [self.ui_body_pos_x, self.ui_body_pos_y, self.ui_body_pos_z],
                            self.ui_body_mass,
                        ))
                    } else {
                        None
                    };
                    match renderer.render(&self.camera, &wave_objects, &bh_data, &body_data, &debris_data, show_waves, time, &trail_instances, preview_black_hole, preview_body) {
                        Ok(result) => result,
                        Err(wgpu::SurfaceError::Outdated) | Err(wgpu::SurfaceError::Lost) => {
                            let size = window_ref.inner_size();
                            renderer.resize(size.width, size.height);
                            return;
                        }
                        Err(e) => {
                            eprintln!("渲染错误: {:?}", e);
                            return;
                        }
                    }
                };

                // 渲染 egui（此时调用 end_pass 获取 shapes）
                if let (Some(egui_renderer), Some(renderer_ref)) =
                    (self.egui_renderer.as_mut(), self.renderer.as_ref())
                {
                    let screen_descriptor = egui_wgpu::ScreenDescriptor {
                        size_in_pixels: [renderer_ref.config.width, renderer_ref.config.height],
                        pixels_per_point: window_ref.scale_factor() as f32,
                    };

                    // end_pass 在此处调用（与 begin_pass 配对）
                    let full_output = self.egui_ctx.end_pass();
                    let paint_jobs = self
                        .egui_ctx
                        .tessellate(full_output.shapes, screen_descriptor.pixels_per_point);

                    let mut encoder = renderer_ref
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("egui 命令编码器"),
                        });

                    for (id, image_delta) in &full_output.textures_delta.set {
                        egui_renderer.update_texture(
                            &renderer_ref.device,
                            &renderer_ref.queue,
                            *id,
                            image_delta,
                        );
                    }

                    egui_renderer.update_buffers(
                        &renderer_ref.device,
                        &renderer_ref.queue,
                        &mut encoder,
                        &paint_jobs,
                        &screen_descriptor,
                    );

                    {
                        let egui_render_pass =
                            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("egui 渲染通道"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });

                        let mut static_pass = egui_render_pass.forget_lifetime();
                        egui_renderer.render(&mut static_pass, &paint_jobs, &screen_descriptor);
                    }

                    for id in &full_output.textures_delta.free {
                        egui_renderer.free_texture(id);
                    }

                    renderer_ref.queue.submit(std::iter::once(encoder.finish()));
                }

                output.present();
                window_ref.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("无法创建事件循环");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("事件循环错误");
}

/// 设置中文字体（从 Windows 系统目录加载微软雅黑）
fn setup_chinese_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 尝试加载系统中文字体
    let font_paths = [
        r"C:\Windows\Fonts\msyh.ttc",      // 微软雅黑
        r"C:\Windows\Fonts\msyhbd.ttc",    // 微软雅黑粗体
        r"C:\Windows\Fonts\simhei.ttf",    // 黑体
        r"C:\Windows\Fonts\simsun.ttc",    // 宋体
    ];

    let mut loaded = false;
    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            let name = format!("chinese_{}", loaded);
            fonts
                .font_data
                .insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(data)));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, name.clone());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push(name);
            loaded = true;
        }
    }

    if !loaded {
        eprintln!("警告：未找到中文字体，中文可能显示为方框");
    }

    ctx.set_fonts(fonts);
}
