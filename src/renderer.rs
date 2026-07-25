// wgpu 渲染器模块 - 3D 渲染管线与 WGSL 着色器

use bytemuck::{Pod, Zeroable};
use nalgebra::{Matrix4, Vector3};
use wgpu::util::DeviceExt;

use crate::camera::OrbitCamera;
use crate::geometry::{create_ring, create_sphere, Vertex};

/// 相机 uniform：视图投影矩阵
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view_inv: [[f32; 4]; 4],
    pub proj_inv: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub _pad: f32,
}

/// 模型 uniform：模型矩阵、对象类型、时间、alpha
/// 手动填充到 256 字节以满足 wgpu 动态偏移对齐要求
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ModelUniform {
    pub model: [[f32; 4]; 4],
    pub object_type: u32,
    pub time: f32,
    pub alpha: f32,
    pub bh_mass: f32,
    pub _pad: [f32; 44],
}

impl ModelUniform {
    pub fn new(model: [[f32; 4]; 4], object_type: u32, time: f32, alpha: f32, bh_mass: f32) -> Self {
        Self {
            model,
            object_type,
            time,
            alpha,
            bh_mass,
            _pad: [0.0; 44],
        }
    }
}

const MODEL_UNIFORM_SIZE: u64 = std::mem::size_of::<ModelUniform>() as u64;
const MAX_OBJECTS: usize = 64;

/// 黑洞数据（用于着色器）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BlackHoleData {
    pub pos: [f32; 3],
    pub mass: f32,
}

/// 黑洞数组 uniform
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BlackHolesUniform {
    pub count: u32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub holes: [BlackHoleData; 8],
}

/// 碎片实例数据（每个粒子一份，instanced billboard）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct DebrisInstance {
    pub pos: [f32; 3],
    pub speed: f32,
    pub life: f32,
    pub _pad: [f32; 3],
}

/// 四边形顶点（billboard 用，2D 位置）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct QuadVertex {
    pub position: [f32; 2],
}

/// 顶点着色器
const VERTEX_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

struct ModelUniform {
    model: mat4x4<f32>,
    object_type: u32,
    time: f32,
    alpha: f32,
    bh_mass: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) object_type: u32,
    @location(3) world_pos: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) view_dir: vec3<f32>,
    @location(6) bh_center: vec3<f32>,
    @location(7) bh_mass: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = model.model * vec4<f32>(input.position, 1.0);
    out.clip_position = camera.view_proj * world;
    out.uv = input.uv;
    out.normal = (model.model * vec4<f32>(input.normal, 0.0)).xyz;
    out.object_type = model.object_type;
    out.world_pos = world.xyz;
    out.alpha = model.alpha;
    out.view_dir = normalize(camera.camera_pos - world.xyz);
    out.bh_center = model.model[3].xyz;
    out.bh_mass = model.bh_mass;
    return out;
}
"#;

/// 片段着色器 - 基于物理的黑洞渲染（全屏幕引力透镜）
const FRAGMENT_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) object_type: u32,
    @location(3) world_pos: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) view_dir: vec3<f32>,
    @location(6) bh_center: vec3<f32>,
    @location(7) bh_mass: f32,
};

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

struct ModelUniform {
    model: mat4x4<f32>,
    object_type: u32,
    time: f32,
    alpha: f32,
    bh_mass: f32,
};

struct BlackHoleData {
    pos: vec3<f32>,
    mass: f32,
};

struct BlackHolesUniform {
    count: u32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    holes: array<BlackHoleData, 8>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> model: ModelUniform;
@group(0) @binding(2) var<uniform> bhs: BlackHolesUniform;

const PI: f32 = 3.14159265359;

fn schwarzschild_radius(mass: f32) -> f32 {
    return 2.0 * mass;
}

fn photon_sphere_radius(mass: f32) -> f32 {
    return 3.0 * mass;
}

fn isco_radius(mass: f32) -> f32 {
    return 6.0 * mass;
}

fn accretion_disk_color(r: f32, r_inner: f32, r_outer: f32) -> vec3<f32> {
    let t = clamp((r - r_inner) / (r_outer - r_inner), 0.0, 1.0);
    let inner_col = vec3<f32>(1.0, 0.95, 0.7);
    let mid_col = vec3<f32>(1.0, 0.55, 0.1);
    let outer_col = vec3<f32>(0.45, 0.08, 0.04);
    var col = mix(inner_col, mid_col, smoothstep(0.0, 0.5, t));
    col = mix(col, outer_col, smoothstep(0.5, 1.0, t));
    let temp = pow(1.0 - t * 0.6, 1.5);
    return col * temp;
}

fn hash3(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn star_field(dir: vec3<f32>) -> vec3<f32> {
    // 银河平面法线：水平带（Y 轴向上）
    let galaxy_normal = vec3<f32>(0.0, 1.0, 0.0);
    let galaxy_dist = abs(dot(dir, galaxy_normal));
    // 更窄更笔直的银河带
    let galaxy_band = exp(-galaxy_dist * galaxy_dist * 20.0);
    let galaxy_noise = hash3(dir * 15.0) * 0.3 + hash3(dir * 40.0) * 0.2;
    let galaxy_bright = galaxy_band * (0.9 + galaxy_noise * 0.2);

    // 深空背景
    var bg = vec3<f32>(0.002, 0.003, 0.008);

    // 银河带颜色：保持高亮度
    let galaxy_core = vec3<f32>(0.3, 0.25, 0.15);
    let galaxy_mid = vec3<f32>(0.15, 0.12, 0.1);
    let galaxy_outer = vec3<f32>(0.05, 0.05, 0.1);
    var galaxy_col = mix(galaxy_outer, galaxy_mid, smoothstep(0.0, 0.5, galaxy_band));
    galaxy_col = mix(galaxy_col, galaxy_core, smoothstep(0.5, 1.0, galaxy_band));
    // 减少尘埃噪声
    let dust = sin(atan2(dir.z, dir.x) * 3.0) * 0.5 + 0.5;
    galaxy_col = galaxy_col * (0.6 + dust * 0.4);
    bg = bg + galaxy_col * galaxy_bright * 2.5;

    // 星云
    var nebula_p = dir * 6.0;
    var nebula_col = vec3<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        let q = floor(nebula_p);
        let id = hash3(q + vec3<f32>(f32(i) * 13.0));
        if (id > 0.95) {
            let nx = hash3(q + vec3<f32>(1.0, 2.0, 3.0));
            let ny = hash3(q + vec3<f32>(4.0, 5.0, 6.0));
            let nz = hash3(q + vec3<f32>(7.0, 8.0, 9.0));
            let center = normalize(vec3<f32>(nx, ny, nz) * 2.0 - 1.0);
            let d = distance(dir, center);
            let glow = exp(-d * d * 50.0) * max(galaxy_band, 0.15);
            let ncol_id = hash3(q + vec3<f32>(10.0, 20.0, 30.0));
            var n_col = mix(vec3<f32>(0.5, 0.18, 0.4), vec3<f32>(0.18, 0.35, 0.6), ncol_id);
            n_col = mix(n_col, vec3<f32>(0.6, 0.3, 0.18), step(0.6, ncol_id));
            nebula_col = nebula_col + n_col * glow;
        }
        nebula_p = nebula_p * 1.6;
    }
    bg = bg + nebula_col * 0.7;

    // 恒星：更亮更多
    var p = dir * 150.0;
    var star = 0.0;
    var star_col = vec3<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        let q = floor(p);
        let f = fract(p);
        let id = hash3(q);
        let threshold = mix(0.995, 0.975, galaxy_band);
        let s = step(threshold, id);
        let cx = hash3(q + vec3<f32>(1.0, 2.0, 3.0));
        let cy = hash3(q + vec3<f32>(4.0, 5.0, 6.0));
        let cz = hash3(q + vec3<f32>(7.0, 8.0, 9.0));
        let center = vec3<f32>(cx, cy, cz) * 0.5 + 0.25;
        let d = distance(f, center);
        let bright = s * smoothstep(0.08, 0.0, d);
        let col_id = hash3(q + vec3<f32>(10.0, 20.0, 30.0));
        var s_col = mix(vec3<f32>(0.95, 0.97, 1.0), vec3<f32>(1.0, 0.9, 0.7), col_id);
        s_col = mix(s_col, vec3<f32>(0.8, 0.9, 1.0), step(0.7, col_id));
        s_col = mix(s_col, vec3<f32>(1.0, 0.65, 0.55), step(0.9, col_id));
        star = star + bright;
        star_col = star_col + s_col * bright * 1.2;
        p = p * 2.0;
    }
    if (star > 0.0) {
        bg = bg + star_col;
    }

    return bg;
}

fn ray_sphere_intersect(ro: vec3<f32>, rd: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let oc = ro - center;
    let b = dot(oc, rd);
    let c = dot(oc, oc) - radius * radius;
    let h = b * b - c;
    if (h < 0.0) { return vec2<f32>(-1.0, -1.0); }
    let s = sqrt(h);
    return vec2<f32>(-b - s, -b + s);
}

fn ray_plane_intersect(ro: vec3<f32>, rd: vec3<f32>, plane_p: vec3<f32>, plane_n: vec3<f32>) -> f32 {
    let denom = dot(plane_n, rd);
    if (abs(denom) < 0.0001) { return -1.0; }
    let t = dot(plane_p - ro, plane_n) / denom;
    return t;
}

fn rotate_vector_towards(v: vec3<f32>, tgt: vec3<f32>, angle: f32) -> vec3<f32> {
    let axis = cross(v, tgt);
    let axis_len = length(axis);
    if (axis_len < 0.0001) { return v; }
    let axis_n = axis / axis_len;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let result = v * cos_a + cross(axis_n, v) * sin_a + axis_n * dot(axis_n, v) * (1.0 - cos_a);
    return normalize(result);
}

fn compute_lensed_direction(ro: vec3<f32>, rd: vec3<f32>) -> vec3<f32> {
    var dir = rd;
    var hit_horizon = false;

    for (var idx = 0u; idx < bhs.count; idx = idx + 1u) {
        let bh = bhs.holes[idx];
        let mass = bh.mass;
        let bh_pos = bh.pos;
        let rs = schwarzschild_radius(mass);

        let to_bh = bh_pos - ro;
        let dist = length(to_bh);
        let to_bh_n = to_bh / dist;

        let perp = cross(dir, to_bh_n);
        let perp_len = length(perp);
        let impact_param = perp_len * dist;

        // 临界碰撞参数 b_c = 3√3·M (Synge 1966)
        // b < b_c 的光线被黑洞捕获
        let b_c = 3.0 * sqrt(3.0) * mass;
        if (impact_param < b_c) {
            hit_horizon = true;
            break;
        }

        // 引力偏折角：α = 4M/b (一阶) + 15π/4 · (M/b)² (二阶)
        // 参考：Weinberg 1972, Keeton & Petters 2005
        var bend_angle = 4.0 * mass / impact_param + (15.0 * PI / 4.0) * mass * mass / (impact_param * impact_param);
        bend_angle = min(bend_angle, PI * 1.5);

        let bend_axis = normalize(cross(dir, to_bh_n));
        dir = rotate_vector_towards(dir, to_bh_n, bend_angle * 0.5);
    }

    if (hit_horizon) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }

    return dir;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.object_type == 0u) {
        // 事件视界：纯黑色，不透明
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    } else if (input.object_type == 1u) {
        discard;
    } else if (input.object_type == 2u) {
        let center = input.bh_center;
        let p = input.world_pos;
        let to_p = p - center;
        let r = length(to_p);
        
        let thickness = 0.1;
        let ring = 1.0 - smoothstep(0.0, thickness, abs(r - 1.0));
        
        let phi = atan2(to_p.z, to_p.x);
        let theta = acos(clamp(to_p.y / max(r, 0.001), -1.0, 1.0));
        
        let time_phase = model.time * 4.0;
        let animated = 0.5 + 0.5 * cos(2.0 * phi + time_phase) * sin(theta) * sin(theta);
        
        var col = vec3<f32>(0.3, 0.7, 1.0);
        let intensity = ring * animated * input.alpha * 0.9;
        
        if (intensity < 0.015) {
            discard;
        }
        
        return vec4<f32>(col, intensity);
    } else if (input.object_type == 3u) {
        let ro = camera.camera_pos;
        let rd = normalize(input.world_pos - camera.camera_pos);

        if (bhs.count == 0u) {
            return vec4<f32>(star_field(rd), 1.0);
        }

        // 使用碰撞参数 b 判断光线是否被黑洞捕获
        // 临界碰撞参数 b_c = 3√3·M ≈ 5.196·M (Synge 1966)
        // b < b_c: 光线被捕获（黑洞阴影）
        // b ≈ b_c: 光子环（薄亮环）
        var hit_black = false;
        var photon_ring = 0.0;

        for (var idx = 0u; idx < bhs.count; idx = idx + 1u) {
            let bh = bhs.holes[idx];
            let mass = bh.mass;
            let b_c = 3.0 * sqrt(3.0) * mass;  // 临界碰撞参数

            let to_bh = bh.pos - ro;
            // 碰撞参数 b = |rd × to_bh|（rd 为单位向量）
            let cross_v = cross(rd, to_bh);
            let b = length(cross_v);

            if (b < b_c) {
                hit_black = true;
            }

            // 光子环：b ≈ b_c 时出现薄亮环
            // 环宽约为 Rs 的 3%（高阶像衰减决定，非常细）
            let rs = schwarzschild_radius(mass);
            let ring_width = rs * 0.03;
            let ring_dist = abs(b - b_c);
            if (ring_dist < ring_width) {
                let ring_intensity = pow(1.0 - ring_dist / ring_width, 3.0);
                photon_ring = max(photon_ring, ring_intensity);
            }
        }

        if (hit_black) {
            // 阴影区域内，如果有光子环则绘制
            if (photon_ring > 0.0) {
                let ring_col = vec3<f32>(1.0, 0.85, 0.5) * photon_ring * 3.0;
                return vec4<f32>(ring_col, 1.0);
            }
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }

        let lensed_dir = compute_lensed_direction(ro, rd);

        if (length(lensed_dir) < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }

        var bg_col = star_field(lensed_dir);

        // 阴影外若有光子环，叠加到背景上
        if (photon_ring > 0.0) {
            let ring_col = vec3<f32>(1.0, 0.85, 0.5) * photon_ring * 3.0;
            bg_col = bg_col + ring_col;
        }

        return vec4<f32>(bg_col, 1.0);
    } else if (input.object_type == 4u) {
        // 普通天体（恒星）：球面着色，中心亮边缘暗，颜色偏黄白
        let n = normalize(input.normal);
        let view_dir = normalize(input.view_dir);
        let ndotv = max(dot(n, view_dir), 0.0);
        
        // 临边昏暗（limb darkening）
        let limb = pow(ndotv, 0.5);
        
        let base_col = vec3<f32>(1.0, 0.95, 0.75);
        var col = base_col * limb;
        
        // 中心加亮（高光）
        let center_boost = pow(ndotv, 4.0) * 0.3;
        col = col + vec3<f32>(1.0, 0.9, 0.6) * center_boost;
        
        return vec4<f32>(col, 1.0);
    }
    return vec4<f32>(1.0, 0.0, 1.0, 1.0);
}
"#;

/// 碎片粒子顶点着色器（instanced billboard）
const DEBRIS_VERTEX_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct DebrisVSInput {
    @location(0) quad_pos: vec2<f32>,
    @location(1) instance_pos: vec3<f32>,
    @location(2) instance_speed: f32,
    @location(3) instance_life: f32,
};

struct DebrisVSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) speed: f32,
    @location(2) life: f32,
};

@vertex
fn debris_vs_main(input: DebrisVSInput) -> DebrisVSOutput {
    var out: DebrisVSOutput;

    // 实例位置投影到裁剪空间
    let center_clip = camera.view_proj * vec4<f32>(input.instance_pos, 1.0);

    // billboard 大小随距离自适应
    let dist = length(camera.camera_pos - input.instance_pos);
    let base_size = 0.15 + input.instance_speed * 0.05;
    let scale = base_size * center_clip.w / max(dist, 0.1);

    // 在裁剪空间中偏移形成 billboard
    let offset = input.quad_pos * scale;
    out.clip_position = vec4<f32>(
        center_clip.x + offset.x,
        center_clip.y + offset.y,
        center_clip.z,
        center_clip.w,
    );

    out.uv = input.quad_pos;
    out.speed = input.instance_speed;
    out.life = input.instance_life;

    return out;
}
"#;

/// 碎片粒子片段着色器（圆形发光点，颜色基于速度，additive blending）
const DEBRIS_FRAGMENT_SHADER: &str = r#"
struct DebrisVSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) speed: f32,
    @location(2) life: f32,
};

@fragment
fn debris_fs_main(input: DebrisVSOutput) -> @location(0) vec4<f32> {
    let d = length(input.uv);
    if (d > 1.0) {
        discard;
    }

    // 圆形发光点：中心亮，边缘衰减
    let intensity = pow(1.0 - d, 2.0);

    // 颜色基于速度：速度高 = 白蓝色，速度低 = 橙红色
    let fast_col = vec3<f32>(0.85, 0.92, 1.0);
    let slow_col = vec3<f32>(1.0, 0.4, 0.1);
    let t = clamp(input.speed * 0.5, 0.0, 1.0);
    var col = mix(slow_col, fast_col, t);

    // life 衰减（碎片寿命越长越暗）
    let life_factor = 1.0 / (1.0 + input.life * 0.3);
    col = col * intensity * life_factor * 2.5;

    return vec4<f32>(col, intensity * life_factor);
}
"#;

/// wgpu 渲染器
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub depth_texture: wgpu::TextureView,
    pub render_pipeline: wgpu::RenderPipeline,
    pub sphere_vertices: wgpu::Buffer,
    pub sphere_indices: wgpu::Buffer,
    pub sphere_index_count: u32,
    pub ring_vertices: wgpu::Buffer,
    pub ring_indices: wgpu::Buffer,
    pub ring_index_count: u32,
    pub camera_buffer: wgpu::Buffer,
    pub model_buffer: wgpu::Buffer,
    pub bhs_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub debris_pipeline: wgpu::RenderPipeline,
    pub debris_instance_buffer: wgpu::Buffer,
    pub quad_vertices: wgpu::Buffer,
    pub quad_indices: wgpu::Buffer,
    pub quad_index_count: u32,
}

impl Renderer {
    pub fn new(window: &'static winit::window::Window) -> Self {
        let instance_desc = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        };
        let instance = wgpu::Instance::new(&instance_desc);

        let surface = instance
            .create_surface(window)
            .expect("无法创建 surface");

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
        let debris_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
            QuadVertex { position: [-1.0, -1.0] },
            QuadVertex { position: [1.0, -1.0] },
            QuadVertex { position: [1.0, 1.0] },
            QuadVertex { position: [-1.0, 1.0] },
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
            quad_vertices,
            quad_indices,
            quad_index_count,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = create_depth_texture(&self.device, &self.config);
    }

    /// 渲染 3D 场景
    /// waves: 引力波对象 (位置, 缩放, 类型, alpha, 黑洞质量)
    /// black_holes: 黑洞列表 (位置, 质量)
    /// bodies: 天体列表 (位置, 质量)
    /// debris: 碎片粒子列表 (位置, 速度大小, 寿命)
    pub fn render(
        &mut self,
        camera: &OrbitCamera,
        waves: &[(Vector3<f32>, f32, u32, f32, f32)],
        black_holes: &[(Vector3<f32>, f32)],
        bodies: &[([f32; 3], f32)],
        debris: &[([f32; 3], f32, f32)],
        show_waves: bool,
        time: f32,
    ) -> Result<(wgpu::SurfaceTexture, wgpu::TextureView), wgpu::SurfaceError> {
        let aspect = self.config.width as f32 / self.config.height as f32;
        let proj = Matrix4::new_perspective(aspect, 60.0_f32.to_radians(), 0.1, 10000.0);
        let view = camera.view_matrix();
        let view_proj = proj * view;
        let view_inv = view.try_inverse().unwrap_or(Matrix4::identity());
        let proj_inv = proj.try_inverse().unwrap_or(Matrix4::identity());
        let cam_pos = camera.position();
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
            count: black_holes.len() as u32,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            holes: [BlackHoleData {
                pos: [0.0, 0.0, 0.0],
                mass: 0.0,
            }; 8],
        };
        for (i, (pos, mass)) in black_holes.iter().take(8).enumerate() {
            bhs_uniform.holes[i] = BlackHoleData {
                pos: [pos.x, pos.y, pos.z],
                mass: *mass,
            };
        }
        self.queue.write_buffer(
            &self.bhs_buffer,
            0,
            bytemuck::cast_slice(&[bhs_uniform]),
        );

        // 准备碎片实例数据（最多 600 个粒子，每帧只写入实际粒子数）
        let debris_instances: Vec<DebrisInstance> = debris
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

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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

            render_pass.set_pipeline(&self.render_pipeline);

            // 在 render pass 开始之前准备所有 model uniform
            let mut draw_calls: Vec<(u32, u32)> = Vec::new(); // (dynamic_offset, obj_type)
            let mut uniforms: Vec<ModelUniform> = Vec::new();

            // 1. 背景天球（最远，先渲染）
            {
                let idx = uniforms.len() as u32;
                let m = Matrix4::new_scaling(500.0);
                let model_matrix = Matrix4::new_translation(&Vector3::new(0.0, 0.0, 0.0)) * m;
                uniforms.push(ModelUniform::new(model_matrix.into(), 3, time, 1.0, 0.0));
                draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 3));
            }

            // 2. 天体（中间）
            for (pos, mass) in bodies.iter() {
                let scale = mass.powf(0.4) * 0.8;
                let idx = uniforms.len() as u32;
                let m = Matrix4::new_scaling(scale);
                let model_matrix = Matrix4::new_translation(&Vector3::new(pos[0], pos[1], pos[2])) * m;
                uniforms.push(ModelUniform::new(model_matrix.into(), 4, time, 1.0, 0.0));
                draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 4));
            }

            // 3. 引力波
            if show_waves {
                for (pos, scale, obj_type, alpha, bh_mass) in waves.iter() {
                    let idx = uniforms.len() as u32;
                    let m = Matrix4::new_scaling(*scale);
                    let model_matrix = Matrix4::new_translation(pos) * m;
                    uniforms.push(ModelUniform::new(model_matrix.into(), *obj_type, time, *alpha, *bh_mass));
                    draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, *obj_type));
                }
            }

            // 4. 黑洞（事件视界，最近，最后渲染，遮挡后面的物体）
            for (pos, mass) in black_holes.iter() {
                let idx = uniforms.len() as u32;
                let event_horizon_r = 2.0 * mass;
                let m = Matrix4::new_scaling(event_horizon_r);
                let model_matrix = Matrix4::new_translation(pos) * m;
                uniforms.push(ModelUniform::new(model_matrix.into(), 0, time, 1.0, *mass));
                draw_calls.push((idx * MODEL_UNIFORM_SIZE as u32, 0));
            }

            // 在 render pass 之前一次性写入所有 uniform
            for (i, u) in uniforms.iter().enumerate() {
                self.queue.write_buffer(
                    &self.model_buffer,
                    (i as u64) * MODEL_UNIFORM_SIZE,
                    bytemuck::cast_slice(std::slice::from_ref(u)),
                );
            }

            // 渲染每个对象，使用动态偏移
            for &(offset, obj_type) in draw_calls.iter() {
                render_pass.set_bind_group(0, &self.bind_group, &[offset]);

                match obj_type {
                    0 | 2 | 3 | 4 => {
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

            // 5. 最后画碎片粒子（instanced billboard，additive blending，不写深度）
            if debris_count > 0 {
                render_pass.set_pipeline(&self.debris_pipeline);
                // 碎片管线只用 camera uniform，model uniform 不使用，传 0 偏移
                render_pass.set_bind_group(0, &self.bind_group, &[0]);
                render_pass.set_vertex_buffer(0, self.quad_vertices.slice(..));
                render_pass.set_vertex_buffer(1, self.debris_instance_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.quad_indices.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(0..self.quad_index_count, 0, 0..debris_count);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        Ok((output, view))
    }
}

fn create_depth_texture(
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
