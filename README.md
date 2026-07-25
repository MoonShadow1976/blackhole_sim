# Black Hole Simulator 🌌

A real-time 3D black hole simulation with gravitational wave visualization, written in Rust using WGPU.

![Black Hole Simulation](https://trae-api-cn.mchost.guru/api/ide/v1/text_to_image?prompt=black%20hole%20simulation%20with%20gravitational%20waves%20warping%20spacetime%20grid%20in%20dark%20space&image_size=landscape_16_9)

## Features

- **Real-time N-body simulation** with Newtonian gravity and relativistic effects
- **Schwarzschild spacetime distortion** visualization using 3D orthogonal grid; both black holes and celestial bodies produce distortion
- **Gravitational wave emission** from binary black hole inspirals with plus and cross polarizations
- **Event horizon physics** - objects crossing the horizon are absorbed
- **Roche limit disruption** - tidal forces tearing apart celestial bodies
- **Trajectory prediction** - visualize future paths of black holes and bodies
- **Instanced rendering** for efficient trail visualization

## Physics Theory

### 1. Schwarzschild Metric & Spacetime Distortion

The simulation uses the Schwarzschild metric to calculate spacetime distortion around black holes and celestial bodies:

\[ ds^2 = -\left(1 - \frac{r_s}{r}\right)dt^2 + \frac{1}{1 - \frac{r_s}{r}}dr^2 + r^2 d\Omega \]

where \( r_s = 2GM \) is the Schwarzschild radius.

**Exterior** (r > r_s): The radial stretch factor is:
\[ \text{stretch} = \frac{1}{\sqrt{1 - \frac{r_s}{r}}} \]

Grid points are displaced toward the source, with increasing distortion as you approach the event horizon.

**Interior** (r < r_s): The **river model** (Hamilton & Lisle 2004) is used, based on Gullstrand-Painlevé coordinates. Space itself flows inward at the escape velocity:
\[ v = \sqrt{\frac{r_s}{r}} \]

At the horizon v = 1 (speed of light), and v → ∞ as r → 0, sweeping everything toward the singularity. This replaces the previous logarithmic approximation with a physically rigorous description of spacetime behavior inside the horizon.

All mass sources (black holes and celestial bodies) produce this effect, superposed via linearized gravity.

### 2. Gravitational Wave Amplitude

For binary black hole systems, the gravitational wave strain amplitude is based on the quadrupole radiation formula (Newtonian order):

\[ h_0 = \frac{4 G^{5/3}}{c^4} \frac{M_c^{5/3} \omega^{2/3}}{D} \]

In natural units (G = c = 1), this simplifies to:

\[ h_0 = \frac{4 M_c^{5/3} \omega^{2/3}}{D} \]

where \( \omega \) is the gravitational wave angular frequency (= 2 × orbital angular frequency) and \( D \) is the distance from the source.

### 3. Chirp Mass

The chirp mass, a key parameter for gravitational wave detection:

\[ M_c = \frac{(m_1 m_2)^{3/5}}{(m_1 + m_2)^{1/5}} \]

The wave amplitude is proportional to \( M_c^{5/3} \), meaning more massive mergers produce stronger gravitational waves.

### 4. Gravitational Wave Polarizations

The simulation implements both plus (+) and cross (×) polarization modes, based on the inclination angle ι of the observer relative to the orbital plane:

\[ h_+ = h_0 \cdot \frac{1 + \cos^2\iota}{2} \cdot \cos(\Phi) \]
\[ h_\times = h_0 \cdot \cos\iota \cdot \sin(\Phi) \]

where Φ is the gravitational wave phase (including orbital phase and time evolution) and ι is the angle between the line of sight and the orbital normal.

### 5. TT-Gauge Strain Tensor Displacement

In the transverse-traceless (TT) gauge, the coordinate displacement caused by the gravitational wave strain tensor is:

\[ \xi_i = \frac{1}{2} \sum_j h_{ij}^{\text{TT}} \, x_j \]

The simulation constructs polarization basis vectors (e_θ, e_φ), accounts for the polarization angle ψ rotation, and combines plus and cross polarizations into the full strain tensor:

\[ h_{\theta\theta} = h_+ \cos 2\psi - h_\times \sin 2\psi \]
\[ h_{\phi\phi} = -h_+ \cos 2\psi - h_\times \sin 2\psi \]
\[ h_{\theta\phi} = h_+ \sin 2\psi + h_\times \cos 2\psi \]

The transverse displacement of each grid point relative to the center of mass is then computed, achieving physically accurate gravitational wave spacetime distortion visualization.

### 6. Linearized Gravity & Superposition

Based on linearized gravity theory, the full metric is decomposed into a flat background plus a small perturbation:

\[ g_{\mu\nu} = \eta_{\mu\nu} + h_{\mu\nu}, \quad |h_{\mu\nu}| \ll 1 \]

In the weak-field regime, perturbations from multiple sources **superpose linearly**. Each mass source's (black hole or celestial body) spacetime distortion is computed independently and summed:

\[ \vec{d}_{\text{total}} = \sum_{i=1}^{N} \vec{d}_{r,i} + \vec{d}_{\text{GW}} \]

This approach is valid in the weak-field limit where \( r \gg r_s \) and is consistent with the post-Newtonian approximation used in gravitational wave modeling.

## Code Structure

```
src/
├── main.rs          # Application entry point, UI, and main loop
├── renderer.rs      # WGPU rendering pipeline, shaders, instanced rendering
├── physics.rs       # N-body simulation, gravity, gravitational waves
└── camera.rs        # Camera control and perspective projection
```

### Key Files

- **[physics.rs](src/physics.rs)** - Contains the `Simulation` struct with:
  - `GridPoint` - Stores original and deformed positions for spacetime grid
  - `GravityWave` - Spherical wave propagating outward
  - `spacetime_distortion()` - Radial displacement based on Schwarzschild metric
  - `update_grid_points()` - Applies all mass sources' distortion and GW effects
  - `emit_inspiral_waves()` - Generates waves during binary black hole inspiral

- **[renderer.rs](src/renderer.rs)** - Implements:
  - Grid vertex/fragment shaders for spacetime visualization
  - Dedicated background pipeline (no depth write, follows camera)
  - Instanced rendering for trajectory points
  - Shape SDFs (Signed Distance Functions) for square/triangle markers

## Build and Run

```bash
# Build with optimizations
cargo build --release

# Run
cargo run --release
```

## Controls

- **Mouse**: Rotate camera
- **Scroll**: Zoom in/out
- **UI Panel**: Add black holes/bodies, adjust parameters, toggle visualization options

## References

1. Abbott, B. P., et al. (2016). "Observation of Gravitational Waves from a Binary Black Hole Merger." Physical Review Letters, 116(6), 061102.

2. Maggiore, M. (2008). "Gravitational Waves. Volume 1: Theory and Experiments." Oxford University Press.

3. Misner, C. W., Thorne, K. S., & Wheeler, J. A. (1973). "Gravitation." W. H. Freeman.

4. Hamilton, A. J. S., & Lisle, J. P. (2008). "The river model of black holes." American Journal of Physics, 76(6), 519-532. arXiv:gr-qc/0411060.

5. Gullstrand, A. (1922). "Allgemeine Lösung des statischen Einkörperproblems in der Einsteinschen Gravitationstheorie." Arkiv för Matematik, Astronomi och Fysik, 16(8), 1-15.

6. Painlevé, P. (1921). "La mécanique classique et la théorie de la relativité." Comptes Rendus de l'Académie des Sciences, 173, 677-680.

7. Kruskal, M. D. (1960). "Maximal extension of Schwarzschild metric." Physical Review, 119(5), 1743.

8. Weinberg, S. (1972). "Gravitation and Cosmology: Principles and Applications of the General Theory of Relativity." Wiley.

9. Poisson, E., & Will, C. M. (2014). "Gravity: Newtonian, Post-Newtonian, Relativistic." Cambridge University Press.

## License

MIT License

---

[中文版本](README_CN.md)