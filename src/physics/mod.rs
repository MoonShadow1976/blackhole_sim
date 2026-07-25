// N体黑洞模拟物理模块
// 自然单位制：G = 1, c = 1

use nalgebra::Vector3;

mod collision;
mod grid;
mod trajectory;

pub use grid::TendexPoint;
#[allow(unused_imports)]
pub use trajectory::TrailData;

pub const DEFAULT_GRID_SIZE: usize = 15;
pub const DEFAULT_GRID_SPACING: f32 = 2.0;

pub(crate) const G: f32 = 1.0;
pub(crate) const WAVE_SPEED: f32 = 1.0;
pub(crate) const MAX_BH: usize = 8;
pub(crate) const MAX_BODIES: usize = 16;
pub(crate) const MAX_DEBRIS: usize = 600;

/// 引力波辐射反作用力加速度 (Peters 1964, 2.5PN 近似)
///
/// 基于双星系统的轨道能量损失率推导：
///   dE/dt = -(32/5) * G⁴ * μ² * M³ / (c⁵ * a⁵)  (Peters 公式)
/// 其中 μ = m_i*m_j/M 为约化质量，M = m_i+m_j 为总质量，a 为半长轴。
///
/// 等效的相对运动阻力加速度（自然单位制 G=c=1）：
///   a_rel_rad = -(32/5) * μ * M² / r⁴ * v_rel
///
/// 拆分到两体（质心系，加速度在伽利略变换下不变）：
///   a_i_rad = +(32/5) * m_i * m_j² / r⁴ * v_rel   (沿 +v_rel 方向)
///   a_j_rad = -(32/5) * m_i² * m_j / r⁴ * v_rel   (沿 -v_rel 方向)
///
/// 物理效应：使轨道能量与角动量逐渐损失，导致双星旋进（inspiral）
/// 并最终合并。当 r 减小时辐射功率急剧增大（∝ 1/r⁴），产生
/// 频率逐渐升高的"啁啾"（chirp）信号。
///
/// Plunge 阶段增强：当 r < r_ISCO ≈ 6M 时（M=m_i+m_j），轨道不再稳定，
/// 黑洞进入快速 plunging 阶段。本模拟在 r < 3M 时线性增强反作用力系数
/// （最高 ×3），模拟近合并阶段的非线性效应。
///
/// 参数:
/// - m_i, m_j: 两体质量
/// - r: 两体间距
/// - v_rel: 相对速度 v_j - v_i
///
/// 返回: (a_i_rad, a_j_rad) 加速度向量
pub(crate) fn gw_radiation_reaction(
    m_i: f32,
    m_j: f32,
    r: f32,
    v_rel: Vector3<f32>,
) -> (Vector3<f32>, Vector3<f32>) {
    let m_total = m_i + m_j;
    // 限制最小有效距离，避免 1/r⁴ 在小间距下数值发散
    // 取 r 与 0.5*(m_i+m_j) 的较大者（合并阈值附近，避免数值爆炸）
    let r_floor = 0.5 * m_total.max(1.0);
    let r_eff = r.max(r_floor);
    let r4 = r_eff * r_eff * r_eff * r_eff;
    let mut coeff = (32.0 / 5.0) / r4;

    // Plunge 阶段增强：r < 3M 时（M=m_i+m_j，对应 0.75*(rs1+rs2)）
    // 轨道已过 ISCO，进入非线性 plunge。线性增强反作用力至最高 3 倍。
    let r_isco = 6.0 * m_total; // ISCO ≈ 6M (试验粒子近似)
    let r_plunge = 3.0 * m_total; // plunge 强增强起点
    if r < r_plunge {
        let t = ((r_plunge - r) / (r_plunge - r_floor).max(0.01)).clamp(0.0, 1.0);
        coeff *= 1.0 + 2.0 * t; // 1× ~ 3×
    }
    // r_isco 仅用于文档/调试，此处不直接使用
    let _ = r_isco;

    let a_i = v_rel * (coeff * m_i * m_j * m_j);
    let a_j = v_rel * (-coeff * m_i * m_i * m_j);
    (a_i, a_j)
}

/// 单个黑洞
#[derive(Clone, Debug)]
pub struct BlackHole {
    pub mass: f32,
    pub pos: Vector3<f32>,
    pub vel: Vector3<f32>,
}

/// 普通天体（恒星/行星，可被洛希极限撕裂或碰撞碎裂）
#[derive(Clone, Debug)]
pub struct CelestialBody {
    pub mass: f32,
    pub pos: Vector3<f32>,
    pub vel: Vector3<f32>,
    /// 材质硬度/粘性 (0.1=松散气体, 1.0=岩石, 2.0=金属)
    /// 硬度越高，碎裂时产生的碎片越多
    pub hardness: f32,
}

/// 碎片粒子（天体被撕裂后的残骸，形成吸积盘）
#[derive(Clone, Debug)]
pub struct DebrisParticle {
    pub pos: Vector3<f32>,
    pub vel: Vector3<f32>,
    pub life: f32,
    /// 碎片质量（用于引力波辐射反作用力计算）
    pub mass: f32,
}

/// N体黑洞系统模拟
pub struct Simulation {
    pub black_holes: Vec<BlackHole>,
    pub bodies: Vec<CelestialBody>,
    pub debris: Vec<DebrisParticle>,
    pub grid_points: Vec<TendexPoint>,
    pub grid_center: Vector3<f32>,
    pub grid_size: usize,
    pub grid_spacing: f32,
    pub time: f32,
    pub show_gravity_waves: bool,
    pub tendex_three_planes: bool,
    pub sim_speed: f32,
    pub paused: bool,
}

impl Simulation {
    pub fn new() -> Self {
        let grid_points = Self::init_tendex_grid(DEFAULT_GRID_SIZE, DEFAULT_GRID_SPACING);
        Self {
            black_holes: vec![BlackHole {
                mass: 2.0,
                pos: Vector3::new(-7.5, 0.0, 0.0),
                vel: Vector3::new(0.0, 0.0, 0.3),
            }],
            bodies: Vec::new(),
            debris: Vec::new(),
            grid_points,
            grid_center: Vector3::zeros(),
            grid_size: DEFAULT_GRID_SIZE,
            grid_spacing: DEFAULT_GRID_SPACING,
            time: 0.0,
            show_gravity_waves: false,
            tendex_three_planes: false,
            sim_speed: 0.5,
            paused: true,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn add_black_hole(&mut self, bh: BlackHole) {
        if self.black_holes.len() < MAX_BH {
            self.black_holes.push(bh);
        }
    }

    pub fn add_body(&mut self, body: CelestialBody) {
        if self.bodies.len() < MAX_BODIES {
            self.bodies.push(body);
        }
    }

    pub fn black_hole_count(&self) -> usize {
        self.black_holes.len()
    }

    /// Schwarzschild 半径 rs = 2*M
    pub fn schwarzschild_radius(mass: f32) -> f32 {
        2.0 * G * mass
    }

    /// 洛希极限：天体被潮汐力撕裂的临界距离
    /// r_roche = R_body * (2 * M_bh / M_body)^(1/3)
    pub fn roche_limit(bh_mass: f32, body_mass: f32) -> f32 {
        let body_radius = Self::body_radius(body_mass);
        body_radius * (2.0 * bh_mass / body_mass).powf(1.0 / 3.0)
    }

    pub fn update(&mut self, dt_real: f32) {
        if self.paused {
            return;
        }
        let dt = dt_real * self.sim_speed;
        if dt <= 0.0 {
            return;
        }
        self.time += dt;

        self.update_black_holes(dt);
        self.update_bodies(dt);
        self.update_debris(dt);
        self.check_mergers();
        self.check_event_horizon_absorption();
        self.check_body_collisions();
        self.check_roche_disruption();

        // 引力波已改为基于轨道参数的连续波模型（在 update_grid_points 中直接计算）
        // 不再使用离散球面波，避免相消干涉和渲染遮挡

        // 更新网格点扭曲
        self.update_grid_points();
    }

    fn update_black_holes(&mut self, dt: f32) {
        let n = self.black_holes.len();
        if n < 2 {
            for bh in self.black_holes.iter_mut() {
                bh.pos += bh.vel * dt;
            }
            return;
        }

        let mut accelerations: Vec<Vector3<f32>> = vec![Vector3::zeros(); n];

        for i in 0..n {
            for j in (i + 1)..n {
                let delta = self.black_holes[j].pos - self.black_holes[i].pos;
                let r = delta.norm().max(0.1);
                let dir = delta / r;
                let m1 = self.black_holes[i].mass;
                let m2 = self.black_holes[j].mass;

                // 牛顿引力
                let force_mag = G * m1 * m2 / (r * r);
                accelerations[i] += dir * force_mag / m1;
                accelerations[j] -= dir * force_mag / m2;

                // 引力波辐射反作用力 (Peters 1964, 2.5PN)
                // 导致轨道能量损失，双黑洞旋进合并
                let v_rel = self.black_holes[j].vel - self.black_holes[i].vel;
                let (a_i_rad, a_j_rad) = gw_radiation_reaction(m1, m2, r, v_rel);
                accelerations[i] += a_i_rad;
                accelerations[j] += a_j_rad;
            }
        }

        let mut new_vels = Vec::with_capacity(n);
        for (bh, acc) in self.black_holes.iter().zip(accelerations.iter()) {
            new_vels.push(bh.vel + acc * dt);
        }
        for (bh, v) in self.black_holes.iter_mut().zip(new_vels.iter()) {
            bh.vel = *v;
            bh.pos += *v * dt;
        }
    }

    fn update_bodies(&mut self, dt: f32) {
        let _n_bh = self.black_holes.len();
        let n_body = self.bodies.len();
        if n_body == 0 {
            return;
        }

        let mut accelerations: Vec<Vector3<f32>> = vec![Vector3::zeros(); n_body];

        // 天体受所有黑洞引力 + 引力波辐射反作用力
        for (i, body) in self.bodies.iter().enumerate() {
            for bh in &self.black_holes {
                let delta = bh.pos - body.pos;
                let r = delta.norm().max(0.1);
                let dir = delta / r;
                let force_mag = G * bh.mass / (r * r);
                accelerations[i] += dir * force_mag;

                // 引力波辐射反作用力 (天体-黑洞对)
                // v_rel = v_bh - v_body (j=bh, i=body)
                let v_rel = bh.vel - body.vel;
                let (a_body_rad, _a_bh_rad) = gw_radiation_reaction(body.mass, bh.mass, r, v_rel);
                accelerations[i] += a_body_rad;
            }
            // 天体之间也有微弱引力
            for (j, other) in self.bodies.iter().enumerate() {
                if i == j {
                    continue;
                }
                let delta = other.pos - body.pos;
                let r = delta.norm().max(0.1);
                let dir = delta / r;
                let force_mag = G * other.mass / (r * r);
                accelerations[i] += dir * force_mag * 0.1;
            }
        }

        let mut new_vels = Vec::with_capacity(n_body);
        for (body, acc) in self.bodies.iter().zip(accelerations.iter()) {
            new_vels.push(body.vel + acc * dt);
        }
        for (body, v) in self.bodies.iter_mut().zip(new_vels.iter()) {
            body.vel = *v;
            body.pos += *v * dt;
        }
    }

    fn update_debris(&mut self, dt: f32) {
        let n = self.debris.len();
        if n == 0 {
            return;
        }

        let mut accelerations: Vec<Vector3<f32>> = vec![Vector3::zeros(); n];

        // 碎片受所有黑洞引力 + 引力波辐射反作用力
        for (i, debris) in self.debris.iter().enumerate() {
            for bh in &self.black_holes {
                let delta = bh.pos - debris.pos;
                let r = delta.norm().max(0.1);
                let dir = delta / r;
                let force_mag = G * bh.mass / (r * r);
                accelerations[i] += dir * force_mag;

                // 引力波辐射反作用力 (碎片-黑洞对)
                let v_rel = bh.vel - debris.vel;
                let (a_debris_rad, _a_bh_rad) =
                    gw_radiation_reaction(debris.mass, bh.mass, r, v_rel);
                accelerations[i] += a_debris_rad;
            }
        }

        let mut new_vels = Vec::with_capacity(n);
        for (debris, acc) in self.debris.iter().zip(accelerations.iter()) {
            new_vels.push(debris.vel + acc * dt);
        }

        for (debris, v) in self.debris.iter_mut().zip(new_vels.iter()) {
            debris.vel = *v;
            debris.pos += *v * dt;
            debris.life += dt;
        }

        // 移除落入事件视界的碎片
        self.debris.retain(|p| {
            for bh in &self.black_holes {
                let rs = Self::schwarzschild_radius(bh.mass);
                if (p.pos - bh.pos).norm() < rs {
                    return false;
                }
            }
            true
        });
    }

    /// 天体半径（从质量推导）
    pub(crate) fn body_radius(mass: f32) -> f32 {
        mass.powf(0.4) * 0.8
    }

    /// 获取引力波渲染对象
    /// 引力波现在通过网格扭曲直接可视化（连续波模型），不再使用离散球面波
    pub fn get_wave_objects(&self) -> Vec<(Vector3<f32>, f32, u32, f32, f32)> {
        Vec::new()
    }

    /// 获取碎片粒子的渲染数据 (位置, 速度大小, 寿命)
    pub fn get_debris_render_data(&self) -> Vec<([f32; 3], f32, f32)> {
        self.debris
            .iter()
            .map(|p| {
                let speed = p.vel.norm();
                ([p.pos.x, p.pos.y, p.pos.z], speed, p.life)
            })
            .collect()
    }

    /// 获取天体的渲染数据 (位置, 质量)
    pub fn get_body_render_data(&self) -> Vec<([f32; 3], f32)> {
        self.bodies
            .iter()
            .map(|b| ([b.pos.x, b.pos.y, b.pos.z], b.mass))
            .collect()
    }

    pub fn phase_string(&self, english: bool) -> String {
        let n = self.black_holes.len();
        let nb = self.bodies.len();
        let nd = self.debris.len();
        if n == 0 {
            if english { "Empty".to_string() } else { "空".to_string() }
        } else if n == 1 && nb == 0 {
            if english {
                format!("Single BH (M={:.2})", self.black_holes[0].mass)
            } else {
                format!("单黑洞 (M={:.2})", self.black_holes[0].mass)
            }
        } else if english {
            format!("{}BH {}Body {}Debris", n, nb, nd)
        } else {
            format!("{}黑洞 {}天体 {}碎片", n, nb, nd)
        }
    }
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}
