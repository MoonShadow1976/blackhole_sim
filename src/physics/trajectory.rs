// 轨迹预测模块：克隆当前状态，向前模拟引力运动 + 引力波辐射反作用力以预览天体轨迹

use nalgebra::Vector3;

use super::{gw_radiation_reaction, BlackHole, CelestialBody, Simulation, G};

/// 轨迹数据：(黑洞轨迹, 天体轨迹)
/// 每个元素是 Vec<Vec<Vector3>>（每条轨迹按时间排序的点列）
pub type TrailData = (Vec<Vec<Vector3<f32>>>, Vec<Vec<Vector3<f32>>>);

impl Simulation {
    /// 轨迹预测：克隆当前状态，向前模拟引力运动 + 引力波辐射反作用力，每秒采样一次位置
    /// 忽略碰撞/撕裂/事件视界吸收等非线性事件，只保留引力轨道 + 辐射阻力
    /// steps: 总模拟步数，dt_per_step: 每步时间
    /// 返回 (黑洞轨迹, 天体轨迹)，每个是 Vec<Vec<Vector3>>（每条轨迹按时间排序的点列）
    pub fn predict_trajectories(&self, steps: usize, dt_per_step: f32) -> TrailData {
        // 克隆黑洞和天体
        let mut bhs: Vec<BlackHole> = self.black_holes.clone();
        let mut bodies: Vec<CelestialBody> = self.bodies.clone();

        let n_bh = bhs.len();
        let n_body = bodies.len();

        let sample_interval = (1.0 / dt_per_step).round() as usize; // 每秒采样一次
        let mut bh_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); n_bh];
        let mut body_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); n_body];

        // 记录初始位置
        for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
            trail.push(bh.pos);
        }
        for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
            trail.push(body.pos);
        }

        for step in 1..=steps {
            // --- 更新黑洞（引力 + 引力波辐射反作用力）---
            if n_bh >= 2 {
                let mut accs: Vec<Vector3<f32>> = vec![Vector3::zeros(); n_bh];
                for i in 0..n_bh {
                    for j in (i + 1)..n_bh {
                        let delta = bhs[j].pos - bhs[i].pos;
                        let r = delta.norm().max(0.1);
                        let dir = delta / r;
                        let f = G * bhs[i].mass * bhs[j].mass / (r * r);
                        accs[i] += dir * f / bhs[i].mass;
                        accs[j] -= dir * f / bhs[j].mass;

                        // 引力波辐射反作用力
                        let v_rel = bhs[j].vel - bhs[i].vel;
                        let (a_i_rad, a_j_rad) =
                            gw_radiation_reaction(bhs[i].mass, bhs[j].mass, r, v_rel);
                        accs[i] += a_i_rad;
                        accs[j] += a_j_rad;
                    }
                }
                let mut new_vels = Vec::with_capacity(n_bh);
                for i in 0..n_bh {
                    new_vels.push(bhs[i].vel + accs[i] * dt_per_step);
                }
                for i in 0..n_bh {
                    bhs[i].vel = new_vels[i];
                    let v = new_vels[i];
                    bhs[i].pos += v * dt_per_step;
                }
            } else if n_bh == 1 {
                let v = bhs[0].vel;
                bhs[0].pos += v * dt_per_step;
            }

            // --- 更新天体（受黑洞引力 + 辐射反作用力）---
            for body in &mut bodies {
                let mut acc = Vector3::zeros();
                for bh in &bhs {
                    let delta = bh.pos - body.pos;
                    let r = delta.norm().max(0.1);
                    acc += delta / r * G * bh.mass / (r * r);

                    // 引力波辐射反作用力 (天体-黑洞对)
                    let v_rel = bh.vel - body.vel;
                    let (a_body_rad, _) = gw_radiation_reaction(body.mass, bh.mass, r, v_rel);
                    acc += a_body_rad;
                }
                body.vel += acc * dt_per_step;
                let v = body.vel;
                body.pos += v * dt_per_step;
            }

            // 每秒采样一次
            if step % sample_interval == 0 {
                for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
                    trail.push(bh.pos);
                }
                for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
                    trail.push(body.pos);
                }
            }
        }

        (bh_trails, body_trails)
    }

    /// 轨迹预测（包含额外的假设天体），用于添加天体时预览
    pub fn predict_trajectories_with_body(
        &self,
        extra_body: &CelestialBody,
        steps: usize,
        dt_per_step: f32,
    ) -> TrailData {
        let mut bhs: Vec<BlackHole> = self.black_holes.clone();
        let mut bodies: Vec<CelestialBody> = self.bodies.clone();
        bodies.push(extra_body.clone());

        let n_bh = bhs.len();
        let n_body = bodies.len();

        let sample_interval = (1.0 / dt_per_step).round() as usize;
        let mut bh_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); n_bh];
        let mut body_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); n_body];

        for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
            trail.push(bh.pos);
        }
        for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
            trail.push(body.pos);
        }

        for step in 1..=steps {
            if n_bh >= 2 {
                let mut accs: Vec<Vector3<f32>> = vec![Vector3::zeros(); n_bh];
                for i in 0..n_bh {
                    for j in (i + 1)..n_bh {
                        let delta = bhs[j].pos - bhs[i].pos;
                        let r = delta.norm().max(0.1);
                        let dir = delta / r;
                        let f = G * bhs[i].mass * bhs[j].mass / (r * r);
                        accs[i] += dir * f / bhs[i].mass;
                        accs[j] -= dir * f / bhs[j].mass;

                        let v_rel = bhs[j].vel - bhs[i].vel;
                        let (a_i_rad, a_j_rad) =
                            gw_radiation_reaction(bhs[i].mass, bhs[j].mass, r, v_rel);
                        accs[i] += a_i_rad;
                        accs[j] += a_j_rad;
                    }
                }
                let mut new_vels = Vec::with_capacity(n_bh);
                for (bh, acc) in bhs.iter().zip(accs.iter()) {
                    new_vels.push(bh.vel + acc * dt_per_step);
                }
                for (bh, v) in bhs.iter_mut().zip(new_vels.iter()) {
                    bh.vel = *v;
                    bh.pos += *v * dt_per_step;
                }
            } else if n_bh == 1 {
                let v = bhs[0].vel;
                bhs[0].pos += v * dt_per_step;
            }

            for body in &mut bodies {
                let mut acc = Vector3::zeros();
                for bh in &bhs {
                    let delta = bh.pos - body.pos;
                    let r = delta.norm().max(0.1);
                    acc += delta / r * G * bh.mass / (r * r);

                    let v_rel = bh.vel - body.vel;
                    let (a_body_rad, _) = gw_radiation_reaction(body.mass, bh.mass, r, v_rel);
                    acc += a_body_rad;
                }
                body.vel += acc * dt_per_step;
                let v = body.vel;
                body.pos += v * dt_per_step;
            }

            if step % sample_interval == 0 {
                for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
                    trail.push(bh.pos);
                }
                for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
                    trail.push(body.pos);
                }
            }
        }

        (bh_trails, body_trails)
    }

    /// 轨迹预测（包含额外的假设黑洞），用于添加黑洞时预览
    pub fn predict_trajectories_with_black_hole(
        &self,
        extra_bh: &BlackHole,
        steps: usize,
        dt_per_step: f32,
    ) -> TrailData {
        let mut bhs: Vec<BlackHole> = self.black_holes.clone();
        bhs.push(extra_bh.clone());
        let mut bodies: Vec<CelestialBody> = self.bodies.clone();

        let n_bh = bhs.len();
        let n_body = bodies.len();

        let sample_interval = (1.0 / dt_per_step).round() as usize;
        let mut bh_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); n_bh];
        let mut body_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); n_body];

        for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
            trail.push(bh.pos);
        }
        for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
            trail.push(body.pos);
        }

        for step in 1..=steps {
            if n_bh >= 2 {
                let mut accs: Vec<Vector3<f32>> = vec![Vector3::zeros(); n_bh];
                for i in 0..n_bh {
                    for j in (i + 1)..n_bh {
                        let delta = bhs[j].pos - bhs[i].pos;
                        let r = delta.norm().max(0.1);
                        let dir = delta / r;
                        let f = G * bhs[i].mass * bhs[j].mass / (r * r);
                        accs[i] += dir * f / bhs[i].mass;
                        accs[j] -= dir * f / bhs[j].mass;

                        let v_rel = bhs[j].vel - bhs[i].vel;
                        let (a_i_rad, a_j_rad) =
                            gw_radiation_reaction(bhs[i].mass, bhs[j].mass, r, v_rel);
                        accs[i] += a_i_rad;
                        accs[j] += a_j_rad;
                    }
                }
                let mut new_vels = Vec::with_capacity(n_bh);
                for (bh, acc) in bhs.iter().zip(accs.iter()) {
                    new_vels.push(bh.vel + acc * dt_per_step);
                }
                for (bh, v) in bhs.iter_mut().zip(new_vels.iter()) {
                    bh.vel = *v;
                    bh.pos += *v * dt_per_step;
                }
            } else if n_bh == 1 {
                let v = bhs[0].vel;
                bhs[0].pos += v * dt_per_step;
            }

            for body in &mut bodies {
                let mut acc = Vector3::zeros();
                for bh in &bhs {
                    let delta = bh.pos - body.pos;
                    let r = delta.norm().max(0.1);
                    acc += delta / r * G * bh.mass / (r * r);

                    let v_rel = bh.vel - body.vel;
                    let (a_body_rad, _) = gw_radiation_reaction(body.mass, bh.mass, r, v_rel);
                    acc += a_body_rad;
                }
                body.vel += acc * dt_per_step;
                let v = body.vel;
                body.pos += v * dt_per_step;
            }

            if step % sample_interval == 0 {
                for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
                    trail.push(bh.pos);
                }
                for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
                    trail.push(body.pos);
                }
            }
        }

        (bh_trails, body_trails)
    }
}
