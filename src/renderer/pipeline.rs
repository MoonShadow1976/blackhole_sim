// 渲染管线与缓冲创建逻辑：Renderer::new() 及初始化辅助函数

use super::shaders::*;
use super::types::*;
use super::Renderer;

use nalgebra::Matrix4;
use wgpu::util::DeviceExt;

use crate::geometry::{create_ring, create_sphere, Vertex};

impl Renderer {
    pub fn new(window: &'static winit::window::Window) -> Self {
        let instance_desc = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        };
        let instance = wgpu::Instance::new(&instance_desc);

        let surface = instance.create_surface(window).expect("无法创建 surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("无法找到合适的 GPU 适配器");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("GPU 设备"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .expect("无法请求设备");

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let depth_texture = create_depth_texture(&device, &config);

        let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("顶点着色器"),
            source: wgpu::ShaderSource::Wgsl(VERTEX_SHADER.into()),
        });
        let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("片段着色器"),
            source: wgpu::ShaderSource::Wgsl(FRAGMENT_SHADER.into()),
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("相机缓冲"),
            contents: bytemuck::cast_slice(&[CameraUniform {
                view_proj: Matrix4::identity().into(),
                view_inv: Matrix4::identity().into(),
                proj_inv: Matrix4::identity().into(),
                camera_pos: [0.0, 0.0, 0.0],
                _pad: 0.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let model_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("模型缓冲池"),
            size: MODEL_UNIFORM_SIZE * MAX_OBJECTS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bhs_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("黑洞数组缓冲"),
            contents: bytemuck::cast_slice(&[BlackHolesUniform {
                count: 0,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
                holes: [BlackHoleData {
                    pos: [0.0, 0.0, 0.0],
                    mass: 0.0,
                }; 8],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group 布局"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: std::num::NonZeroU64::new(MODEL_UNIFORM_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &model_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(MODEL_UNIFORM_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bhs_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("渲染管线布局"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("渲染管线"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // 创建几何体（高分辨率球体避免接缝）
        let (sphere_v, sphere_i) = create_sphere(1.0, 48, 24);
        let sphere_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("球体顶点"),
            contents: bytemuck::cast_slice(&sphere_v),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let sphere_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("球体索引"),
            contents: bytemuck::cast_slice(&sphere_i),
            usage: wgpu::BufferUsages::INDEX,
        });
        let sphere_index_count = sphere_i.len() as u32;

        // 背景专用管线：不写深度，永远通过深度测试
        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("背景渲染管线"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let (ring_v, ring_i) = create_ring(1.0, 2.0, 64);
        let ring_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("圆环顶点"),
            contents: bytemuck::cast_slice(&ring_v),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ring_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("圆环索引"),
            contents: bytemuck::cast_slice(&ring_i),
            usage: wgpu::BufferUsages::INDEX,
        });
        let ring_index_count = ring_i.len() as u32;

        // 碎片粒子渲染管线（instanced billboard，additive blending）
        let debris_vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("碎片顶点着色器"),
            source: wgpu::ShaderSource::Wgsl(DEBRIS_VERTEX_SHADER.into()),
        });
        let debris_fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("碎片片段着色器"),
            source: wgpu::ShaderSource::Wgsl(DEBRIS_FRAGMENT_SHADER.into()),
        });

        // 复用现有的 bind_group_layout（碎片着色器只用 binding 0 的 camera uniform）
        let debris_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("碎片管线布局"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let debris_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("碎片渲染管线"),
            layout: Some(&debris_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &debris_vs_module,
                entry_point: Some("debris_vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<DebrisInstance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x3,
                            },
                            wgpu::VertexAttribute {
                                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32,
                            },
                            wgpu::VertexAttribute {
                                offset: (std::mem::size_of::<[f32; 3]>()
                                    + std::mem::size_of::<f32>())
                                    as wgpu::BufferAddress,
                                shader_location: 3,
                                format: wgpu::VertexFormat::Float32,
                            },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &debris_fs_module,
                entry_point: Some("debris_fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // 四边形顶点（两个三角形组成一个 quad，uv 范围 [-1, 1]）
        let quad_verts: [QuadVertex; 4] = [
            QuadVertex {
                position: [-1.0, -1.0],
            },
            QuadVertex {
                position: [1.0, -1.0],
            },
            QuadVertex {
                position: [1.0, 1.0],
            },
            QuadVertex {
                position: [-1.0, 1.0],
            },
        ];
        let quad_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("四边形顶点缓冲"),
            contents: bytemuck::cast_slice(&quad_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_idx: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let quad_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("四边形索引缓冲"),
            contents: bytemuck::cast_slice(&quad_idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let quad_index_count = quad_idx.len() as u32;

        // 碎片实例缓冲（最多 600 个粒子，动态更新）
        const DEBRIS_MAX: usize = 600;
        let debris_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("碎片实例缓冲"),
            size: (DEBRIS_MAX * std::mem::size_of::<DebrisInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 轨迹实例缓冲（最多 4000 个点）
        const TRAIL_MAX: usize = 4000;
        let trail_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("轨迹实例缓冲"),
            size: (TRAIL_MAX * std::mem::size_of::<TrailInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 轨迹渲染管线（复用 quad 顶点，独立着色器）
        let trail_vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("轨迹顶点着色器"),
            source: wgpu::ShaderSource::Wgsl(TRAIL_VERTEX_SHADER.into()),
        });
        let trail_fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("轨迹片段着色器"),
            source: wgpu::ShaderSource::Wgsl(TRAIL_FRAGMENT_SHADER.into()),
        });
        let trail_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("轨迹渲染管线"),
            layout: Some(&debris_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &trail_vs_module,
                entry_point: Some("trail_vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<TrailInstance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x3,
                            },
                            wgpu::VertexAttribute {
                                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32,
                            },
                            wgpu::VertexAttribute {
                                offset: (std::mem::size_of::<[f32; 3]>()
                                    + std::mem::size_of::<f32>())
                                    as wgpu::BufferAddress,
                                shader_location: 3,
                                format: wgpu::VertexFormat::Float32,
                            },
                            wgpu::VertexAttribute {
                                offset: (std::mem::size_of::<[f32; 3]>()
                                    + std::mem::size_of::<f32>()
                                    + std::mem::size_of::<f32>())
                                    as wgpu::BufferAddress,
                                shader_location: 4,
                                format: wgpu::VertexFormat::Float32,
                            },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &trail_fs_module,
                entry_point: Some("trail_fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Tendex 线着色器
        let tendex_vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tendex 顶点着色器"),
            source: wgpu::ShaderSource::Wgsl(TENDEX_VERTEX_SHADER.into()),
        });
        let tendex_fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tendex 片段着色器"),
            source: wgpu::ShaderSource::Wgsl(TENDEX_FRAGMENT_SHADER.into()),
        });

        // Tendex 管线布局（复用相机+模型 bind group 布局，模型 uniform 不使用，传偏移 0）
        let tendex_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Tendex 管线布局"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        // Tendex 渲染管线：LineList 拓扑，alpha blending，不写深度
        let tendex_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tendex 渲染管线"),
            layout: Some(&tendex_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &tendex_vs_module,
                entry_point: Some("tendex_vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[TendexVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &tendex_fs_module,
                entry_point: Some("tendex_fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Tendex 顶点缓冲：固定大小，每帧通过 write_buffer 更新
        // 20*20*20 采样点 * 3 线段/点 * 2 顶点/线段 = 96000 顶点，向上取整到 100000
        const MAX_TENDEX_VERTICES: u64 = 100000;
        let tendex_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tendex 顶点缓冲"),
            size: MAX_TENDEX_VERTICES * std::mem::size_of::<TendexVertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device,
            queue,
            surface,
            config,
            depth_texture,
            render_pipeline,
            sphere_vertices,
            sphere_indices,
            sphere_index_count,
            ring_vertices,
            ring_indices,
            ring_index_count,
            camera_buffer,
            model_buffer,
            bhs_buffer,
            bind_group,
            debris_pipeline,
            debris_instance_buffer,
            trail_instance_buffer,
            trail_pipeline,
            quad_vertices,
            quad_indices,
            quad_index_count,
            tendex_pipeline,
            tendex_vertex_buffer,
            tendex_vertex_count: 0,
            bg_pipeline,
        }
    }
}

pub(super) fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("深度纹理"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
