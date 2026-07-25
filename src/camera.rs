// 轨道相机模块 - 使用四元数实现平滑的轨道旋转
// 支持鼠标拖拽旋转、滚轮缩放、WASD 平移、方向键旋转

use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector3};

/// 轨道相机：围绕目标点旋转
pub struct OrbitCamera {
    /// 相机注视的目标点
    pub target: Vector3<f32>,
    /// 相机到目标的距离
    pub distance: f32,
    /// 相机的朝向（用四元数表示，从相机到目标的方向）
    pub orientation: UnitQuaternion<f32>,
}

impl OrbitCamera {
    /// 创建新相机，初始距离 15，方位角 -45°，俯仰角 -30°
    pub fn new() -> Self {
        // 初始 yaw = -45°, pitch = -30°
        let yaw: f32 = -45.0_f32.to_radians();
        let pitch: f32 = -30.0_f32.to_radians();

        // yaw 绕世界 Y 轴旋转
        let q_yaw = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), yaw);
        // pitch 绕局部 X 轴旋转
        let q_pitch = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), pitch);

        // 组合：先 yaw（左乘）再 pitch（右乘）
        let orientation = q_yaw * q_pitch;

        Self {
            target: Vector3::new(0.0, 0.0, 0.0),
            distance: 15.0,
            orientation,
        }
    }

    /// 获取相机在世界空间中的位置
    pub fn position(&self) -> Vector3<f32> {
        // 相机位置 = target + orientation * [0, 0, distance]
        // 这里 orientation 把局部 Z 方向变换到世界方向
        let offset = self.orientation * Vector3::new(0.0, 0.0, self.distance);
        self.target + offset
    }

    /// 计算视图矩阵 (view matrix)
    pub fn view_matrix(&self) -> Matrix4<f32> {
        let eye = self.position();
        // 上方向经过相同的朝向变换
        let up = self.orientation * Vector3::new(0.0, 1.0, 0.0);
        // look_at_rh 需要 Point3 类型
        let eye_point = Point3::from(eye);
        let target_point = Point3::from(self.target);
        Matrix4::look_at_rh(&eye_point, &target_point, &up)
    }

    /// 绕世界 Y 轴旋转（左乘）
    pub fn rotate_yaw(&mut self, angle: f32) {
        let q_yaw = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle);
        self.orientation = q_yaw * self.orientation;
    }

    /// 绕局部 X 轴旋转（右乘）
    pub fn rotate_pitch(&mut self, angle: f32) {
        let q_pitch = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), angle);
        self.orientation *= q_pitch;
    }

    /// 平移目标点（在相机的局部坐标系中）
    pub fn pan(&mut self, delta: Vector3<f32>) {
        // 将局部平移变换到世界空间
        let world_delta = self.orientation * delta;
        self.target += world_delta;
    }

    /// 缩放距离（无限制）
    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance + amount).max(0.1);
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new()
    }
}
