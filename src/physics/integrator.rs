// 引力积分器模块：黑洞与天体共享的引力推进逻辑
// 实时模拟 (Simulation::update) 与轨迹预测 (trajectory.rs) 共用同一积分器，
// 保证预览轨迹与实际演化完全一致，且消除重复代码。

use nalgebra::Vector3;

use super::{gw_radiation_reaction, BlackHole, CelestialBody, G};

/// 推进一个时间步：
/// 1. 计算所有黑洞的加速度（两两牛顿引力 + 引力波辐射反作用力），先更新速度再更新位置
/// 2. 以更新后的黑洞位置计算所有天体的加速度
///    （黑洞引力 + 辐射反作用力 + 0.1 倍的天体间引力），更新速度与位置
///
/// 更新顺序与实时模拟一致：黑洞先行，天体后行。
pub(crate) fn step_gravity(
    bhs: &mut [BlackHole],
    bodies: &mut [CelestialBody],
    dt: f32,
) {
    // --- 黑洞 ---
    let n = bhs.len();
    if n >= 2 {
        let mut accs = vec![Vector3::zeros(); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let delta = bhs[j].pos - bhs[i].pos;
                let r = delta.norm().max(0.1);
                let dir = delta / r;
                let m1 = bhs[i].mass;
                let m2 = bhs[j].mass;

                // 牛顿引力
                let force_mag = G * m1 * m2 / (r * r);
                accs[i] += dir * force_mag / m1;
                accs[j] -= dir * force_mag / m2;

                // 引力波辐射反作用力 (Peters 1964, 2.5PN)
                let v_rel = bhs[j].vel - bhs[i].vel;
                let (a_i_rad, a_j_rad) = gw_radiation_reaction(m1, m2, r, v_rel);
                accs[i] += a_i_rad;
                accs[j] += a_j_rad;
            }
        }
        for (bh, acc) in bhs.iter_mut().zip(accs.iter()) {
            bh.vel += acc * dt;
            bh.pos += bh.vel * dt;
        }
    } else if n == 1 {
        // 单黑洞：匀速直线运动
        let v = bhs[0].vel;
        bhs[0].pos += v * dt;
    }

    // --- 天体 ---
    if bodies.is_empty() {
        return;
    }
    let n_body = bodies.len();
    let mut accs = vec![Vector3::zeros(); n_body];

    for (i, body) in bodies.iter().enumerate() {
        // 天体受所有黑洞引力 + 引力波辐射反作用力
        for bh in bhs.iter() {
            let delta = bh.pos - body.pos;
            let r = delta.norm().max(0.1);
            let dir = delta / r;
            let force_mag = G * bh.mass / (r * r);
            accs[i] += dir * force_mag;

            // 引力波辐射反作用力 (天体-黑洞对)
            let v_rel = bh.vel - body.vel;
            let (a_body_rad, _) = gw_radiation_reaction(body.mass, bh.mass, r, v_rel);
            accs[i] += a_body_rad;
        }
        // 天体之间也有微弱引力
        for (j, other) in bodies.iter().enumerate() {
            if i == j {
                continue;
            }
            let delta = other.pos - body.pos;
            let r = delta.norm().max(0.1);
            let dir = delta / r;
            let force_mag = G * other.mass / (r * r);
            accs[i] += dir * force_mag * 0.1;
        }
    }

    for (body, acc) in bodies.iter_mut().zip(accs.iter()) {
        body.vel += acc * dt;
        body.pos += body.vel * dt;
    }
}
