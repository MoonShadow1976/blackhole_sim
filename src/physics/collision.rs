// 碰撞与天体演化模块：黑洞合并、事件视界吸收、天体碰撞、洛希极限撕裂

use nalgebra::Vector3;

use super::{
    BlackHole, CelestialBody, DebrisParticle, G, MAX_BODIES, MAX_DEBRIS, Simulation,
};

impl Simulation {
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
                if r < (rs1 + rs2) * 0.9 {
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

    /// 事件视界吸收：天体越过事件视界（r < Rs）后被黑洞吞噬，不再可见
    pub(crate) fn check_event_horizon_absorption(&mut self) {
        if self.bodies.is_empty() {
            return;
        }
        self.bodies.retain(|body| {
            for bh in &self.black_holes {
                let rs = Self::schwarzschild_radius(bh.mass);
                let dist = (body.pos - bh.pos).norm();
                if dist < rs {
                    // 天体越过事件视界，被黑洞吸收
                    // 黑洞质量增加（吸积）
                    return false;
                }
            }
            true
        });
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

        for i in 0..num_particles {
            let angle = (i as f32 / num_particles as f32) * std::f32::consts::TAU;
            let r_offset = (i as f32 % 7.0) * 0.3;
            let orbit_r = (dist + r_offset).max(r_isco * 1.2);

            // 在轨道平面上分布
            let tangent = orbital_plane_normal.cross(&radial).normalize();
            let pos_offset = tangent * angle.cos() * orbit_r * 0.3
                + radial.cross(&tangent).normalize() * angle.sin() * orbit_r * 0.3;

            let particle_pos = body.pos + pos_offset;

            // 轨道速度（开普勒速度）
            let orb_v = (G * bh.mass / orbit_r).sqrt();
            let vel_dir = orbital_plane_normal
                .cross(&(bh.pos - particle_pos))
                .normalize();
            let particle_vel = body.vel * 0.3 + vel_dir * orb_v + pos_offset.normalize() * 0.1;

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
