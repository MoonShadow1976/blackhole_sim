// 生成校验模块：防止新加入的黑洞/天体与已有对象立即碰撞、被吞噬或瞬间合并
//
// 校验规则（保证添加后不会立刻发生"意外"事件）：
//   - 新黑洞 vs 已有黑洞：距离必须大于事件视界，且大于合并阈值 0.5*(rs1+rs2)
//   - 新黑洞 vs 已有天体：距离必须大于洛希极限（否则天体瞬间被撕裂）
//   - 新天体 vs 已有黑洞：距离必须大于 max(事件视界, 洛希极限)（否则瞬间被吸收/撕裂）
//   - 新天体 vs 已有天体：距离必须大于两者半径之和（否则瞬间碰撞）
//   - 数量上限
//
// UI 层每帧调用 check_* 做实时提示，点击添加时 add_* 再做一次防御性校验；
// safe_*_pos 提供"自动避让"：把位置推到最近的安全距离之外。

use nalgebra::Vector3;

use super::{BlackHole, CelestialBody, Simulation, MAX_BH, MAX_BODIES};

/// 生成失败原因（Ok 表示可安全添加）
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnError {
    /// 已达数量上限
    AtCapacity { max: usize },
    /// 距黑洞 index 过近，最小安全距离为 required
    TooCloseToBlackHole { index: usize, required: f32 },
    /// 距天体 index 过近，最小安全距离为 required
    TooCloseToBody { index: usize, required: f32 },
}

impl SpawnError {
    /// 最小安全距离（AtCapacity 时无意义，返回 0.0）
    pub fn required_distance(&self) -> f32 {
        match *self {
            SpawnError::AtCapacity { .. } => 0.0,
            SpawnError::TooCloseToBlackHole { required, .. }
            | SpawnError::TooCloseToBody { required, .. } => required,
        }
    }
}

/// 生成校验时的安全裕度：在物理临界距离（事件视界/合并阈值/洛希极限/接触距离）
/// 基础上额外放宽 20%，避免"恰好贴着临界距离生成、下一帧即触发事件"。
const SAFETY_FACTOR: f32 = 1.2;

/// 自动避让时的推出余量（在 required 之上再多留一点）
const AVOID_MARGIN: f32 = 0.05;

/// 自动避让最大迭代次数（每次迭代至少推远一个冲突对象，实际收敛很快）
const AVOID_MAX_ITER: usize = 16;

impl Simulation {
    /// 校验一个新黑洞能否安全加入
    pub fn check_black_hole_spawn(&self, bh: &BlackHole) -> Result<(), SpawnError> {
        if self.black_holes.len() >= MAX_BH {
            return Err(SpawnError::AtCapacity { max: MAX_BH });
        }
        let rs = Self::schwarzschild_radius(bh.mass);

        // 距已有黑洞：须在事件视界与合并阈值之外（含安全裕度）
        for (i, other) in self.black_holes.iter().enumerate() {
            let rs_other = Self::schwarzschild_radius(other.mass);
            let required = rs_other.max(0.5 * (rs + rs_other)) * SAFETY_FACTOR;
            if (bh.pos - other.pos).norm() < required {
                return Err(SpawnError::TooCloseToBlackHole { index: i, required });
            }
        }

        // 距已有天体：须在洛希极限之外（否则天体瞬间被撕裂）
        for (i, body) in self.bodies.iter().enumerate() {
            let required = Self::roche_limit(bh.mass, body.mass) * SAFETY_FACTOR;
            if (bh.pos - body.pos).norm() < required {
                return Err(SpawnError::TooCloseToBody { index: i, required });
            }
        }

        Ok(())
    }

    /// 校验一个新天体能否安全加入
    pub fn check_body_spawn(&self, body: &CelestialBody) -> Result<(), SpawnError> {
        if self.bodies.len() >= MAX_BODIES {
            return Err(SpawnError::AtCapacity { max: MAX_BODIES });
        }

        // 距已有黑洞：须在事件视界与洛希极限之外（否则瞬间被吸收/撕裂）
        for (i, bh) in self.black_holes.iter().enumerate() {
            let required = Self::schwarzschild_radius(bh.mass)
                .max(Self::roche_limit(bh.mass, body.mass))
                * SAFETY_FACTOR;
            if (body.pos - bh.pos).norm() < required {
                return Err(SpawnError::TooCloseToBlackHole { index: i, required });
            }
        }

        // 距已有天体：须在两者半径之和之外（否则瞬间碰撞）
        for (i, other) in self.bodies.iter().enumerate() {
            let required =
                (Self::body_radius(body.mass) + Self::body_radius(other.mass)) * SAFETY_FACTOR;
            if (body.pos - other.pos).norm() < required {
                return Err(SpawnError::TooCloseToBody { index: i, required });
            }
        }

        Ok(())
    }

    /// 校验并添加黑洞。失败时返回原因，列表保持不变。
    pub fn add_black_hole(&mut self, bh: BlackHole) -> Result<(), SpawnError> {
        self.check_black_hole_spawn(&bh)?;
        self.black_holes.push(bh);
        Ok(())
    }

    /// 校验并添加天体。失败时返回原因，列表保持不变。
    pub fn add_body(&mut self, body: CelestialBody) -> Result<(), SpawnError> {
        self.check_body_spawn(&body)?;
        self.bodies.push(body);
        Ok(())
    }

    /// 计算新黑洞的安全生成位置：
    /// 若当前位置不安全，沿最近冲突对象的径向推出到安全距离之外（速度/质量不变）。
    /// 数量已满时无法通过移动解决，原样返回。
    pub fn safe_black_hole_pos(&self, bh: &BlackHole) -> Vector3<f32> {
        if self.black_holes.len() >= MAX_BH {
            return bh.pos;
        }
        let mut candidate = bh.clone();
        for _ in 0..AVOID_MAX_ITER {
            match self.check_black_hole_spawn(&candidate) {
                Ok(()) => return candidate.pos,
                Err(SpawnError::AtCapacity { .. }) => return bh.pos,
                Err(err) => {
                    let (anchor, dist) = self.spawn_conflict(&err, candidate.pos);
                    let dir = if dist > 1e-6 {
                        (candidate.pos - anchor) / dist
                    } else {
                        Vector3::new(1.0, 0.0, 0.0)
                    };
                    let required = err.required_distance();
                    candidate.pos = anchor + dir * (required + AVOID_MARGIN);
                }
            }
        }
        candidate.pos
    }

    /// 计算新天体的安全生成位置（规则同上）
    pub fn safe_body_pos(&self, body: &CelestialBody) -> Vector3<f32> {
        if self.bodies.len() >= MAX_BODIES {
            return body.pos;
        }
        let mut candidate = body.clone();
        for _ in 0..AVOID_MAX_ITER {
            match self.check_body_spawn(&candidate) {
                Ok(()) => return candidate.pos,
                Err(SpawnError::AtCapacity { .. }) => return body.pos,
                Err(err) => {
                    let (anchor, dist) = self.spawn_conflict(&err, candidate.pos);
                    let dir = if dist > 1e-6 {
                        (candidate.pos - anchor) / dist
                    } else {
                        Vector3::new(1.0, 0.0, 0.0)
                    };
                    let required = err.required_distance();
                    candidate.pos = anchor + dir * (required + AVOID_MARGIN);
                }
            }
        }
        candidate.pos
    }

    /// 由 SpawnError 定位冲突对象的锚点位置与当前距离
    fn spawn_conflict(&self, err: &SpawnError, pos: Vector3<f32>) -> (Vector3<f32>, f32) {
        match *err {
            SpawnError::TooCloseToBlackHole { index, .. } => {
                let anchor = self.black_holes[index].pos;
                (anchor, (pos - anchor).norm())
            }
            SpawnError::TooCloseToBody { index, .. } => {
                let anchor = self.bodies[index].pos;
                (anchor, (pos - anchor).norm())
            }
            SpawnError::AtCapacity { .. } => (Vector3::zeros(), 0.0),
        }
    }
}
