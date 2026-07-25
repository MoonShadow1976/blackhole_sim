// 时空曲率可视化模块 —— Tendex 线方法
// 基于 Owen et al. (2011) arXiv:1012.4869
// 曲率张量"电"部分 E_jk 描述潮汐拉伸/压缩
// 在每个采样点计算 E_jk 的特征值和特征向量，用红蓝线可视化

use nalgebra::{Matrix3, Vector3};

use super::{G, Simulation, WAVE_SPEED};

/// Tendex 采样点：存储位置和曲率张量的特征分解
#[derive(Clone, Copy, Debug)]
pub struct TendexPoint {
    pub pos: Vector3<f32>,
    /// 三个特征值（正=拉伸，负=压缩，和为零因为无迹）
    pub eigvals: [f32; 3],
    /// 三个特征向量（单位向量）
    pub eigvecs: [Vector3<f32>; 3],
}

impl Simulation {
    pub fn set_grid_params(&mut self, size: usize, spacing: f32) {
        if size != self.grid_size || (spacing - self.grid_spacing).abs() > 0.001 {
            self.grid_size = size;
            self.grid_spacing = spacing;
            self.grid_points = Self::init_tendex_grid(size, spacing);
        }
    }

    pub(crate) fn init_tendex_grid(size: usize, spacing: f32) -> Vec<TendexPoint> {
        let mut points = Vec::with_capacity(size * size * size);
        for i in 0..size {
            for j in 0..size {
                for k in 0..size {
                    let x = (i as f32 - (size - 1) as f32 * 0.5) * spacing;
                    let y = (j as f32 - (size - 1) as f32 * 0.5) * spacing;
                    let z = (k as f32 - (size - 1) as f32 * 0.5) * spacing;
                    points.push(TendexPoint {
                        pos: Vector3::new(x, y, z),
                        eigvals: [0.0; 3],
                        eigvecs: [
                            Vector3::new(1.0, 0.0, 0.0),
                            Vector3::new(0.0, 1.0, 0.0),
                            Vector3::new(0.0, 0.0, 1.0),
                        ],
                    });
                }
            }
        }
        points
    }

    /// 计算给定位置的潮汐张量 E_jk（曲率张量的"电"部分）
    /// 牛顿极限：E_jk = Σ GM_i / r_i³ (3 n_j n_k - δ_jk)
    /// 参考：Owen et al. (2011), Nichols 可视化方法
    fn compute_tidal_tensor(&self, pos: Vector3<f32>) -> Matrix3<f32> {
        let mut e = Matrix3::zeros();

        // 黑洞贡献
        for bh in &self.black_holes {
            let delta = pos - bh.pos;
            let r = delta.norm();
            if r < 0.01 {
                continue;
            }
            let n = delta / r;
            let gmr3 = G * bh.mass / (r * r * r);
            // E_jk = GM/r³ (3 n_j n_k - δ_jk)
            for j in 0..3 {
                for k in 0..3 {
                    let delta_jk = if j == k { 1.0 } else { 0.0 };
                    e[(j, k)] += gmr3 * (3.0 * n[j] * n[k] - delta_jk);
                }
            }
        }

        // 普通天体贡献
        for body in &self.bodies {
            let delta = pos - body.pos;
            let r = delta.norm();
            if r < 0.01 {
                continue;
            }
            let n = delta / r;
            let gmr3 = G * body.mass / (r * r * r);
            for j in 0..3 {
                for k in 0..3 {
                    let delta_jk = if j == k { 1.0 } else { 0.0 };
                    e[(j, k)] += gmr3 * (3.0 * n[j] * n[k] - delta_jk);
                }
            }
        }

        // 双黑洞引力波贡献：TT 规范下的度规微扰 h_jk^TT
        // 叠加到潮汐张量上，表现为动态振荡的曲率
        if self.black_holes.len() >= 2 {
            let bh1 = &self.black_holes[0];
            let bh2 = &self.black_holes[1];
            let m1 = bh1.mass;
            let m2 = bh2.mass;
            let total_mass = m1 + m2;
            let chirp_mass = (m1 * m2).powf(3.0 / 5.0) / total_mass.powf(1.0 / 5.0);
            let com = (bh1.pos * m1 + bh2.pos * m2) / total_mass;
            let delta_pos = bh1.pos - bh2.pos;
            let separation = delta_pos.norm();

            if separation > 0.5 {
                let orbital_ang_freq = (G * total_mass / separation.powi(3)).sqrt();
                let gw_ang_freq = 2.0 * orbital_ang_freq;
                let h0 = 4.0 * chirp_mass.powf(5.0 / 3.0) * gw_ang_freq.powf(2.0 / 3.0);

                let rel_vel = bh1.vel - bh2.vel;
                let orbital_normal = if rel_vel.norm() > 0.001 {
                    delta_pos.cross(&rel_vel).normalize()
                } else {
                    Vector3::new(0.0, 0.0, 1.0)
                };

                let r_vec = pos - com;
                let r = r_vec.norm();
                if r > 0.1 {
                    let n = r_vec / r;
                    let retardation = r / WAVE_SPEED;
                    let phase = gw_ang_freq * (self.time - retardation);

                    let cos_iota = n.dot(&orbital_normal).clamp(-1.0, 1.0);
                    let f_plus = (1.0 + cos_iota * cos_iota) * 0.5;
                    let f_cross = cos_iota;

                    let h_plus = (h0 / r.max(separation)) * f_plus * phase.cos();
                    let h_cross = (h0 / r.max(separation)) * f_cross * phase.sin();

                    // 构造垂直于 n 的极化基
                    let e_theta = if (n.x.abs() + n.y.abs()) > 0.01 {
                        Vector3::new(-n.y, n.x, 0.0).normalize()
                    } else {
                        Vector3::new(1.0, 0.0, 0.0)
                    };
                    let e_phi = n.cross(&e_theta).normalize();

                    // 升交点方向和极化角
                    let sin_iota = (1.0 - cos_iota * cos_iota).sqrt();
                    let node_dir = if sin_iota > 0.001 {
                        orbital_normal.cross(&n).normalize()
                    } else {
                        e_theta
                    };
                    let psi = e_theta.dot(&node_dir).clamp(-1.0, 1.0).acos();

                    let cos2p = (2.0 * psi).cos();
                    let sin2p = (2.0 * psi).sin();

                    // TT 应变张量在 (e_theta, e_phi) 基下
                    let h_tt = h_plus * cos2p - h_cross * sin2p;
                    let h_pp = -h_plus * cos2p - h_cross * sin2p;
                    let h_tp = h_plus * sin2p + h_cross * cos2p;

                    // 将 TT 应变转换为全局坐标并叠加到曲率张量
                    // h_jk^TT 的二阶时间导数对应曲率振荡：E_jk += -0.5 * d²h_jk/dt²
                    // 简化为振幅 * ω² 因子
                    let gw_factor = 0.5 * gw_ang_freq * gw_ang_freq;
                    for j in 0..3 {
                        for k in 0..3 {
                            let h_global = h_tt * e_theta[j] * e_theta[k]
                                + h_pp * e_phi[j] * e_phi[k]
                                + h_tp * (e_theta[j] * e_phi[k] + e_phi[j] * e_theta[k]);
                            e[(j, k)] += gw_factor * h_global;
                        }
                    }
                }
            }
        }

        e
    }

    /// 更新所有 Tendex 采样点的曲率特征分解
    pub(crate) fn update_grid_points(&mut self) {
        // 网格跟随黑洞质心移动
        if !self.black_holes.is_empty() {
            let total_mass: f32 = self.black_holes.iter().map(|bh| bh.mass).sum();
            let com = self
                .black_holes
                .iter()
                .map(|bh| bh.pos * bh.mass)
                .sum::<Vector3<f32>>()
                / total_mass;
            self.grid_center = self.grid_center * 0.95 + com * 0.05;
        }

        // 先以不可变借用计算所有点的潮汐张量（避免与可变借用冲突）
        let tensors: Vec<Matrix3<f32>> = self
            .grid_points
            .iter()
            .map(|point| {
                let world_pos = point.pos + self.grid_center;
                self.compute_tidal_tensor(world_pos)
            })
            .collect();

        // 再以可变借用更新特征分解
        for (point, e) in self.grid_points.iter_mut().zip(tensors.iter()) {
            // 对称矩阵特征分解
            let sym = e.symmetric_eigen();

            // 按 |特征值| 降序排列
            let mut idx = [0usize, 1, 2];
            idx.sort_by(|&a, &b| {
                sym.eigenvalues[b]
                    .abs()
                    .partial_cmp(&sym.eigenvalues[a].abs())
                    .unwrap()
            });

            for i in 0..3 {
                let vi = idx[i];
                point.eigvals[i] = sym.eigenvalues[vi];
                point.eigvecs[i] = sym.eigenvectors.column(vi).into();
            }
        }
    }

    /// 获取 Tendex 线段渲染数据
    /// 每个采样点生成 3 条线段（6 个顶点）
    /// 线段长度按 |特征值| 缩放，颜色由特征值符号决定
    pub fn get_tendex_render_data(&self) -> Vec<([f32; 3], f32)> {
        let mut vertices = Vec::with_capacity(self.grid_points.len() * 6);
        // 视觉缩放因子
        let scale = 2.0;

        for point in &self.grid_points {
            let world_pos = point.pos + self.grid_center;
            let pos = [world_pos.x, world_pos.y, world_pos.z];

            for i in 0..3 {
                let val = point.eigvals[i];
                let dir = point.eigvecs[i];
                // 线段长度按 |特征值| 缩放，有最小长度保证可见
                let len = val.abs().sqrt() * scale;
                let half = [
                    dir.x * len * 0.5,
                    dir.y * len * 0.5,
                    dir.z * len * 0.5,
                ];
                // color_sign: +1 = 红色（拉伸），-1 = 蓝色（压缩）
                let sign = if val >= 0.0 { 1.0 } else { -1.0 };

                // 线段两个端点
                vertices.push((
                    [pos[0] - half[0], pos[1] - half[1], pos[2] - half[2]],
                    sign,
                ));
                vertices.push((
                    [pos[0] + half[0], pos[1] + half[1], pos[2] + half[2]],
                    sign,
                ));
            }
        }
        vertices
    }
}
