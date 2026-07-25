# 黑洞模拟器 🌌

一个基于 Rust 和 WGPU 的实时 3D 黑洞模拟器，包含引力波可视化功能。

![黑洞模拟](https://trae-api-cn.mchost.guru/api/ide/v1/text_to_image?prompt=black%20hole%20simulation%20with%20gravitational%20waves%20warping%20spacetime%20grid%20in%20dark%20space&image_size=landscape_16_9)

## 功能特性

- **实时 N 体模拟**：牛顿引力和相对论效应
- **Schwarzschild 时空扭曲可视化**：使用三维正交网格展示时空弯曲，黑洞和天体均产生扭曲
- **双黑洞旋进引力波发射**：基于四极辐射公式模拟 plus 和 cross 双极化引力波
- **事件视界物理**：穿过视界的物体被吸收
- **洛希极限撕裂**：潮汐力撕裂天体
- **轨迹预测**：可视化黑洞和天体的未来路径
- **实例化渲染**：高效的轨迹可视化

## 物理理论

### 1. Tendex 线：时空曲率可视化

模拟器采用 **Tendex 线**方法可视化时空曲率，基于 Owen et al. (2011, arXiv:1012.4869) 和 Nichols 的可视化框架。

曲率张量的"电"部分 \( E_{jk} \) 描述潮汐拉伸/压缩效应。在牛顿极限下：

\[ E_{jk} = \sum_i \frac{GM_i}{r_i^3} \left( 3 n_j n_k - \delta_{jk} \right) \]

其中 \( \mathbf{n} = \mathbf{r}/r \) 是从质量源指向场点的单位向量。

在每个空间采样点，计算 \( E_{jk} \) 的三个特征值和特征向量：
- **红色线**：正特征值方向（潮汐**拉伸**）
- **蓝色线**：负特征值方向（潮汐**压缩**）
- 线段长度按 \( \sqrt{|\lambda|} \) 缩放

由于 \( E_{jk} \) 是无迹的（\( \text{tr}(E) = 0 \)），三个特征值之和为零，拉伸和压缩方向互补。

### 2. 引力波曲率振荡

双黑洞系统的引力波在远场产生动态曲率振荡。基于线性化引力，TT 规范下的度规微扰 \( h_{jk}^{\text{TT}} \) 对潮汐张量的贡献为：

\[ E_{jk}^{\text{GW}} = \frac{1}{2} \omega^2 h_{jk}^{\text{TT}} \]

引力波应变振幅基于四极辐射公式：

\[ h_0 = \frac{4 M_c^{5/3} \omega^{2/3}}{D} \]

其中 \( M_c = \frac{(m_1 m_2)^{3/5}}{(m_1+m_2)^{1/5}} \) 是 Chirp 质量，\( \omega \) 是引力波角频率（= 2 × 轨道角频率），\( D \) 是到源的距离。

### 3. 引力波双极化模式

模拟器实现了 plus (+) 和 cross (×) 两种极化模式，基于观测者相对于轨道平面的倾角 ι：

\[ h_+ = h_0 \cdot \frac{1 + \cos^2\iota}{2} \cdot \cos(\Phi) \]
\[ h_\times = h_0 \cdot \cos\iota \cdot \sin(\Phi) \]

在 TT 规范下，考虑极化角 ψ 的旋转效应，组合为完整应变张量：

\[ h_{\theta\theta} = h_+ \cos 2\psi - h_\times \sin 2\psi \]
\[ h_{\phi\phi} = -h_+ \cos 2\psi - h_\times \sin 2\psi \]
\[ h_{\theta\phi} = h_+ \sin 2\psi + h_\times \cos 2\psi \]

### 4. 线性化引力与叠加原理

基于线性化引力理论，度规分解为平直背景加微扰：

\[ g_{\mu\nu} = \eta_{\mu\nu} + h_{\mu\nu}, \quad |h_{\mu\nu}| \ll 1 \]

在弱场极限下，多个源的曲率张量**线性叠加**：

\[ E_{jk}^{\text{total}} = \sum_{i=1}^{N_{\text{bodies}}} E_{jk}^{(i)} + E_{jk}^{\text{GW}} \]

所有质量源（黑洞和普通天体）均产生潮汐效应，与引力波动态振荡叠加。

## 代码结构

```
src/
├── main.rs              # 应用入口，事件循环，ApplicationHandler
├── ui.rs                # egui 面板，坐标轴 gizmo
├── camera.rs            # 相机控制和透视投影
├── geometry.rs          # 球体/圆环几何体生成
├── physics/
│   ├── mod.rs           # Simulation 结构体，update，公共 API
│   ├── grid.rs          # Tendex 线：潮汐张量计算和特征分解
│   ├── collision.rs     # 黑洞合并，视界吸收，洛希撕裂
│   └── trajectory.rs    # 轨迹预测
└── renderer/
    ├── mod.rs           # Renderer 结构体，render 方法
    ├── types.rs         # 顶点/Uniform 结构体，常量
    ├── shaders.rs       # WGSL 着色器源码
    └── pipeline.rs      # 渲染管线和缓冲区创建
```

### 关键模块

- **[physics/grid.rs](src/physics/grid.rs)** - Tendex 线时空曲率可视化：
  - `TendexPoint` - 存储采样点位置和曲率张量特征分解
  - `compute_tidal_tensor()` - 计算潮汐张量 E_jk（含引力波贡献）
  - `update_grid_points()` - 对所有采样点进行特征分解
  - `get_tendex_render_data()` - 生成红蓝线段顶点数据

- **[renderer/](src/renderer/)** - WGPU 渲染：
  - Tendex 线段管线（LineList 拓扑，半透明混合）
  - 专用背景渲染管线（不写深度，跟随相机）
  - 轨迹点的实例化渲染
  - 方形/三角形标记的 SDF

## 构建和运行

```bash
# 优化构建
cargo build --release

# 运行
cargo run --release
```

## 控制

- **鼠标**：旋转相机
- **滚轮**：缩放
- **UI 面板**：添加黑洞/天体，调整参数，切换可视化选项

## 参考文献

1. Owen, R., et al. (2011). "Frame-Dragging Vortexes and Tidal Tendexes Attached to Colliding Black Holes: Visualizing the Curvature of Spacetime." Physical Review Letters, 106, 151101. arXiv:1012.4869.

2. Nichols, D. A. "Visualizations of Spacetime Curvature." https://dnichols1.github.io/visualizations/

3. Abbott, B. P., et al. (2016). "Observation of Gravitational Waves from a Binary Black Hole Merger." Physical Review Letters, 116(6), 061102.

4. Maggiore, M. (2008). "Gravitational Waves. Volume 1: Theory and Experiments." Oxford University Press.

5. Misner, C. W., Thorne, K. S., & Wheeler, J. A. (1973). "Gravitation." W. H. Freeman.

6. Hamilton, A. J. S., & Lisle, J. P. (2008). "The river model of black holes." American Journal of Physics, 76(6), 519-532. arXiv:gr-qc/0411060.

7. Weinberg, S. (1972). "Gravitation and Cosmology: Principles and Applications of the General Theory of Relativity." Wiley.

8. Poisson, E., & Will, C. M. (2014). "Gravity: Newtonian, Post-Newtonian, Relativistic." Cambridge University Press.

## 许可证

MIT 许可证

---

[English Version](README.md)