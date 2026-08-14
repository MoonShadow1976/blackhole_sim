// 黑洞碰撞模拟 - 主入口
// 使用 winit 创建窗口，wgpu 进行 3D 渲染，egui 提供控制面板

mod camera;
mod geometry;
mod physics;
mod renderer;
mod ui;

#[cfg(not(target_family = "wasm"))]
use std::time::Instant;

#[cfg(target_family = "wasm")]
fn perf_now_ms() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy)]
struct Instant(f64);

#[cfg(target_family = "wasm")]
impl Instant {
    fn now() -> Self {
        Self(perf_now_ms())
    }
    fn duration_since(&self, earlier: Self) -> std::time::Duration {
        let ms = (self.0 - earlier.0).max(0.0);
        std::time::Duration::from_secs_f64(ms / 1000.0)
    }
}

use camera::OrbitCamera;
use physics::Simulation;
use renderer::Renderer;

use egui_wgpu::Renderer as EguiRenderer;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(target_family = "wasm")]
type AppEvent = WasmAppEvent;

#[cfg(not(target_family = "wasm"))]
type AppEvent = ();

#[cfg(target_family = "wasm")]
enum WasmAppEvent {
    RendererReady {
        renderer: Renderer,
        egui_state: egui_winit::State,
        egui_renderer: EguiRenderer,
    },
    ChineseFontLoaded(Vec<u8>),
    TouchRotate {
        dx: f32,
        dy: f32,
    },
    TouchZoom {
        delta: f32,
    },
}

/// 待添加对象的参数（质量/位置/速度），供 UI 编辑、实时安全校验与轨迹预览共用
#[derive(Clone, Copy)]
pub(crate) struct SpawnParams {
    pub mass: f32,
    pub pos: nalgebra::Vector3<f32>,
    pub vel: nalgebra::Vector3<f32>,
}

impl SpawnParams {
    pub(crate) fn default_black_hole() -> Self {
        Self {
            mass: 2.0,
            pos: nalgebra::Vector3::new(7.5, 0.0, 0.0),
            vel: nalgebra::Vector3::new(0.0, 0.0, -0.3),
        }
    }

    pub(crate) fn default_body() -> Self {
        Self {
            mass: 0.3,
            pos: nalgebra::Vector3::new(6.0, 0.5, 0.0),
            vel: nalgebra::Vector3::new(0.0, 0.0, 0.6),
        }
    }

    pub(crate) fn to_black_hole(&self) -> physics::BlackHole {
        physics::BlackHole {
            mass: self.mass,
            pos: self.pos,
            vel: self.vel,
        }
    }

    pub(crate) fn to_body(&self) -> physics::CelestialBody {
        physics::CelestialBody {
            mass: self.mass,
            pos: self.pos,
            vel: self.vel,
            hardness: 1.0, // 默认岩石材质
        }
    }
}

/// 应用状态
struct App {
    pub(crate) renderer: Option<Renderer>,
    pub(crate) camera: OrbitCamera,
    pub(crate) sim: Simulation,
    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_state: Option<egui_winit::State>,
    pub(crate) egui_renderer: Option<EguiRenderer>,
    pub(crate) last_frame: Option<Instant>,
    pub(crate) mouse_pressed: bool,
    pub(crate) last_mouse_pos: Option<PhysicalPosition<f64>>,
    pub(crate) active_touches: Vec<(u64, PhysicalPosition<f64>)>,
    pub(crate) last_pinch_dist: Option<f64>,
    pub(crate) window: Option<&'static Window>,
    /// WASM 端事件代理（桌面端为 None）
    pub(crate) event_proxy: Option<EventLoopProxy<AppEvent>>,
    // UI 显示状态
    pub(crate) ui_show_background: bool,
    pub(crate) ui_show_bodies: bool,
    pub(crate) ui_show_trails: bool,
    pub(crate) ui_show_panel: bool,
    pub(crate) ui_lang: UiLang,
    // 添加黑洞 / 天体的参数
    pub(crate) spawn_bh: SpawnParams,
    pub(crate) spawn_body: SpawnParams,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiLang {
    Zh,
    En,
}

impl App {
    /// 创建应用。WASM 端传入事件代理；桌面端传 None。
    fn new(event_proxy: Option<EventLoopProxy<AppEvent>>) -> Self {
        let sim = Simulation::new();
        Self {
            renderer: None,
            camera: OrbitCamera::new(),
            ui_show_background: true,
            ui_show_bodies: true,
            ui_show_trails: true,
            ui_show_panel: true,
            ui_lang: UiLang::Zh,
            sim,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            egui_renderer: None,
            last_frame: None,
            mouse_pressed: false,
            last_mouse_pos: None,
            active_touches: Vec::new(),
            last_pinch_dist: None,
            window: None,
            event_proxy,
            spawn_bh: SpawnParams::default_black_hole(),
            spawn_body: SpawnParams::default_body(),
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
                self.sim.paused = !self.sim.paused;
            }
            _ => {}
        }
    }

    /// 构建轨迹预览实例（暂停时显示）：
    /// - 黑洞轨迹：橙色 (color_type=0)，方形 (shape_type=0)
    /// - 天体轨迹：青色 (color_type=1)，三角形 (shape_type=1)
    /// - 待添加黑洞预览：粉紫色 (color_type=3)，方形
    /// - 待添加天体预览：黄色 (color_type=2)，三角形
    fn build_trail_instances(&self) -> Vec<renderer::TrailInstance> {
        if !self.ui_show_trails || !self.sim.paused {
            return Vec::new();
        }
        let mut instances = Vec::new();
        // 模拟 72 秒，每步 0.03s（2400 步）
        let steps = 2400;
        let dt_step = 0.03;
        let (bh_trails, body_trails) = self.sim.predict_trajectories(steps, dt_step);

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

        // 待添加黑洞的轨迹预览（在克隆状态末尾，取最后一条）
        let preview_bh = self.spawn_bh.to_black_hole();
        let (preview_bh_trails, _) =
            self.sim
                .predict_trajectories_with_black_hole(&preview_bh, steps, dt_step);
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

        // 待添加天体的轨迹预览
        let preview_body = self.spawn_body.to_body();
        let (_, preview_trails) =
            self.sim
                .predict_trajectories_with_body(&preview_body, steps, dt_step);
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
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attrs = Window::default_attributes()
            .with_title("Black Hole Collision Simulation")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        let window = event_loop
            .create_window(window_attrs)
            .expect("无法创建窗口");
        let scale_factor = window.scale_factor();

        #[cfg(target_family = "wasm")]
        {
            use winit::platform::web::WindowExtWebSys;
            let canvas = window.canvas().expect("winit window has no canvas");
            let style = canvas.style();
            style.set_property("display", "block").unwrap();
            style.set_property("width", "100vw").unwrap();
            style.set_property("height", "100vh").unwrap();
            style.set_property("position", "fixed").unwrap();
            style.set_property("top", "0").unwrap();
            style.set_property("left", "0").unwrap();
            style.set_property("margin", "0").unwrap();
            style.set_property("padding", "0").unwrap();
            let document = web_sys::window().unwrap().document().unwrap();
            if let Some(container) = document.get_element_by_id("canvas-container") {
                container.append_child(&canvas).unwrap();
            } else {
                document.body().unwrap().append_child(&canvas).unwrap();
            }
        }

        let window: &'static Window = Box::leak(Box::new(window));

        // 设置 egui 样式
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(20, 22, 30);
        visuals.extreme_bg_color = egui::Color32::from_rgb(12, 14, 20);
        self.egui_ctx.set_visuals(visuals);

        // 设置中文字体
        #[cfg(not(target_family = "wasm"))]
        setup_chinese_font(&self.egui_ctx);

        #[cfg(target_family = "wasm")]
        {
            let proxy = self.event_proxy.clone().unwrap();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(font_data) = load_chinese_font_async().await {
                    let _ = proxy.send_event(WasmAppEvent::ChineseFontLoaded(font_data));
                }
            });
        }

        self.window = Some(window);
        self.last_frame = Some(Instant::now());

        // 同步/异步初始化 renderer
        #[cfg(not(target_family = "wasm"))]
        {
            let renderer = pollster::block_on(Renderer::new(window));
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
            self.renderer = Some(renderer);
            self.egui_state = Some(egui_state);
            self.egui_renderer = Some(egui_renderer);
        }

        #[cfg(target_family = "wasm")]
        {
            use wasm_bindgen_futures::spawn_local;
            let proxy = self.event_proxy.clone();
            let egui_ctx = self.egui_ctx.clone();
            let scale_factor = scale_factor as f32;
            spawn_local(async move {
                let renderer = Renderer::new(window).await;
                let egui_state = egui_winit::State::new(
                    egui_ctx.clone(),
                    egui::ViewportId::ROOT,
                    window,
                    Some(scale_factor),
                    None,
                    None,
                );
                let egui_renderer =
                    EguiRenderer::new(&renderer.device, renderer.config.format, None, 1, true);
                let _ = proxy.send_event(WasmAppEvent::RendererReady {
                    renderer,
                    egui_state,
                    egui_renderer,
                });
            });
        }
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
            WindowEvent::Touch(touch) => {
                if egui_consumed {
                    return;
                }
                match touch.phase {
                    TouchPhase::Started => {
                        self.active_touches.push((touch.id, touch.location));
                        if self.active_touches.len() == 2 {
                            let p0 = self.active_touches[0].1;
                            let p1 = self.active_touches[1].1;
                            self.last_pinch_dist =
                                Some(((p0.x - p1.x).powi(2) + (p0.y - p1.y).powi(2)).sqrt());
                        }
                    }
                    TouchPhase::Moved => {
                        if let Some(entry) = self
                            .active_touches
                            .iter_mut()
                            .find(|(id, _)| *id == touch.id)
                        {
                            let old_pos = entry.1;
                            entry.1 = touch.location;

                            if self.active_touches.len() == 1 {
                                let dx = touch.location.x - old_pos.x;
                                let dy = touch.location.y - old_pos.y;
                                let sensitivity = 0.005;
                                self.camera.rotate_yaw(dx as f32 * sensitivity);
                                self.camera.rotate_pitch(dy as f32 * sensitivity);
                            }
                        }

                        if self.active_touches.len() >= 2 {
                            let p0 = self.active_touches[0].1;
                            let p1 = self.active_touches[1].1;
                            let current_dist =
                                ((p0.x - p1.x).powi(2) + (p0.y - p1.y).powi(2)).sqrt();
                            if let Some(last_dist) = self.last_pinch_dist {
                                let zoom_amount = (last_dist - current_dist) * 0.03;
                                self.camera.zoom(zoom_amount as f32);
                            }
                            self.last_pinch_dist = Some(current_dist);
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.active_touches.retain(|(id, _)| *id != touch.id);
                        if self.active_touches.len() < 2 {
                            self.last_pinch_dist = None;
                        }
                    }
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

                // 相机 target 跟随所有黑洞的质心
                if let Some(com) = self.sim.center_of_mass() {
                    self.camera.target = com;
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

                let bh_data: Vec<(nalgebra::Vector3<f32>, f32)> = self
                    .sim
                    .black_holes
                    .iter()
                    .map(|bh| (bh.pos, bh.mass))
                    .collect();
                let body_data = self.sim.get_body_render_data();
                let debris_data = self.sim.get_debris_render_data();

                // 计算轨迹预测（暂停时显示，含新对象预览轨迹）
                let trail_instances = self.build_trail_instances();

                // 渲染 3D 场景
                let (output, view) = {
                    let Some(renderer) = self.renderer.as_mut() else {
                        return;
                    };
                    let preview_black_hole = if self.sim.paused && self.ui_show_trails {
                        Some((self.spawn_bh.pos, self.spawn_bh.mass))
                    } else {
                        None
                    };
                    let preview_body = if self.sim.paused && self.ui_show_trails {
                        Some((
                            [self.spawn_body.pos.x, self.spawn_body.pos.y, self.spawn_body.pos.z],
                            self.spawn_body.mass,
                        ))
                    } else {
                        None
                    };
                    let tendex_data = self
                        .sim
                        .get_tendex_render_data(self.sim.tendex_three_planes);
                    match renderer.render(renderer::RenderParams {
                        camera: &self.camera,
                        waves: &wave_objects,
                        black_holes: &bh_data,
                        bodies: &body_data,
                        debris: &debris_data,
                        show_waves,
                        show_background: self.ui_show_background,
                        show_bodies: self.ui_show_bodies,
                        time,
                        trails: &trail_instances,
                        preview_black_hole,
                        preview_body,
                        tendex_data: &tendex_data,
                    }) {
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

                // 渲染 egui 覆盖层并提交（end_pass 与 begin_pass 配对）
                self.render_egui_overlay(window_ref, output, view);
            }
            _ => {}
        }
    }

    /// 渲染 egui 覆盖层：结束 egui 帧、绘制到主视图并提交队列
    fn render_egui_overlay(
        &mut self,
        window: &'static Window,
        output: wgpu::SurfaceTexture,
        view: wgpu::TextureView,
    ) {
        if let (Some(egui_renderer), Some(renderer_ref)) =
            (self.egui_renderer.as_mut(), self.renderer.as_ref())
        {
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [renderer_ref.config.width, renderer_ref.config.height],
                pixels_per_point: window.scale_factor() as f32,
            };

            // end_pass 在此处调用（与 begin_pass 配对）
            let full_output = self.egui_ctx.end_pass();
            let paint_jobs = self
                .egui_ctx
                .tessellate(full_output.shapes, screen_descriptor.pixels_per_point);

            let mut encoder = renderer_ref.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("egui 命令编码器"),
                },
            );

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
        window.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    #[cfg(target_family = "wasm")]
    fn user_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        event: WasmAppEvent,
    ) {
        match event {
            WasmAppEvent::RendererReady {
                renderer,
                egui_state,
                egui_renderer,
            } => {
                self.renderer = Some(renderer);
                self.egui_state = Some(egui_state);
                self.egui_renderer = Some(egui_renderer);
            }
            WasmAppEvent::ChineseFontLoaded(font_data) => {
                setup_chinese_font_from_data(&self.egui_ctx, &font_data);
            }
            WasmAppEvent::TouchRotate { dx, dy } => {
                let sensitivity = 0.005;
                self.camera.rotate_yaw(dx * sensitivity);
                self.camera.rotate_pitch(dy * sensitivity);
            }
            WasmAppEvent::TouchZoom { delta } => {
                self.camera.zoom(delta);
            }
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("无法创建事件循环");
    let mut app = App::new(None);
    event_loop.run_app(&mut app).expect("事件循环错误");
}

#[cfg(target_family = "wasm")]
fn main() {
    // WASM 入口在 #[wasm_bindgen(start)] 中
}

#[cfg(target_family = "wasm")]
static EVENT_PROXY: std::sync::OnceLock<EventLoopProxy<AppEvent>> = std::sync::OnceLock::new();

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn touch_rotate(dx: f32, dy: f32) {
    if let Some(proxy) = EVENT_PROXY.get() {
        let _ = proxy.send_event(WasmAppEvent::TouchRotate { dx, dy });
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn touch_zoom(delta: f32) {
    if let Some(proxy) = EVENT_PROXY.get() {
        let _ = proxy.send_event(WasmAppEvent::TouchZoom { delta });
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        if msg.contains("Using exceptions for control flow") {
            return;
        }
        web_sys::console::error_1(&format!("Panic: {}", msg).into());
    }));

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("无法创建事件循环");
    let proxy = event_loop.create_proxy();

    let _ = EVENT_PROXY.set(proxy.clone());

    let mut app = App::new(Some(proxy));

    wasm_bindgen_futures::spawn_local(async move {
        let _ = event_loop.run_app(&mut app);
    });

    Ok(())
}

/// 设置中文字体（从 Windows 系统目录加载微软雅黑）
#[cfg(not(target_family = "wasm"))]
fn setup_chinese_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 尝试加载系统中文字体
    let font_paths = [
        r"C:\Windows\Fonts\msyh.ttc",   // 微软雅黑
        r"C:\Windows\Fonts\msyhbd.ttc", // 微软雅黑粗体
        r"C:\Windows\Fonts\simhei.ttf", // 黑体
        r"C:\Windows\Fonts\simsun.ttc", // 宋体
    ];

    let mut loaded = false;
    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            let name = format!("chinese_{}", loaded);
            fonts.font_data.insert(
                name.clone(),
                std::sync::Arc::new(egui::FontData::from_owned(data)),
            );
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

/// 从字体数据设置中文字体（Wasm 端用）
#[cfg(target_family = "wasm")]
fn setup_chinese_font_from_data(ctx: &egui::Context, font_data: &[u8]) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "chinese_font".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(font_data.to_vec())),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "chinese_font".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("chinese_font".to_owned());

    ctx.set_fonts(fonts);
}

/// 异步加载中文字体（Wasm 端从 CDN 加载）
#[cfg(target_family = "wasm")]
async fn load_chinese_font_async() -> Option<Vec<u8>> {
    let font_urls = [
        "https://cdn.jsdelivr.net/npm/@electron-fonts/noto-sans-sc@1.2.0/fonts/NotoSansSC-Regular.ttf",
        "https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@main/Sans/OTF/SimplifiedChinese/NotoSansSC-Regular.otf",
    ];

    for url in &font_urls {
        if let Ok(data) = fetch_bytes(url).await {
            return Some(data);
        }
    }

    None
}

/// 通过 fetch API 加载二进制数据
#[cfg(target_family = "wasm")]
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("request error: {:?}", e))?;

    let window = web_sys::window().ok_or("no window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("fetch error: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|e| format!("response cast error: {:?}", e))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let buf = JsFuture::from(
        resp.array_buffer()
            .map_err(|e| format!("array_buffer error: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("array_buffer future error: {:?}", e))?;

    let array = js_sys::Uint8Array::new(&buf);
    Ok(array.to_vec())
}
