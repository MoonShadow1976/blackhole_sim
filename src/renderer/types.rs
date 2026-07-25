// 渲染器数据类型：uniform / 实例 / 顶点结构定义

use bytemuck::{Pod, Zeroable};

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
    pub fn new(
        model: [[f32; 4]; 4],
        object_type: u32,
        time: f32,
        alpha: f32,
        bh_mass: f32,
    ) -> Self {
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

pub(crate) const MODEL_UNIFORM_SIZE: u64 = std::mem::size_of::<ModelUniform>() as u64;
pub(crate) const MAX_OBJECTS: usize = 64;

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

/// 轨迹点实例数据（instanced billboard）
/// color_type: 0=黑洞轨迹(橙), 1=天体轨迹(青), 2=预览天体轨迹(黄), 3=预览黑洞轨迹(粉紫)
/// shape_type: 0=方形(黑洞), 1=三角形(天体)
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TrailInstance {
    pub pos: [f32; 3],
    pub color_type: f32,
    pub shape_type: f32,
    pub fade: f32,
    pub _pad: [f32; 1],
}

/// 四边形顶点（billboard 用，2D 位置）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct QuadVertex {
    pub position: [f32; 2],
}

/// Tendex 线顶点：世界空间位置 + 颜色符号
/// color_sign: +1.0 = 拉伸（红），-1.0 = 压缩（蓝）
/// LineList 拓扑：每两个相邻顶点构成一条线段
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TendexVertex {
    pub position: [f32; 3],
    pub color_sign: f32,
}

impl TendexVertex {
    /// 描述 wgpu 顶点缓冲布局
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TendexVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}
