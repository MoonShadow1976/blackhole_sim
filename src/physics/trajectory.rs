// 轨迹预测模块：克隆当前状态，向前模拟引力运动 + 引力波辐射反作用力以预览天体轨迹
// 与实时模拟共用 integrator::step_gravity，保证预览轨迹与实际演化一致。
// 忽略碰撞/撕裂/事件视界吸收等非线性事件，只保留引力轨道 + 辐射阻力。

use nalgebra::Vector3;

use super::{integrator, BlackHole, CelestialBody, Simulation};

/// 轨迹数据：(黑洞轨迹, 天体轨迹)
/// 每个元素是 Vec<Vec<Vector3>>（每条轨迹按时间排序的点列）
pub type TrailData = (Vec<Vec<Vector3<f32>>>, Vec<Vec<Vector3<f32>>>);

impl Simulation {
    /// 轨迹预测：克隆当前状态，向前模拟引力运动 + 引力波辐射反作用力，每秒采样一次位置
    /// steps: 总模拟步数，dt_per_step: 每步时间
    /// 返回 (黑洞轨迹, 天体轨迹)，每个是 Vec<Vec<Vector3>>（每条轨迹按时间排序的点列）
    pub fn predict_trajectories(&self, steps: usize, dt_per_step: f32) -> TrailData {
        self.predict_with(None, None, steps, dt_per_step)
    }

    /// 轨迹预测（包含额外的假设天体），用于添加天体时预览。
    /// 额外天体的轨迹位于返回值的第二条列表末尾。
    pub fn predict_trajectories_with_body(
        &self,
        extra_body: &CelestialBody,
        steps: usize,
        dt_per_step: f32,
    ) -> TrailData {
        self.predict_with(None, Some(extra_body), steps, dt_per_step)
    }

    /// 轨迹预测（包含额外的假设黑洞），用于添加黑洞时预览。
    /// 额外黑洞的轨迹位于返回值的第一条列表末尾。
    pub fn predict_trajectories_with_black_hole(
        &self,
        extra_bh: &BlackHole,
        steps: usize,
        dt_per_step: f32,
    ) -> TrailData {
        self.predict_with(Some(extra_bh), None, steps, dt_per_step)
    }

    /// 统一轨迹预测核心：克隆当前状态，可追加一个假设黑洞/天体，逐步积分并采样
    fn predict_with(
        &self,
        extra_bh: Option<&BlackHole>,
        extra_body: Option<&CelestialBody>,
        steps: usize,
        dt_per_step: f32,
    ) -> TrailData {
        let mut bhs: Vec<BlackHole> = self.black_holes.clone();
        if let Some(bh) = extra_bh {
            bhs.push(bh.clone());
        }
        let mut bodies: Vec<CelestialBody> = self.bodies.clone();
        if let Some(body) = extra_body {
            bodies.push(body.clone());
        }

        let sample_interval = (1.0 / dt_per_step).round() as usize; // 每秒采样一次
        let mut bh_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); bhs.len()];
        let mut body_trails: Vec<Vec<Vector3<f32>>> = vec![Vec::new(); bodies.len()];

        // 记录初始位置
        for (trail, bh) in bh_trails.iter_mut().zip(bhs.iter()) {
            trail.push(bh.pos);
        }
        for (trail, body) in body_trails.iter_mut().zip(bodies.iter()) {
            trail.push(body.pos);
        }

        for step in 1..=steps {
            // 与实时模拟相同的积分器
            integrator::step_gravity(&mut bhs, &mut bodies, dt_per_step);

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
}
