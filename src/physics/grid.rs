// 时空曲率可视化模块 —— Tendex 线方法
// 基于 Owen et al. (2011) arXiv:1012.4869
// 曲率张量"电"部分 E_jk 描述潮汐拉伸/压缩
// 在每个采样点计算 E_jk 的特征值和特征向量，用红蓝线可视化

use nalgebra::{Matrix3, Vector3};

use super::{Simulation, G, WAVE_SPEED};

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
    ///
    /// 包含两类效应：
    ///   1. 牛顿潮汐场（瞬时）：由当前黑洞/天体位置直接计算，无推迟
    ///      物理：在牛顿极限下引力作用瞬时，这是 PN 展开的 0 阶项
    ///   2. 引力波辐射场（推迟）：使用推迟时间 t_ret = t - r/c
    ///      物理：辐射场严格以光速 c 传播 (Einstein 1916, Blanchet 2014 §4)
    ///
    /// 网格中心跟随质心：使用 0.85/0.15 平滑系数（约 7 帧响应）
    pub(crate) fn update_grid_points(&mut self) {
        // 网格跟随黑洞质心移动（响应速率 0.15/帧，约 7 帧达到 95%）
        if let Some(com) = self.center_of_mass() {
            self.grid_center = self.grid_center * 0.85 + com * 0.15;
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
        // 引入时间平滑（0.7/0.3）：使场对天体运动的响应有轻微延迟
        // 物理意义：近似 PN 展开中的尾项 (tail, ∝ 1/c²) 造成的 hereditary 效应
        // 参考：Blanchet 2014 Living Rev. Rel. 17, 2, Eq. (219) 及后续
        let smooth_alpha = 0.3; // 新值权重
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

            for (i, &vi) in idx.iter().enumerate() {
                let new_val = sym.eigenvalues[vi];
                let new_vec: Vector3<f32> = sym.eigenvectors.column(vi).into();
                // 时间平滑：特征值用指数加权平均
                point.eigvals[i] = point.eigvals[i] * (1.0 - smooth_alpha) + new_val * smooth_alpha;
                // 特征向量平滑（注意方向一致性）
                let dot = point.eigvecs[i].dot(&new_vec);
                if dot >= 0.0 {
                    point.eigvecs[i] =
                        point.eigvecs[i] * (1.0 - smooth_alpha) + new_vec * smooth_alpha;
                } else {
                    point.eigvecs[i] =
                        point.eigvecs[i] * (1.0 - smooth_alpha) - new_vec * smooth_alpha;
                }
                let n = point.eigvecs[i].norm();
                if n > 1e-6 {
                    point.eigvecs[i] /= n;
                }
            }
        }
    }

    /// 获取 Tendex 线段渲染数据（四边形 ribbon 形式）
    /// 每个采样点生成 3 条线段，每条线段 = 2 个三角形 = 6 个顶点
    /// 线段长度 = grid_spacing * 2/3（每侧 1/3，相邻点间留 1/3 空隙）
    /// 线段强度/线宽由 |特征值| 调制，颜色由特征值符号决定
    /// three_planes_only: 仅显示 xyz 三个正交中心面上的格点
    #[allow(clippy::type_complexity)]
    pub fn get_tendex_render_data(
        &self,
        three_planes_only: bool,
    ) -> Vec<([f32; 3], [f32; 3], f32, [f32; 2], f32, f32, f32)> {
        let n = self.grid_size;
        let mid = n / 2;
        let mut vertices = Vec::with_capacity(self.grid_points.len() * 6);
        // 每侧伸出长度 = grid_spacing / 3，总长 = 2/3 * spacing
        let half_len = self.grid_spacing / 3.0;
        // 基准线宽 = grid_spacing * 0.08（与格距成正比，强度调制 0~1.5 倍）
        let base_thickness = self.grid_spacing * 0.08;
        // 强度归一化因子：特征值典型量级约为 GM/r³，用一个经验系数映射到 [0,1]
        let intensity_scale = 8.0;

        for (idx, point) in self.grid_points.iter().enumerate() {
            // 三平面模式：仅保留 x=0 / y=0 / z=0 三个中心面上的点
            if three_planes_only {
                let i = idx / (n * n);
                let rem = idx % (n * n);
                let j = rem / n;
                let k = rem % n;
                if i != mid && j != mid && k != mid {
                    continue;
                }
            }

            let world_pos = point.pos + self.grid_center;
            let center = [world_pos.x, world_pos.y, world_pos.z];

            for i in 0..3 {
                let val = point.eigvals[i];
                let dir = point.eigvecs[i];
                let line_dir = [dir.x, dir.y, dir.z];
                // color_sign: +1 = 红色（拉伸），-1 = 蓝色（压缩）
                let sign = if val >= 0.0 { 1.0 } else { -1.0 };
                // 强度：sqrt(|λ| * scale) 映射到 [0, 1]，使用 clamp 避免过亮
                let intensity = (val.abs() * intensity_scale).sqrt().min(1.0);

                if intensity < 0.02 {
                    continue;
                }

                // 6 个顶点构成 2 个三角形（四边形 ribbon）
                // corner: (沿轴方向, 垂直方向)
                // 三角形1: (-1,-1), (+1,-1), (+1,+1)
                // 三角形2: (-1,-1), (+1,+1), (-1,+1)
                let corners = [
                    [-1.0, -1.0],
                    [1.0, -1.0],
                    [1.0, 1.0],
                    [-1.0, -1.0],
                    [1.0, 1.0],
                    [-1.0, 1.0],
                ];
                for c in &corners {
                    vertices.push((
                        center,
                        line_dir,
                        half_len,
                        *c,
                        sign,
                        intensity,
                        base_thickness,
                    ));
                }
            }
        }
        vertices
    }
}
