// 碰撞与天体演化模块：黑洞合并、事件视界吸收、天体碰撞、洛希极限撕裂

use nalgebra::Vector3;

use super::{BlackHole, CelestialBody, DebrisParticle, Simulation, G, MAX_BODIES, MAX_DEBRIS};

impl Simulation {
    /// 黑洞合并检测
    ///
    /// 物理依据：双黑洞旋进 (inspiral) 在 ISCO (Innermost Stable Circular Orbit) 处结束，
    /// 进入快速 plunge 阶段，最终在视界接触附近形成公共视界并合并。
    ///
    /// ISCO 判据 (Blanchet & Iyer 2003, 3PN)：
    ///   r_ISCO ≈ 6 G(m1+m2)/c² = 3 (rs1+rs2)  (试验粒子极限)
    /// 等质量时数值相对论给出 r_ISCO ≈ 5M (Buonanno, Cook & Pretorius 2007)。
    ///
    /// 本模拟器采用简化模型：
    ///   1. 当 r < r_ISCO 时进入 plunge 阶段（辐射反作用力增强，见 gw_radiation_reaction）
    ///   2. 当 r < 0.5*(rs1+rs2)（视界显著重叠）时触发合并
    ///
    /// 视觉效果：两个黑洞会相互穿透、视觉重叠一段时间后再合并，体现质心点状特性。
    /// 在真实 GR 中，公共视界在视界接触前已形成；本模拟为可视化目的延迟合并触发。
    pub(crate) fn check_mergers(&mut self) {
        let n = self.black_holes.len();
        if n < 2 {
            return;
        }

        let mut to_remove = Vec::new();
        let mut to_add: Option<BlackHole> = None;

        'outer: for i in 0..n {
            for j in (i + 1)..n {
                if to_remove.contains(&i) || to_remove.contains(&j) {
                    continue;
                }
                let delta = self.black_holes[j].pos - self.black_holes[i].pos;
                let r = delta.norm();
                let rs1 = Self::schwarzschild_radius(self.black_holes[i].mass);
                let rs2 = Self::schwarzschild_radius(self.black_holes[j].mass);
                // 合并条件：视界重叠 50%（对应两奇点距离约 0.5*(rs1+rs2)）
                // 物理上 ISCO ≈ 3*(rs1+rs2) 是 inspiral 终点，此处继续 plunge 至视界重叠
                let merge_threshold = 0.5 * (rs1 + rs2);
                if r < merge_threshold {
                    let m1 = self.black_holes[i].mass;
                    let m2 = self.black_holes[j].mass;
                    let total_mass = m1 + m2;
                    let total_momentum =
                        m1 * self.black_holes[i].vel + m2 * self.black_holes[j].vel;
                    let new_vel = total_momentum / total_mass;
                    let new_pos =
                        (m1 * self.black_holes[i].pos + m2 * self.black_holes[j].pos) / total_mass;

                    // 合并产生的引力波效应已由 update_grid_points 中的连续波模型覆盖

                    to_remove.push(i);
                    to_remove.push(j);
                    to_add = Some(BlackHole {
                        // 5% 质量亏损以引力波形式辐射 (Peters 公式预测值)
                        mass: total_mass * 0.95,
                        pos: new_pos,
                        vel: new_vel,
                    });
                    break 'outer;
                }
            }
        }

        if !to_remove.is_empty() {
            to_remove.sort_by(|a, b| b.cmp(a));
            for idx in to_remove {
                self.black_holes.remove(idx);
            }
            if let Some(bh) = to_add {
                self.black_holes.push(bh);
            }
        }
    }

    /// 事件视界吸收：天体越过事件视界 (r < Rs) 后被黑洞吞噬
    ///
    /// 物理依据 (Rees 1988, Hills 1975)：
    ///   潮汐撕裂半径 d_Roche = R_body * (2*M_bh/M_body)^(1/3)
    ///   事件视界半径 rs = 2*M_bh
    ///
    ///   若 d_Roche > rs（天体平均密度 < 黑洞视界内平均密度）：
    ///     天体在视界外被潮汐力撕裂 → 走 Roche 撕裂路径（不在此处处理）
    ///   若 d_Roche < rs（天体密度大，如中子星、白矮星）：
    ///     天体越过视界后整体被吞噬 → 直接吸收
    ///
    /// Hills 质量 M_H ≈ 1.08×10⁸ M_⊙（太阳型恒星，Schwarzschild）
    /// 当 M_bh < M_H 时太阳型恒星在视界外撕裂；M_bh > M_H 时直接吸收。
    ///
    /// 本函数仅处理"直接吸收"情形。Roche 撕裂由 check_roche_disruption 处理
    /// （在 update() 中先于本函数调用）。
    /// 若大时间步导致天体跳过 Roche 阶段直接进入视界，本函数会判断其
    /// 是否本应被撕裂，若是则走 Roche 路径而非直接吸收。
    pub(crate) fn check_event_horizon_absorption(&mut self) {
        if self.bodies.is_empty() {
            return;
        }

        // 收集需要走 Roche 撕裂路径的天体（避免在 retain 中调用 disrupt_body）
        let mut to_disrupt: Vec<(usize, usize)> = Vec::new();

        // 第一遍：标记直接吸收的天体，并找出应走 Roche 路径的
        let mut to_absorb: Vec<usize> = Vec::new();
        for (bi, body) in self.bodies.iter().enumerate() {
            for (bhi, bh) in self.black_holes.iter().enumerate() {
                let rs = Self::schwarzschild_radius(bh.mass);
                let dist = (body.pos - bh.pos).norm();
                if dist < rs {
                    // 越过视界：判断是否本应在视界外被 Roche 撕裂
                    let d_roche = Self::roche_limit(bh.mass, body.mass);
                    if d_roche > rs {
                        // 大天体 / 低密度天体：走 Roche 撕裂
                        to_disrupt.push((bi, bhi));
                    } else {
                        // 致密天体：直接吸收
                        to_absorb.push(bi);
                    }
                    break; // 一个黑洞处理即可
                }
            }
        }

        // 处理 Roche 撕裂（从后往前删除避免索引错位）
        to_disrupt.sort_by(|a, b| b.0.cmp(&a.0));
        for (bi, bhi) in to_disrupt.iter().rev() {
            let body = self.bodies.remove(*bi);
            let bh = self.black_holes[*bhi].clone();
            self.disrupt_body(&body, &bh);
        }

        // 处理直接吸收
        if !to_absorb.is_empty() {
            to_absorb.sort_by(|a, b| b.cmp(a));
            for bi in to_absorb {
                if bi < self.bodies.len() {
                    self.bodies.remove(bi);
                }
            }
        }
    }

    /// 天体间碰撞：基于 Q*_D 标度律（Holsapple 1994, Love & Ahrens 1996）
    /// Q*_D = Q_S * D^(-0.24) + Q_G * D^1.13 （强度区 + 引力区）
    /// 碎片数量取决于撞击能量与碎裂阈值之比，以及材质硬度
    pub(crate) fn check_body_collisions(&mut self) {
        if self.bodies.len() < 2 {
            return;
        }

        let mut to_remove: Vec<usize> = Vec::new();
        let mut new_debris: Vec<DebrisParticle> = Vec::new();
        let mut merged_bodies: Vec<CelestialBody> = Vec::new();

        for i in 0..self.bodies.len() {
            if to_remove.contains(&i) {
                continue;
            }
            for j in (i + 1)..self.bodies.len() {
                if to_remove.contains(&j) {
                    continue;
                }

                let delta = self.bodies[j].pos - self.bodies[i].pos;
                let dist = delta.norm();
                let r1 = Self::body_radius(self.bodies[i].mass);
                let r2 = Self::body_radius(self.bodies[j].mass);

                if dist < r1 + r2 {
                    // 碰撞发生
                    let m1 = self.bodies[i].mass;
                    let m2 = self.bodies[j].mass;
                    let m_tot = m1 + m2;
                    let mu = m1 * m2 / m_tot; // 约化质量

                    let v_rel_vec = self.bodies[j].vel - self.bodies[i].vel;
                    let v_rel = v_rel_vec.norm();

                    // 比撞击能 Q_R (Leinhardt & Stewart 2012)
                    let q = 0.5 * mu * v_rel * v_rel / m_tot;

                    // 碎裂阈值 Q*_D（Holsapple 标度律）
                    let hardness = (self.bodies[i].hardness + self.bodies[j].hardness) * 0.5;
                    let q_star = Self::critical_disruption_energy(m_tot, hardness);

                    let collision_pos = (m1 * self.bodies[i].pos + m2 * self.bodies[j].pos) / m_tot;
                    let collision_vel = (m1 * self.bodies[i].vel + m2 * self.bodies[j].vel) / m_tot;

                    if q < q_star * 0.5 {
                        // 撞击能量不足：合并为更大的天体（吸积）
                        merged_bodies.push(CelestialBody {
                            mass: m_tot * 0.95,
                            pos: collision_pos,
                            vel: collision_vel,
                            hardness,
                        });
                    } else {
                        // 碎裂：产生碎片
                        let ratio = q / q_star;
                        // 碎片数量：与能量比和硬度成正比
                        let num_fragments = ((ratio * 15.0 * hardness) as usize).clamp(8, 80);

                        // 碰撞法线方向
                        let normal = if dist > 0.01 {
                            delta / dist
                        } else {
                            Vector3::new(0.0, 1.0, 0.0)
                        };

                        // 构造垂直于法线的两个切向方向
                        let tangent1 = if normal.y.abs() < 0.9 {
                            normal.cross(&Vector3::new(0.0, 1.0, 0.0)).normalize()
                        } else {
                            normal.cross(&Vector3::new(1.0, 0.0, 0.0)).normalize()
                        };
                        let tangent2 = normal.cross(&tangent1).normalize();

                        for k in 0..num_fragments {
                            let angle = (k as f32 / num_fragments as f32) * std::f32::consts::TAU;
                            let pitch = ((k as f32 % 5.0) - 2.0) * 0.3;
                            let spread = 0.5 + (k as f32 % 7.0) * 0.15;

                            let dir = (normal * pitch.sin()
                                + tangent1 * angle.cos() * pitch.cos()
                                + tangent2 * angle.sin() * pitch.cos())
                            .normalize();

                            // 碎片速度：碰撞速度 + 随机散布
                            let speed = v_rel * 0.3 + spread * (q / q_star).sqrt() * 0.5;
                            let frag_vel = collision_vel + dir * speed;

                            // 碎片位置：在碰撞点附近散布
                            let frag_pos = collision_pos + dir * (r1 + r2) * 0.5;

                            // 碎片质量：从总质量中分配
                            let frag_mass = m_tot / num_fragments as f32;

                            new_debris.push(DebrisParticle {
                                pos: frag_pos,
                                vel: frag_vel,
                                life: 0.0,
                                mass: frag_mass,
                            });
                        }
                    }

                    to_remove.push(i);
                    to_remove.push(j);
                    break;
                }
            }
        }

        if !to_remove.is_empty() {
            to_remove.sort_by(|a, b| b.cmp(a));
            for idx in to_remove {
                self.bodies.remove(idx);
            }
            // 添加合并后的天体
            for body in merged_bodies {
                if self.bodies.len() < MAX_BODIES {
                    self.bodies.push(body);
                }
            }
            // 添加碎片
            for debris in new_debris {
                if self.debris.len() < MAX_DEBRIS {
                    self.debris.push(debris);
                }
            }
        }
    }

    /// 碎裂阈值 Q*_D（基于 Holsapple 双幂律标度律）
    /// Q*_D = Q_S * D^(-0.24) + Q_G * D^1.13
    /// Q_S：强度区参数（与小天体材质强度相关）
    /// Q_G：引力区参数（与大天体引力结合能相关）
    fn critical_disruption_energy(mass: f32, hardness: f32) -> f32 {
        let radius = Self::body_radius(mass);
        let diameter = radius * 2.0;
        // 强度区：硬度越高，Q_S 越大（更难碎裂）
        let q_s = 50.0 * hardness;
        // 引力区：质量越大，引力结合能越高
        let q_g = 0.3;
        q_s * diameter.powf(-0.24) + q_g * diameter.powf(1.13)
    }

    pub(crate) fn check_roche_disruption(&mut self) {
        if self.bodies.is_empty() {
            return;
        }

        let mut to_disrupt: Vec<(usize, usize, Vector3<f32>)> = Vec::new();

        for (bi, body) in self.bodies.iter().enumerate() {
            for (bhi, bh) in self.black_holes.iter().enumerate() {
                let dist = (body.pos - bh.pos).norm();
                let roche = Self::roche_limit(bh.mass, body.mass);
                if dist < roche {
                    to_disrupt.push((bi, bhi, bh.pos));
                    break;
                }
            }
        }

        // 从后往前删除，避免索引错位
        for (bi, bhi, _bh_pos) in to_disrupt.iter().rev() {
            let body = self.bodies.remove(*bi);
            // 克隆 BlackHole 以避免与 &mut self 的借用冲突
            let bh = self.black_holes[*bhi].clone();
            self.disrupt_body(&body, &bh);
        }
    }

    fn disrupt_body(&mut self, body: &CelestialBody, bh: &BlackHole) {
        let dist = (body.pos - bh.pos).norm().max(0.5);
        let to_bh = bh.pos - body.pos;
        let radial = to_bh / dist;

        // 轨道平面法线（垂直于速度和径向方向）
        let orbital_plane_normal = {
            let v = body.vel;
            let n = radial.cross(&v);
            if n.norm() < 0.01 {
                Vector3::new(0.0, 1.0, 0.0)
            } else {
                n.normalize()
            }
        };

        let num_particles = 60;
        let rs = Self::schwarzschild_radius(bh.mass);
        let r_isco = 3.0 * rs;
        // 每个碎片分得的质量
        let particle_mass = body.mass / num_particles as f32;

        // 轨道半径基准：确保在 ISCO 之外
        // 若天体已在视界内（被 check_event_horizon_absorption 转来），
        // 则用 r_isco * 1.5 作为基准轨道半径，碎片分布在以黑洞为中心的环上
        let base_orbit_r = dist.max(r_isco * 1.5);

        for i in 0..num_particles {
            let angle = (i as f32 / num_particles as f32) * std::f32::consts::TAU;
            let r_offset = (i as f32 % 7.0 - 3.0) * 0.2; // -0.6 ~ +0.6
            let orbit_r = base_orbit_r + r_offset;

            // 在轨道平面上分布：以黑洞为中心的圆形轨道
            // tangent 为轨道切向，orbit_r * radial_cross_tangent 为径向偏移方向
            let tangent = orbital_plane_normal.cross(&radial).normalize();
            let radial_perp = radial.cross(&tangent).normalize();
            // 碎片在轨道平面上的位置（以黑洞为参考，距离 = orbit_r，角度 = angle）
            let pos_in_plane = radial * angle.cos() * orbit_r + radial_perp * angle.sin() * orbit_r;
            let particle_pos = bh.pos - pos_in_plane; // 从黑洞指向碎片

            // 轨道速度（开普勒速度），切向
            let orb_v = (G * bh.mass / orbit_r).sqrt();
            let vel_dir = orbital_plane_normal
                .cross(&(particle_pos - bh.pos))
                .normalize();
            let particle_vel = bh.vel * 0.2 + vel_dir * orb_v;

            if self.debris.len() < MAX_DEBRIS {
                self.debris.push(DebrisParticle {
                    pos: particle_pos,
                    vel: particle_vel,
                    life: 0.0,
                    mass: particle_mass,
                });
            }
        }
    }
}
