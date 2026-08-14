[中文版本](README.zh.md)

---

# Black Hole Simulator 🌌

A real-time 3D black hole simulation with gravitational wave radiation, orbital inspiral, tidal disruption, and spacetime curvature visualization, written in Rust using WGPU.

![Black Hole Simulation](static/interface.png)

## Features

- **Real-time N-body simulation** with Newtonian gravity + 2.5PN post-Newtonian corrections
- **Gravitational wave radiation reaction** (Peters 1964): binary inspiral → plunge → merger
- **Schwarzschild spacetime distortion** via Tendex lines (tidal tensor visualization, ribbon rendering with intensity-driven thickness)
- **Three-orthogonal-planes mode** for cleaner cross-section visualization of tidal fields
- **Binary black hole photon sphere deformation** due to companion perturbation
- **Tidal disruption and event horizon absorption** with Hills-mass branching
- **Gravitational wave retarded propagation**: radiative field at speed c, near-field instantaneous
- **Trajectory prediction** with radiation damping
- **Spawn safety validation**: real-time checks against event horizons, merger thresholds, Roche limits and body overlap when adding black holes/bodies — prevents instant mergers/absorption/collisions; one-click "Safe Pos" auto-avoidance
- **Instanced rendering** for efficient trail and debris visualization

## Physics Theory

### 1. Tendex Lines: Spacetime Curvature Visualization

The simulation uses **Tendex lines** to visualize spacetime curvature, based on Owen et al. (2011, arXiv:1012.4869).

The "electric" part of the curvature tensor $E_{jk}$ describes tidal stretching/compression. In the Newtonian limit:

$$
E_{jk} = \sum_i \frac{GM_i}{r_i^3} \left( 3 n_j n_k - \delta_{jk} \right)
$$

where $\mathbf{n} = \mathbf{r}/r$ is the unit vector from the mass source to the field point.

At each spatial sample point, we compute the three eigenvalues and eigenvectors of $E_{jk}$:

- **Red lines**: positive eigenvalue direction (tidal **stretching**)
- **Blue lines**: negative eigenvalue direction (tidal **compression**)
- Line length: fixed at $\frac{2}{3} \times \text{grid spacing}$ (1/3 on each side, leaving 1/3 gap between adjacent grid points)
- Line thickness and opacity: modulated by $\sqrt{|\lambda|}$ (intensity), so stronger tidal forces produce thicker, brighter lines

Since $E_{jk}$ is traceless ($\text{tr}(E) = 0$), the three eigenvalues sum to zero; stretching and compression directions are complementary.

**Rendering**: Lines are rendered as camera-facing quad ribbons (TriangleList topology, 2 triangles = 6 vertices per line), with intensity driving both thickness ($0.2\times$ ~ $1.5\times$ base) and opacity ($0$ ~ $0.75$).

**Three-orthogonal-planes mode**: Instead of rendering the full 3D volume of grid points, you can toggle to show only the three central orthogonal planes (XY, YZ, XZ), producing a cleaner cross-section view. The grid center follows the mass-weighted centroid of all black holes with a smooth response (0.15 per frame, ~7 frames to 95%).

### 2. Gravitational Wave Radiation Reaction (Peters 1964)

Binary systems lose orbital energy to gravitational wave radiation, causing inspiral and eventual merger. Based on the classic Peters (1964) formula:

**Energy loss rate**:

$$
\frac{dE}{dt} = -\frac{32}{5} \frac{G^4 \mu^2 M^3}{c^5 a^5}
$$

**Semi-major axis decay rate**:

$$
\frac{da}{dt} = -\frac{64}{5} \frac{G^3 \mu M^2}{c^5 a^3}
$$

**Equivalent relative drag acceleration** (natural units G=c=1):

$$
\mathbf{a}_{\text{rad}} = -\frac{32}{5} \frac{\mu M^2}{r^4} \mathbf{v}_{\text{rel}}
$$

Split to two bodies (center-of-mass frame, accelerations are Galilean-invariant):

$$
\mathbf{a}_i = +\frac{32}{5} \frac{m_i m_j^2}{r^4} \mathbf{v}_{\text{rel}}
$$

$$
\mathbf{a}_j = -\frac{32}{5} \frac{m_i^2 m_j}{r^4} \mathbf{v}_{\text{rel}}
$$

where $\mu = m_i m_j / (m_i + m_j)$ is the reduced mass, $M = m_i + m_j$ is the total mass, $r$ is the separation, and $\mathbf{v}_{\text{rel}} = \mathbf{v}_j - \mathbf{v}_i$ is the relative velocity.

**Application**: applied to black-hole pairs, body-black-hole pairs, and debris-black-hole pairs.

**Plunge phase enhancement**: when $r < 3M$ (past ISCO), the reaction coefficient is linearly enhanced up to 3× to model non-linear effects near merger.

### 3. Black Hole Merger Criterion: ISCO

Binary black hole inspiral ends at the **ISCO (Innermost Stable Circular Orbit)**, followed by a rapid plunge phase.

**ISCO criterion** (Blanchet & Iyer 2003, 3PN):

$$
C_{\text{ISCO}}^{3\text{PN}} = 1 - 6x + 14\nu x^2 + \nu\left[\frac{397}{2} - \frac{123}{16}\pi^2 - 14\nu\right] x^3 = 0
$$

where $x = (G M \Omega / c^3)^{2/3}$ is the PN parameter, $\nu = \mu/M$ is the symmetric mass ratio.

- Test-particle limit ($\nu \to 0$): $r_{\text{ISCO}} = 6GM/c^2 = 3(r_{s1}+r_{s2})$
- Equal-mass ($\nu = 1/4$): numerical relativity gives $r_{\text{ISCO}} \approx 5M$

**This simulator's merger condition**: $r < 0.5(r_{s1}+r_{s2})$ (50% horizon overlap)

Physically, ISCO ≈ $3(r_{s1}+r_{s2})$ is the inspiral endpoint; here we continue the plunge to significant horizon overlap. In real GR, a common horizon forms before the individual horizons touch; this simulator delays merger triggering for visualization, letting two black holes interpenetrate and visually overlap before merging, reflecting the point-like nature of the singularities.

**Mass loss**: the merged black hole has mass $0.95 \times (m_1 + m_2)$; 5% of the mass is radiated as gravitational waves (Peters formula prediction).

### 4. Photon Sphere Deformation (Binary Black Holes)

Isolated Schwarzschild black hole photon sphere radius and critical impact parameter:

$$
r_{\text{ph}} = \frac{3GM}{c^2} = \frac{3}{2} r_s, \quad b_c = 3\sqrt{3} \frac{GM}{c^2} \approx 5.196 M
$$

**Companion perturbation** (Erdl & Schneider 1993; Patil et al. 2016 arXiv:1610.04863; Cunha et al. 2018 arXiv:1805.03798):

Let the primary black hole have mass $M$, the companion have mass $M'$ at distance $D$ in direction $\hat{n}$. In the weak-perturbation limit ($D \gtrsim 10M$), the critical impact parameter becomes:

$$
b_c \approx 3\sqrt{3} M \left( 1 + \delta_{\text{mono}} + \delta_{\text{tidal}} \right)
$$

**Monopole perturbation** (overall compression):

$$
\delta_{\text{mono}} = -\kappa_1 \frac{M'}{D}
$$

**Quadrupole tidal perturbation** (angle-dependent deformation):

$$
\delta_{\text{tidal}} = \kappa_2 \frac{M'}{M} \left(\frac{M}{D}\right)^2 P_2(\cos\theta)
$$

where $P_2(\cos\theta) = \frac{1}{2}(3\cos^2\theta - 1)$ is the second Legendre polynomial, $\theta$ is the angle between the ray direction and the companion direction.

**Calibration constants**: $\kappa_1 = 2$, $\kappa_2 = 5$ (weak-field approximation, error < few percent for $D \gtrsim 10M$).

**Physical effect**: the photon sphere is elongated toward the companion and compressed on the far side; when $D$ decreases below a threshold, the two photon spheres merge and disappear.

### 5. Gravitational Wave Propagation Speed and Retardation

**Standard GR result**: gravitational waves propagate exactly at the speed of light $c$ (Einstein 1916, 1918). Wave equation:

$$
\square h_{\mu\nu} = -\frac{16\pi G}{c^4} T_{\mu\nu}, \quad \square = -\partial_t^2 + c^2 \nabla^2
$$

**Observational constraint** (GW170817/GRB 170817A, Abbott et al. 2017):

$$
-3 \times 10^{-15} \leq \frac{v_{\text{gw}} - c}{c} \leq +7 \times 10^{-16}
$$

**Retarded time** (Blanchet 2014, Eq. 219):

$$
t_{\text{ret}} = t_{\text{obs}} - \frac{|\mathbf{x}_{\text{obs}} - \mathbf{x}_{\text{source}}|}{c}
$$

The radiative field at observation point $(t, \mathbf{x})$ is determined by the source state at $t_{\text{ret}}$:

$$
h_{ij}^{\text{TT}}(t, \mathbf{x}) = \frac{2G}{c^4 R} \Lambda_{ij,kl}(\hat{N}) \frac{d^2 I_{kl}}{dt^2}(t_{\text{ret}})
$$

**Near-field vs. radiative field** (2.5PN expansion structure):

| Component                      | Distance dependence        | Propagation                                     |
| ------------------------------ | -------------------------- | ----------------------------------------------- |
| Near-field (Newtonian-like)    | $\propto 1/r^2, 1/r^3$ | Instantaneous in Newtonian limit (PN 0th order) |
| Radiative (gravitational wave) | $\propto 1/r$          | Strictly at $c$                              |

In this simulator:

- Newtonian tidal field (Tendex static part) uses instantaneous positions (PN 0th order)
- Gravitational wave radiative field uses retarded time $t_{\text{ret}} = t - r/c$
- `WAVE_SPEED = c = 1` (natural units)
- Grid eigenvalue temporal smoothing factor 0.3, approximating PN tail terms ($\propto 1/c^2$) hereditary effect

### 6. Tidal Disruption and Event Horizon Absorption

**Roche limit** (Rees 1988, Hills 1975):

$$
d_{\text{Roche}} = R_{\text{body}} \left( \frac{2 M_{\text{bh}}}{M_{\text{body}}} \right)^{1/3}
$$

**Condition for disruption outside horizon**: $d_{\text{Roche}} > r_s = 2GM_{\text{bh}}/c^2$

Equivalently, a density criterion:

$$
\rho_{\text{body}} < \rho_{\text{BH}} = \frac{3 M_{\text{bh}}}{4\pi r_s^3} = \frac{3 c^6}{32\pi G^3 M_{\text{bh}}^2}
$$

**Hills mass** $M_H$: for Sun-like stars ($M_\odot, R_\odot$):

$$
M_H \approx 1.08 \times 10^8 M_\odot \quad \text{(Schwarzschild)}
$$

- $M_{\text{bh}} < M_H$: Sun-like stars are disrupted outside the horizon (producing tidal disruption events, TDEs)
- $M_{\text{bh}} > M_H$: stars cross the horizon intact and are swallowed whole

**This simulator's branching logic**:

- If $d_{\text{Roche}} > r_s$ (large/low-density bodies): Roche disruption path, producing 60 debris particles forming an accretion disk around the black hole
- If $d_{\text{Roche}} < r_s$ (compact bodies like neutron stars, white dwarfs): direct absorption

**Debris disk formation**: When a body is tidally disrupted, debris forms a prograde accretion disk around the black hole centered at the black hole position, with orbital radius ≥ 1.5 × ISCO radius (ensuring debris spawns outside the event horizon, even if the body was already inside). Particles follow Keplerian orbital velocities and gradually spiral in via gravitational wave radiation reaction.

### 7. Gravitational Wave Polarizations

Implements both plus (+) and cross (×) polarization modes (Maggiore 2008):

$$
h_+ = h_0 \cdot \frac{1 + \cos^2\iota}{2} \cdot \cos(\Phi)
$$
$$
h_\times = h_0 \cdot \cos\iota \cdot \sin(\Phi)
$$

where $\iota$ is the inclination of the observer relative to the orbital plane, $\Phi$ is the gravitational wave phase.

**Chirp mass**:

$$
M_c = \frac{(m_1 m_2)^{3/5}}{(m_1 + m_2)^{1/5}}, \quad h_0 = \frac{4 M_c^{5/3} \omega^{2/3}}{D}
$$

**TT-gauge strain tensor** (with polarization angle $\psi$ rotation):

$$
h_{\theta\theta} = h_+ \cos 2\psi - h_\times \sin 2\psi
$$
$$
h_{\phi\phi} = -h_+ \cos 2\psi - h_\times \sin 2\psi
$$
$$
h_{\theta\phi} = h_+ \sin 2\psi + h_\times \cos 2\psi
$$

### 8. Linearized Gravity and Superposition

$$
g_{\mu\nu} = \eta_{\mu\nu} + h_{\mu\nu}, \quad |h_{\mu\nu}| \ll 1
$$

In the weak-field limit, curvature tensors from multiple sources **superpose linearly**:

$$
E_{jk}^{\text{total}} = \sum_{i=1}^{N_{\text{bodies}}} E_{jk}^{(i)} + E_{jk}^{\text{GW}}
$$

All mass sources (black holes and ordinary bodies) produce tidal effects, superposed with the dynamic gravitational wave oscillation.

## Code Structure

```
src/
├── main.rs              # Application entry, event loop, ApplicationHandler
├── ui.rs                # egui panel, axis gizmo, spawn safety indicators
├── camera.rs            # Camera control and perspective projection
├── geometry.rs          # Sphere/torus geometry generation
├── physics/
│   ├── mod.rs           # Simulation struct, gw_radiation_reaction (Peters formula)
│   ├── integrator.rs    # Shared gravity integrator (live sim & trajectory prediction)
│   ├── spawn.rs         # Spawn validation: prevent instant merger/absorption/collision
│   ├── grid.rs          # Tendex lines: tidal tensor computation and eigendecomposition
│   ├── collision.rs     # Black hole merger (ISCO), horizon absorption (Hills), Roche disruption
│   └── trajectory.rs    # Trajectory prediction (with radiation damping)
└── renderer/
    ├── mod.rs           # Renderer struct, render method
    ├── types.rs         # Vertex/Uniform structs, constants
    ├── shaders.rs       # WGSL shaders (with photon sphere deformation)
    └── pipeline.rs      # Render pipeline and buffer creation
```

### Key Modules

- **[physics/mod.rs](src/physics/mod.rs)** - Core physics:

  - `gw_radiation_reaction()` - Peters 1964 formula implementation, with plunge phase enhancement
  - `update_debris()` - Debris gravity + radiation damping updates
  - `center_of_mass()` - Mass-weighted center of mass of all black holes (shared by camera & grid)
- **[physics/integrator.rs](src/physics/integrator.rs)** - Shared gravity integrator:

  - `step_gravity()` - Single shared time-step for black holes & bodies; used by both the live simulation and trajectory prediction, so previews match actual evolution exactly
- **[physics/spawn.rs](src/physics/spawn.rs)** - Spawn validation & auto-avoidance:

  - `check_black_hole_spawn() / check_body_spawn()` - Reject positions inside event horizons, merger thresholds, Roche limits, or overlapping bodies (with a 20% safety margin)
  - `safe_black_hole_pos() / safe_body_pos()` - Nudge a conflicting position to the nearest safe spot
- **[physics/grid.rs](src/physics/grid.rs)** - Tendex line spacetime curvature visualization:

  - `compute_tidal_tensor()` - Computes tidal tensor $E_{jk}$ (with retarded GW contribution)
  - `update_grid_points()` - Eigendecomposition + temporal smoothing (approximating PN tail terms)
  - `get_tendex_render_data()` - Generates camera-facing quad ribbon vertices (intensity-driven thickness & opacity, three-planes mode)
- **[physics/collision.rs](src/physics/collision.rs)** - Collisions and evolution:

  - `check_mergers()` - ISCO-based merger criterion
  - `check_event_horizon_absorption()` - Hills-mass branching: disruption vs. absorption
  - `check_roche_disruption()` - Roche-limit disruption producing accretion disks
  - `check_body_collisions()` - Body collision fragmentation based on Q*_D scaling law
- **[renderer/shaders.rs](src/renderer/shaders.rs)** - WGSL shaders:

  - `perturbed_photon_sphere()` - Photon sphere deformation (companion perturbation)
  - `compute_lensed_direction()` - Gravitational lensing ray bending
  - `star_field()` - Procedural starfield (with Milky Way band, nebulae)

## Build and Run

### Desktop

```bash
# Build with optimizations
cargo build --release

# Run
cargo run --release
```

### Web (WebAssembly)

```bash
# Install wasm32 target
rustup target add wasm32-unknown-unknown

# Install wasm-bindgen-cli
cargo install wasm-bindgen-cli

# Build wasm release
cargo build --release --target wasm32-unknown-unknown --features web

# Generate JS bindings into dist/
wasm-bindgen --out-dir dist --target web target/wasm32-unknown-unknown/release/blackhole_sim.wasm

# Copy index.html
cp static/index.html dist/

# Serve locally (Wasm requires an HTTP server, cannot use file://)
cd dist && python -m http.server 8080
# Open http://localhost:8080/
```

The web build uses the WebGL2 backend via `wgpu` for broad browser compatibility.

## Controls

- **Mouse**: Rotate camera
- **Scroll**: Zoom in/out
- **UI Panel**: Add black holes/bodies, adjust parameters, toggle visualization options
  - Spawn positions are validated in real time: the Add button is disabled with the reason shown when too close to an existing black hole/body (event horizon, merger threshold, Roche limit, or overlap); click **🛡 Safe Pos** to auto-nudge to a safe location
  - **Gravity Waves**: Toggle Tendex grid visualization
  - **Three Orthogonal Planes**: Show only XY/YZ/XZ central planes (cleaner cross-section)
  - **Grid Size / Spacing**: Adjust grid resolution and physical scale
- **Space**: Pause/resume
- **ESC**: Exit

## References

### Gravitational Wave Radiation and Inspiral

1. **Peters, P. C.** (1964). "Gravitational Radiation and the Motion of Two Point Masses." *Physical Review*, 136(4B), B1224-B1232. — **Core formula source**
2. **Blanchet, L.** (2014). "Gravitational Radiation from Post-Newtonian Sources and Inspiralling Compact Binaries." *Living Reviews in Relativity*, 17, 2. arXiv:1310.1528. — PN expansion and retardation
3. **Blanchet, L. & Iyer, B. R.** (2003). "Third post-Newtonian dynamics of compact binaries." *Class. Quantum Grav.*, 20, 755. — 3PN ISCO criterion
4. **Blanchet, L., Langlois, K. & Ligout, P.** (2025). "ISCO of arbitrary-mass compact binaries at fourth post-Newtonian order." arXiv:2505.01278. — 4PN ISCO
5. **Buonanno, A., Cook, G. B. & Pretorius, F.** (2007). "Inspiral, plunge, merger, ringdown waveform of black-hole binaries." *Phys. Rev. D*, 75, 124018. arXiv:gr-qc/0610122.

### ISCO and Black Hole Merger

6. **Barack, L. & Sago, N.** (2007). "Gravitational self-force on a particle in circular orbit around a Schwarzschild black hole." *Phys. Rev. D*, 75, 064021. — GSF ISCO offset $\alpha = 1.2512$
7. **Favata, M.** (2010). "Conservative self-force correction to the innermost stable circular orbit." *Phys. Rev. D*, 83, 024028.

### Photon Sphere and Gravitational Lensing

8. **Synge, J. L.** (1966). "The escape of photons from gravitationally intense stars." *Mon. Not. R. Astron. Soc.*, 131, 463. — $b_c = 3\sqrt{3} M$
9. **Erdl, H. & Schneider, P.** (1993). "The gravitational lensing in the binary black hole system." *Astronomy & Astrophysics*, 268, L9. — Binary black hole lensing
10. **Patil, S. P., Mishra, M. & Narasimha, B. P.** (2016). "Curious case of gravitational lensing by binary black holes." arXiv:1610.04863. — Dual photon sphere merger
11. **Cunha, P. V. P., Herdeiro, C. A. R. & Rodriguez, M. J.** (2018). "Shadows of exact binary black holes." *Phys. Rev. D*, 98, 044053. arXiv:1805.03798.
12. **Assumpção, T. et al.** (2018). "Black hole binaries: ergoregions, photon surfaces, wave scattering." arXiv:1806.07909.
13. **Weinberg, S.** (1972). *Gravitation and Cosmology*. Wiley. — Deflection angle formula
14. **Keeton, C. R. & Petters, A. O.** (2005). "Formalism for testing theories of gravity using lensing by compact objects." *Phys. Rev. D*, 72, 104006. — Second-order deflection

### Gravitational Wave Propagation Speed

15. **Einstein, A.** (1916, 1918). "Näherungsweise Integration der Feldgleichungen / Über Gravitationswellen." *Sitzungsber. K. Preuss. Akad. Wiss.* — GWs propagate at c
16. **Abbott, B. P. et al.** (2017). "Gravitational Waves and Gamma-Rays from a Binary Neutron Star Merger: GW170817 and GRB 170817A." *Astrophysical Journal Letters*, 848, L13. — $v_{\text{gw}} = c$ to $10^{-15}$
17. **Will, C. M.** (1998). "Bounding the mass of the graviton using gravitational-wave observations." *Phys. Rev. D*, 57, 2061. arXiv:gr-qc/9709011.

### Tidal Disruption

18. **Rees, M. J.** (1988). "Tidal disruption of stars by black holes of 10⁶–10⁸ solar masses." *Nature*, 333, 523-528. — Hills mass
19. **Hills, J. G.** (1975). "Possible power source of Seyfert galaxies and QSOs." *Nature*, 254, 295-298.
20. **Kesden, M.** (2012). "Tidal-disruption rate of stars by spinning supermassive black holes." *Phys. Rev. D*, 86, 064026. — Spin dependence
21. **Stone, N. C., Kesden, M., Cheng, R. M. & van Velzen, S.** (2019). "Stellar Tidal Disruption Events in General Relativity." *Gen. Rel. Grav.*, 51, 30. arXiv:1801.10180.

### Tidal Tensor and Visualization

22. **Owen, R. et al.** (2011). "Frame-Dragging Vortexes and Tidal Tendexes Attached to Colliding Black Holes." *Physical Review Letters*, 106, 151101. arXiv:1012.4869. — Tendex lines
23. **Nichols, D. A.** "Visualizations of Spacetime Curvature." https://dnichols1.github.io/visualizations/

### Foundational Textbooks

24. **Maggiore, M.** (2008). *Gravitational Waves. Volume 1: Theory and Experiments.* Oxford University Press.
25. **Misner, C. W., Thorne, K. S. & Wheeler, J. A.** (1973). *Gravitation.* W. H. Freeman.
26. **Poisson, E. & Will, C. M.** (2014). *Gravity: Newtonian, Post-Newtonian, Relativistic.* Cambridge University Press.
27. **Hamilton, A. J. S. & Lisle, J. P.** (2008). "The river model of black holes." *Am. J. Phys.*, 76, 519-532. arXiv:gr-qc/0411060.

### Observational Discoveries

28. **Abbott, B. P. et al.** (2016). "Observation of Gravitational Waves from a Binary Black Hole Merger." *Physical Review Letters*, 116(6), 061102. — GW150914 first detection

## License

MIT License
