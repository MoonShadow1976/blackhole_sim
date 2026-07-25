// wgpu 渲染器模块 - 3D 渲染管线与 WGSL 着色器

mod pipeline;
mod shaders;
mod types;

pub use types::*;

use nalgebra::{Matrix4, Vector3};

use crate::camera::OrbitCamera;

pub struct RenderParams<'a> {
    pub camera: &'a OrbitCamera,
    pub waves: &'a [(Vector3<f32>, f32, u32, f32, f32)],
    pub black_holes: &'a [(Vector3<f32>, f32)],
    pub bodies: &'a [([f32; 3], f32)],
    pub debris: &'a [([f32; 3], f32, f32)],
    pub show_waves: bool,
    pub show_background: bool,
    pub show_bodies: bool,
    pub time: f32,
    pub trails: &'a [TrailInstance],
    pub preview_black_hole: Option<(Vector3<f32>, f32)>,
    pub preview_body: Option<([f32; 3], f32)>,
    /// Tendex 线渲染数据：(位置[xyz], color_sign)
    /// color_sign: +1.0 = 拉伸（红），-1.0 = 压缩（蓝）
    /// 每 2 个连续顶点构成一条线段（LineList 拓扑）
    pub tendex_data: &'a [([f32; 3], f32)],
}

/// wgpu 渲染器
pub struct Renderer {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) depth_texture: wgpu::TextureView,
    pub(crate) render_pipeline: wgpu::RenderPipeline,
    pub(crate) sphere_vertices: wgpu::Buffer,
    pub(crate) sphere_indices: wgpu::Buffer,
    pub(crate) sphere_index_count: u32,
    pub(crate) ring_vertices: wgpu::Buffer,
    pub(crate) ring_indices: wgpu::Buffer,
    pub(crate) ring_index_count: u32,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) model_buffer: wgpu::Buffer,
    pub(crate) bhs_buffer: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) debris_pipeline: wgpu::RenderPipeline,
    pub(crate) debris_instance_buffer: wgpu::Buffer,
    pub(crate) trail_instance_buffer: wgpu::Buffer,
    pub(crate) trail_pipeline: wgpu::RenderPipeline,
    pub(crate) quad_vertices: wgpu::Buffer,
    pub(crate) quad_indices: wgpu::Buffer,
    pub(crate) quad_index_count: u32,
    pub(crate) tendex_pipeline: wgpu::RenderPipeline,
    pub(crate) tendex_vertex_buffer: wgpu::Buffer,
    /// 当前 tendex 顶点缓冲中有效顶点数（每帧更新）
    pub(crate) tendex_vertex_count: u32,
    pub(crate) bg_pipeline: wgpu::RenderPipeline,
}

impl Renderer {
    /// 更新 Tendex 顶点缓冲（每帧调用）
    /// data: (位置[xyz], color_sign)，每 2 个顶点构成一条线段（LineList）
    /// 顶点数受 MAX_TENDEX_VERTICES (100000) 限制
    pub fn update_tendex_buffer(&mut self, data: &[([f32; 3], f32)]) {
        const MAX_TENDEX_VERTICES: usize = 100000;
        let take = data.len().min(MAX_TENDEX_VERTICES);
        let vertices: Vec<TendexVertex> = data[..take]
            .iter()
            .map(|(pos, sign)| TendexVertex {
                position: *pos,
                color_sign: *sign,
            })
            .collect();
        self.tendex_vertex_count = vertices.len() as u32;
        if !vertices.is_empty() {
            self.queue.write_buffer(
                &self.tendex_vertex_buffer,
                0,
                bytemuck::cast_slice(&vertices),
            );
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = pipeline::create_depth_texture(&self.device, &self.config);
    }

    /// 渲染 3D 场景
    /// waves: 引力波对象 (位置, 缩放, 类型, alpha, 黑洞质量)
    /// black_holes: 黑洞列表 (位置, 质量)
    /// bodies: 天体列表 (位置, 质量)
    /// debris: 碎片粒子列表 (位置, 速度大小, 寿命)
    pub fn render(
        &mut self,
        params: RenderParams,
    ) -> Result<(wgpu::SurfaceTexture, wgpu::TextureView), wgpu::SurfaceError> {
        // 更新 Tendex 顶点缓冲（每帧）
        if params.show_waves {
            self.update_tendex_buffer(params.tendex_data);
        } else {
            self.tendex_vertex_count = 0;
        }

        let aspect = self.config.width as f32 / self.config.height as f32;
        let proj = Matrix4::new_perspective(aspect, 60.0_f32.to_radians(), 0.1, 10000.0);
        let view = params.camera.view_matrix();
        let view_proj = proj * view;
        let view_inv = view.try_inverse().unwrap_or(Matrix4::identity());
        let proj_inv = proj.try_inverse().unwrap_or(Matrix4::identity());
        let cam_pos = params.camera.position();
        let camera_uniform = CameraUniform {
            view_proj: view_proj.into(),
            view_inv: view_inv.into(),
            proj_inv: proj_inv.into(),
            camera_pos: [cam_pos.x, cam_pos.y, cam_pos.z],
            _pad: 0.0,
        };
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[camera_uniform]),
        );

        // 更新黑洞数组
        let mut bhs_uniform = BlackHolesUniform {
            count: params.black_holes.len() as u32,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            holes: [BlackHoleData {
                pos: [0.0, 0.0, 0.0],
                mass: 0.0,
            }; 8],
        };
        for (i, (pos, mass)) in params.black_holes.iter().take(8).enumerate() {
            bhs_uniform.holes[i] = BlackHoleData {
                pos: [pos.x, pos.y, pos.z],
                mass: *mass,
            };
        }
        self.queue
            .write_buffer(&self.bhs_buffer, 0, bytemuck::cast_slice(&[bhs_uniform]));

        // 准备碎片实例数据（最多 600 个粒子，每帧只写入实际粒子数）
        let debris_instances: Vec<DebrisInstance> = params
            .debris
            .iter()
            .take(600)
            .map(|(pos, speed, life)| DebrisInstance {
                pos: *pos,
                speed: *speed,
                life: *life,
                _pad: [0.0; 3],
            })
            .collect();
        let debris_count = debris_instances.len() as u32;
        if debris_count > 0 {
            self.queue.write_buffer(
                &self.debris_instance_buffer,
                0,
                bytemuck::cast_slice(&debris_instances),
            );
        }

        // 写入轨迹实例数据（在 render pass 之前）
        let trail_count = params.trails.len().min(4000) as u32;
        if trail_count > 0 {
            self.queue.write_buffer(
                &self.trail_instance_buffer,
                0,
                bytemuck::cast_slice(&params.trails[..trail_count as usize]),
            );
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("3D 渲染命令编码器"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D 渲染通道"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 深空黑背景
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.005,
                            g: 0.008,
                            b: 0.02,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 在 render pass 之前准备所有 model uniform
            let mut draw_calls: Vec<(u32, u32)> = Vec::new();
            let mut uniforms: Vec<ModelUniform> = Vec::new();

            // 背景天球（单独处理，最先渲染，使用专用管线）
            let bg_uniform = if params.show_background {
                let cam_pos = params.camera.position();
                let m = Matrix4::new_scaling(500.0);
                let model_matrix = Matrix4::new_translation(&cam_pos) * m;
                Some(ModelUniform::new(
                    model_matrix.into(),
                    3,
                    params.time,
                    1.0,
                    0.0,
                ))
            } else {
                None
            };

            // 1. 天体
            if params.show_bodies {
                for (pos, mass) in params.bodies.iter() {
                    let scale = mass.powf(0.4) * 0.8;
                    let idx = uniforms.len() as u32;
                    let m = Matrix4::new_scaling(scale);
                    let model_matrix =
                        Matrix4::new_translation(&Vector3::new(pos[0], pos[1], pos[2])) * m;
                    uniforms.push(ModelUniform::new(
                        model_matrix.into(),
                        4,
                        params.time,
                        1.0,
                        0.0,
                    ));
                    draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 4));
                }
            }

            // 3. 引力波
            if params.show_waves {
                for (pos, scale, obj_type, alpha, bh_mass) in params.waves.iter() {
                    let idx = uniforms.len() as u32;
                    let m = Matrix4::new_scaling(*scale);
                    let model_matrix = Matrix4::new_translation(pos) * m;
                    uniforms.push(ModelUniform::new(
                        model_matrix.into(),
                        *obj_type,
                        params.time,
                        *alpha,
                        *bh_mass,
                    ));
                    draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, *obj_type));
                }
            }

            // 4. 黑洞（事件视界，最近，最后渲染，遮挡后面的物体）
            if params.show_bodies {
                for (pos, mass) in params.black_holes.iter() {
                    let idx = uniforms.len() as u32;
                    let event_horizon_r = 2.0 * mass;
                    let m = Matrix4::new_scaling(event_horizon_r);
                    let model_matrix = Matrix4::new_translation(pos) * m;
                    uniforms.push(ModelUniform::new(
                        model_matrix.into(),
                        0,
                        params.time,
                        1.0,
                        *mass,
                    ));
                    draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 0));
                }
            }

            // 4.5 预览天体（闪烁）
            if let Some((pos, mass)) = params.preview_body {
                let blink = (params.time * 4.0).sin() * 0.5 + 0.5;
                let alpha = 0.3 + blink * 0.6;
                let scale = mass.powf(0.4) * 0.8;
                let idx = uniforms.len() as u32;
                let m = Matrix4::new_scaling(scale);
                let model_matrix =
                    Matrix4::new_translation(&Vector3::new(pos[0], pos[1], pos[2])) * m;
                uniforms.push(ModelUniform::new(
                    model_matrix.into(),
                    4,
                    params.time,
                    alpha,
                    0.0,
                ));
                draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 4));
            }

            // 4.6 预览黑洞（闪烁）
            if let Some((pos, mass)) = params.preview_black_hole {
                let blink = (params.time * 4.0 + 1.0).sin() * 0.5 + 0.5;
                let alpha = 0.4 + blink * 0.6;
                let idx = uniforms.len() as u32;
                let event_horizon_r = 2.0 * mass;
                let m = Matrix4::new_scaling(event_horizon_r);
                let model_matrix = Matrix4::new_translation(&pos) * m;
                uniforms.push(ModelUniform::new(
                    model_matrix.into(),
                    0,
                    params.time,
                    alpha,
                    mass,
                ));
                draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 0));
            }

            // 背景uniform加入数组（在天体之后，避免覆盖天体uniform）
            let bg_offset: Option<u32> = if let Some(bg_u) = &bg_uniform {
                let idx = uniforms.len() as u32;
                if (idx as usize) < MAX_OBJECTS {
                    uniforms.push(*bg_u);
                    Some(idx * MODEL_UNIFORM_SIZE as u32)
                } else {
                    None
                }
            } else {
                None
            };

            // 在 render pass 之前一次性写入所有 uniform
            // 防止 uniforms 数量超过 buffer 容量（MAX_OBJECTS）
            let max_write = uniforms.len().min(MAX_OBJECTS);
            for (i, u) in uniforms.iter().take(max_write).enumerate() {
                self.queue.write_buffer(
                    &self.model_buffer,
                    (i as u64) * MODEL_UNIFORM_SIZE,
                    bytemuck::cast_slice(std::slice::from_ref(u)),
                );
            }
            // 过滤掉超出容量的 draw_calls
            draw_calls.retain(|&(offset, _)| {
                ((offset / MODEL_UNIFORM_SIZE as u32) as usize) < max_write
            });

            // 0. 最先画背景（使用专用管线，不写深度，永远通过深度测试）
            if let Some(offset) = bg_offset {
                render_pass.set_pipeline(&self.bg_pipeline);
                render_pass.set_bind_group(0, &self.bind_group, &[offset]);
                render_pass.set_vertex_buffer(0, self.sphere_vertices.slice(..));
                render_pass.set_index_buffer(
                    self.sphere_indices.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(0..self.sphere_index_count, 0, 0..1);
            }

            // 1. Tendex 线（在背景之上、天体之下，半透明叠加不写深度）
            if params.show_waves && self.tendex_vertex_count > 0 {
                render_pass.set_pipeline(&self.tendex_pipeline);
                render_pass.set_bind_group(0, &self.bind_group, &[0]);
                render_pass.set_vertex_buffer(0, self.tendex_vertex_buffer.slice(..));
                // LineList 拓扑：每对相邻顶点形成一条线段，无需索引缓冲
                render_pass.draw(0..self.tendex_vertex_count, 0..1);
            }

            // 2. 天体和黑洞（写深度，遮挡网格）
            render_pass.set_pipeline(&self.render_pipeline);

            for &(offset, obj_type) in draw_calls.iter() {
                render_pass.set_bind_group(0, &self.bind_group, &[offset]);

                match obj_type {
                    0 | 2 | 4 => {
                        render_pass.set_vertex_buffer(0, self.sphere_vertices.slice(..));
                        render_pass.set_index_buffer(
                            self.sphere_indices.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(0..self.sphere_index_count, 0, 0..1);
                    }
                    1 => {
                        render_pass.set_vertex_buffer(0, self.ring_vertices.slice(..));
                        render_pass.set_index_buffer(
                            self.ring_indices.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        render_pass.draw_indexed(0..self.ring_index_count, 0, 0..1);
                    }
                    _ => {}
                }
            }

            // 3. 碎片粒子（additive blending，不写深度）
            if debris_count > 0 {
                render_pass.set_pipeline(&self.debris_pipeline);
                render_pass.set_bind_group(0, &self.bind_group, &[0]);
                render_pass.set_vertex_buffer(0, self.quad_vertices.slice(..));
                render_pass.set_vertex_buffer(1, self.debris_instance_buffer.slice(..));
                render_pass
                    .set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..self.quad_index_count, 0, 0..debris_count);
            }

            // 4. 轨迹散点（additive blending，不写深度）
            if trail_count > 0 {
                render_pass.set_pipeline(&self.trail_pipeline);
                render_pass.set_bind_group(0, &self.bind_group, &[0]);
                render_pass.set_vertex_buffer(0, self.quad_vertices.slice(..));
                render_pass.set_vertex_buffer(1, self.trail_instance_buffer.slice(..));
                render_pass
                    .set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..self.quad_index_count, 0, 0..trail_count);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        Ok((output, view))
    }
}
