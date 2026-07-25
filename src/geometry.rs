// 几何体生成模块 - 球体和圆环网格

use bytemuck::{Pod, Zeroable};

/// 顶点数据：位置、法线、UV 坐标
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    /// 描述 wgpu 顶点缓冲布局
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// 创建 UV 球体网格（高分辨率，无缝隙）
/// - radius: 半径
/// - sectors: 经度分段数
/// - stacks: 纬度分段数
pub fn create_sphere(radius: f32, sectors: u32, stacks: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let sector_step = 2.0 * std::f32::consts::PI / sectors as f32;
    let stack_step = std::f32::consts::PI / stacks as f32;

    // 生成顶点（含首尾重复顶点以保证纹理连续）
    for i in 0..=stacks {
        let stack_angle = std::f32::consts::PI / 2.0 - (i as f32) * stack_step;
        let xy = radius * stack_angle.cos();
        let z = radius * stack_angle.sin();

        for j in 0..=sectors {
            let sector_angle = (j as f32) * sector_step;
            let x = xy * sector_angle.cos();
            let y = xy * sector_angle.sin();

            let nx = x / radius;
            let ny = y / radius;
            let nz = z / radius;

            let u = j as f32 / sectors as f32;
            let v = i as f32 / stacks as f32;

            vertices.push(Vertex {
                position: [x, y, z],
                normal: [nx, ny, nz],
                uv: [u, v],
            });
        }
    }

    // 生成索引（三角形带）
    for i in 0..stacks {
        let k1 = i * (sectors + 1);
        let k2 = k1 + sectors + 1;
        for j in 0..sectors {
            if i != 0 {
                indices.push(k1 + j);
                indices.push(k2 + j);
                indices.push(k1 + j + 1);
            }
            if i != stacks - 1 {
                indices.push(k1 + j + 1);
                indices.push(k2 + j);
                indices.push(k2 + j + 1);
            }
        }
    }

    (vertices, indices)
}

/// 创建平铺在 XZ 平面上的圆环（吸积盘）
/// - inner: 内半径
/// - outer: 外半径
/// - segments: 分段数
///   uv.x 从 0（内）到 1（外），uv.y 沿圆周方向
pub fn create_ring(inner: f32, outer: f32, segments: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let angle_step = 2.0 * std::f32::consts::PI / segments as f32;

    for i in 0..=segments {
        let angle = (i as f32) * angle_step;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // 内圈顶点
        vertices.push(Vertex {
            position: [inner * cos_a, 0.0, inner * sin_a],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, i as f32 / segments as f32],
        });
        // 外圈顶点
        vertices.push(Vertex {
            position: [outer * cos_a, 0.0, outer * sin_a],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, i as f32 / segments as f32],
        });
    }

    for i in 0..segments {
        let i0 = i * 2;
        let i1 = i * 2 + 1;
        let i2 = i * 2 + 2;
        let i3 = i * 2 + 3;
        indices.push(i0);
        indices.push(i1);
        indices.push(i2);
        indices.push(i2);
        indices.push(i1);
        indices.push(i3);
    }

    (vertices, indices)
}
