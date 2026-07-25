// WGSL 着色器源码字符串

/// 顶点着色器
pub(crate) const VERTEX_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

struct ModelUniform {
    model: mat4x4<f32>,
    object_type: u32,
    time: f32,
    alpha: f32,
    bh_mass: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) object_type: u32,
    @location(3) world_pos: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) view_dir: vec3<f32>,
    @location(6) bh_center: vec3<f32>,
    @location(7) bh_mass: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = model.model * vec4<f32>(input.position, 1.0);
    out.clip_position = camera.view_proj * world;
    out.uv = input.uv;
    out.normal = (model.model * vec4<f32>(input.normal, 0.0)).xyz;
    out.object_type = model.object_type;
    out.world_pos = world.xyz;
    out.alpha = model.alpha;
    out.view_dir = normalize(camera.camera_pos - world.xyz);
    out.bh_center = model.model[3].xyz;
    out.bh_mass = model.bh_mass;
    return out;
}
"#;

/// 片段着色器 - 基于物理的黑洞渲染（全屏幕引力透镜）
pub(crate) const FRAGMENT_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) object_type: u32,
    @location(3) world_pos: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) view_dir: vec3<f32>,
    @location(6) bh_center: vec3<f32>,
    @location(7) bh_mass: f32,
};

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

struct ModelUniform {
    model: mat4x4<f32>,
    object_type: u32,
    time: f32,
    alpha: f32,
    bh_mass: f32,
};

struct BlackHoleData {
    pos: vec3<f32>,
    mass: f32,
};

struct BlackHolesUniform {
    count: u32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    holes: array<BlackHoleData, 8>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> model: ModelUniform;
@group(0) @binding(2) var<uniform> bhs: BlackHolesUniform;

const PI: f32 = 3.14159265359;

fn schwarzschild_radius(mass: f32) -> f32 {
    return 2.0 * mass;
}

fn photon_sphere_radius(mass: f32) -> f32 {
    return 3.0 * mass;
}

fn isco_radius(mass: f32) -> f32 {
    return 6.0 * mass;
}

fn accretion_disk_color(r: f32, r_inner: f32, r_outer: f32) -> vec3<f32> {
    let t = clamp((r - r_inner) / (r_outer - r_inner), 0.0, 1.0);
    let inner_col = vec3<f32>(1.0, 0.95, 0.7);
    let mid_col = vec3<f32>(1.0, 0.55, 0.1);
    let outer_col = vec3<f32>(0.45, 0.08, 0.04);
    var col = mix(inner_col, mid_col, smoothstep(0.0, 0.5, t));
    col = mix(col, outer_col, smoothstep(0.5, 1.0, t));
    let temp = pow(1.0 - t * 0.6, 1.5);
    return col * temp;
}

fn hash3(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn star_field(dir: vec3<f32>) -> vec3<f32> {
    // 银河平面法线：水平带（Y 轴向上）
    let galaxy_normal = vec3<f32>(0.0, 1.0, 0.0);
    let galaxy_dist = abs(dot(dir, galaxy_normal));
    // 更窄更笔直的银河带
    let galaxy_band = exp(-galaxy_dist * galaxy_dist * 20.0);
    let galaxy_noise = hash3(dir * 15.0) * 0.3 + hash3(dir * 40.0) * 0.2;
    let galaxy_bright = galaxy_band * (0.9 + galaxy_noise * 0.2);

    // 深空背景
    var bg = vec3<f32>(0.002, 0.003, 0.008);

    // 银河带颜色：保持高亮度
    let galaxy_core = vec3<f32>(0.3, 0.25, 0.15);
    let galaxy_mid = vec3<f32>(0.15, 0.12, 0.1);
    let galaxy_outer = vec3<f32>(0.05, 0.05, 0.1);
    var galaxy_col = mix(galaxy_outer, galaxy_mid, smoothstep(0.0, 0.5, galaxy_band));
    galaxy_col = mix(galaxy_col, galaxy_core, smoothstep(0.5, 1.0, galaxy_band));
    // 减少尘埃噪声
    let dust = sin(atan2(dir.z, dir.x) * 3.0) * 0.5 + 0.5;
    galaxy_col = galaxy_col * (0.6 + dust * 0.4);
    bg = bg + galaxy_col * galaxy_bright * 2.5;

    // 星云
    var nebula_p = dir * 6.0;
    var nebula_col = vec3<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        let q = floor(nebula_p);
        let id = hash3(q + vec3<f32>(f32(i) * 13.0));
        if (id > 0.95) {
            let nx = hash3(q + vec3<f32>(1.0, 2.0, 3.0));
            let ny = hash3(q + vec3<f32>(4.0, 5.0, 6.0));
            let nz = hash3(q + vec3<f32>(7.0, 8.0, 9.0));
            let center = normalize(vec3<f32>(nx, ny, nz) * 2.0 - 1.0);
            let d = distance(dir, center);
            let glow = exp(-d * d * 50.0) * max(galaxy_band, 0.15);
            let ncol_id = hash3(q + vec3<f32>(10.0, 20.0, 30.0));
            var n_col = mix(vec3<f32>(0.5, 0.18, 0.4), vec3<f32>(0.18, 0.35, 0.6), ncol_id);
            n_col = mix(n_col, vec3<f32>(0.6, 0.3, 0.18), step(0.6, ncol_id));
            nebula_col = nebula_col + n_col * glow;
        }
        nebula_p = nebula_p * 1.6;
    }
    bg = bg + nebula_col * 0.7;

    // 恒星：更亮更多
    var p = dir * 150.0;
    var star = 0.0;
    var star_col = vec3<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        let q = floor(p);
        let f = fract(p);
        let id = hash3(q);
        let threshold = mix(0.995, 0.975, galaxy_band);
        let s = step(threshold, id);
        let cx = hash3(q + vec3<f32>(1.0, 2.0, 3.0));
        let cy = hash3(q + vec3<f32>(4.0, 5.0, 6.0));
        let cz = hash3(q + vec3<f32>(7.0, 8.0, 9.0));
        let center = vec3<f32>(cx, cy, cz) * 0.5 + 0.25;
        let d = distance(f, center);
        let bright = s * smoothstep(0.08, 0.0, d);
        let col_id = hash3(q + vec3<f32>(10.0, 20.0, 30.0));
        var s_col = mix(vec3<f32>(0.95, 0.97, 1.0), vec3<f32>(1.0, 0.9, 0.7), col_id);
        s_col = mix(s_col, vec3<f32>(0.8, 0.9, 1.0), step(0.7, col_id));
        s_col = mix(s_col, vec3<f32>(1.0, 0.65, 0.55), step(0.9, col_id));
        star = star + bright;
        star_col = star_col + s_col * bright * 1.2;
        p = p * 2.0;
    }
    if (star > 0.0) {
        bg = bg + star_col;
    }

    return bg;
}

fn ray_sphere_intersect(ro: vec3<f32>, rd: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let oc = ro - center;
    let b = dot(oc, rd);
    let c = dot(oc, oc) - radius * radius;
    let h = b * b - c;
    if (h < 0.0) { return vec2<f32>(-1.0, -1.0); }
    let s = sqrt(h);
    return vec2<f32>(-b - s, -b + s);
}

fn ray_plane_intersect(ro: vec3<f32>, rd: vec3<f32>, plane_p: vec3<f32>, plane_n: vec3<f32>) -> f32 {
    let denom = dot(plane_n, rd);
    if (abs(denom) < 0.0001) { return -1.0; }
    let t = dot(plane_p - ro, plane_n) / denom;
    return t;
}

fn rotate_vector_towards(v: vec3<f32>, tgt: vec3<f32>, angle: f32) -> vec3<f32> {
    let axis = cross(v, tgt);
    let axis_len = length(axis);
    if (axis_len < 0.0001) { return v; }
    let axis_n = axis / axis_len;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let result = v * cos_a + cross(axis_n, v) * sin_a + axis_n * dot(axis_n, v) * (1.0 - cos_a);
    return normalize(result);
}

fn compute_lensed_direction(ro: vec3<f32>, rd: vec3<f32>) -> vec3<f32> {
    var dir = rd;
    var hit_horizon = false;

    for (var idx = 0u; idx < bhs.count; idx = idx + 1u) {
        let bh = bhs.holes[idx];
        let mass = bh.mass;
        let bh_pos = bh.pos;
        let rs = schwarzschild_radius(mass);

        let to_bh = bh_pos - ro;
        let dist = length(to_bh);
        let to_bh_n = to_bh / dist;

        let perp = cross(dir, to_bh_n);
        let perp_len = length(perp);
        let impact_param = perp_len * dist;

        // 临界碰撞参数 b_c = 3√3·M (Synge 1966)
        // 受其他黑洞扰动后变形 (Erdl & Schneider 1993, Patil et al. 2016)：
        //   b_c ≈ 3√3·M·(1 + δ_mono + δ_tidal)
        //   δ_mono = -κ₁·M'/D                      (整体压缩，朝伴星方向拉伸)
        //   δ_tidal = κ₂·(M'/M)·(M/D)²·P₂(cosθ)    (四极潮汐变形)
        //   P₂(cosθ) = (3cos²θ - 1)/2, θ 为光线方向与伴星方向夹角
        let b_c = perturbed_photon_sphere(mass, bh_pos, rd);

        if (impact_param < b_c) {
            hit_horizon = true;
            break;
        }

        // 引力偏折角：α = 4M/b (一阶) + 15π/4 · (M/b)² (二阶)
        // 参考：Weinberg 1972, Keeton & Petters 2005
        var bend_angle = 4.0 * mass / impact_param + (15.0 * PI / 4.0) * mass * mass / (impact_param * impact_param);
        bend_angle = min(bend_angle, PI * 1.5);

        let bend_axis = normalize(cross(dir, to_bh_n));
        dir = rotate_vector_towards(dir, to_bh_n, bend_angle * 0.5);
    }

    if (hit_horizon) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }

    return dir;
}

/// 计算受其他黑洞扰动的光子球临界碰撞参数 b_c
/// 公式：b_c ≈ 3√3·M·(1 - κ₁·Σ(M'/D) + κ₂·Σ(M'/M)·(M/D)²·P₂(cosθ))
/// 参考：Erdl & Schneider 1993; Patil et al. 2016 arXiv:1610.04863;
///       Cunha et al. 2018 arXiv:1805.03798
/// κ₁=2, κ₂=5 为标定常数（弱场近似，D ≳ 10M 时误差 < 几个百分点）
fn perturbed_photon_sphere(mass: f32, bh_pos: vec3<f32>, rd: vec3<f32>) -> f32 {
    let b_c_0 = 3.0 * sqrt(3.0) * mass;
    var delta = 0.0;

    for (var j = 0u; j < bhs.count; j = j + 1u) {
        let other = bhs.holes[j];
        let to_other = other.pos - bh_pos;
        let D = length(to_other);
        if (D < 0.1) {
            continue;
        }
        let n_hat = to_other / D;

        // θ 为光线方向与伴星方向夹角
        let cos_theta = clamp(dot(rd, n_hat), -1.0, 1.0);
        let P2 = 0.5 * (3.0 * cos_theta * cos_theta - 1.0);

        // 单极扰动（整体压缩）+ 四极潮汐扰动（角度相关变形）
        let kappa1 = 2.0;
        let kappa2 = 5.0;
        let delta_mono = -kappa1 * other.mass / D;
        let delta_tidal = kappa2 * (other.mass / mass) * (mass / D) * (mass / D) * P2;
        delta = delta + delta_mono + delta_tidal;
    }

    // 限制扰动幅度，避免负值或过大变形
    delta = clamp(delta, -0.5, 0.8);
    return b_c_0 * (1.0 + delta);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.object_type == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, input.alpha);
    } else if (input.object_type == 1u) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    } else if (input.object_type == 2u) {
        let center = input.bh_center;
        let p = input.world_pos;
        let to_p = p - center;
        let r = length(to_p);
        
        let thickness = 0.1;
        let ring = 1.0 - smoothstep(0.0, thickness, abs(r - 1.0));
        
        let phi = atan2(to_p.z, to_p.x);
        let theta = acos(clamp(to_p.y / max(r, 0.001), -1.0, 1.0));
        
        let time_phase = model.time * 4.0;
        let animated = 0.5 + 0.5 * cos(2.0 * phi + time_phase) * sin(theta) * sin(theta);
        
        var col = vec3<f32>(0.3, 0.7, 1.0);
        let intensity = ring * animated * input.alpha * 0.9;
        
        if (intensity < 0.015) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }
        
        return vec4<f32>(col, intensity);
    } else if (input.object_type == 3u) {
        let ro = camera.camera_pos;
        let rd = normalize(input.world_pos - camera.camera_pos);

        if (bhs.count == 0u) {
            return vec4<f32>(star_field(rd), 1.0);
        }

        var hit_black = false;
        var photon_ring = 0.0;

        for (var idx = 0u; idx < bhs.count; idx = idx + 1u) {
            let bh = bhs.holes[idx];
            let mass = bh.mass;

            // 扰动后的临界碰撞参数（受其他黑洞影响）
            // 光子球变形：朝伴星方向拉伸，背向压缩
            let b_c = perturbed_photon_sphere(mass, bh.pos, rd);

            let to_bh = bh.pos - ro;
            let cross_v = cross(rd, to_bh);
            let b = length(cross_v);

            if (b < b_c) {
                hit_black = true;
            }

            // 光子环宽度随变形调整：变形越大环越宽（更易观察变形）
            let rs = schwarzschild_radius(mass);
            let deformation = abs(b_c - 3.0 * sqrt(3.0) * mass) / (3.0 * sqrt(3.0) * mass);
            let ring_width = rs * 0.03 * (1.0 + deformation * 2.0);
            let ring_dist = abs(b - b_c);
            if (ring_dist < ring_width) {
                let ring_intensity = pow(1.0 - ring_dist / ring_width, 3.0);
                photon_ring = max(photon_ring, ring_intensity);
            }
        }

        if (hit_black) {
            if (photon_ring > 0.0) {
                let ring_col = vec3<f32>(1.0, 0.85, 0.5) * photon_ring * 3.0;
                return vec4<f32>(ring_col, 1.0);
            }
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }

        let lensed_dir = compute_lensed_direction(ro, rd);

        if (length(lensed_dir) < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }

        var bg_col = star_field(lensed_dir);

        if (photon_ring > 0.0) {
            let ring_col = vec3<f32>(1.0, 0.85, 0.5) * photon_ring * 3.0;
            bg_col = bg_col + ring_col;
        }

        return vec4<f32>(bg_col, 1.0);
    } else if (input.object_type == 4u) {
        let n = normalize(input.normal);
        let view_dir = normalize(input.view_dir);
        let ndotv = max(dot(n, view_dir), 0.0);
        
        let limb = pow(ndotv, 0.5);
        
        let base_col = vec3<f32>(1.0, 0.95, 0.75);
        var col = base_col * limb;
        
        let center_boost = pow(ndotv, 4.0) * 0.3;
        col = col + vec3<f32>(1.0, 0.9, 0.6) * center_boost;
        
        return vec4<f32>(col, input.alpha);
    }
    
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
"#;

/// 碎片粒子顶点着色器（instanced billboard）
pub(crate) const DEBRIS_VERTEX_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct DebrisVSInput {
    @location(0) quad_pos: vec2<f32>,
    @location(1) instance_pos: vec3<f32>,
    @location(2) instance_speed: f32,
    @location(3) instance_life: f32,
};

struct DebrisVSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) speed: f32,
    @location(2) life: f32,
};

@vertex
fn debris_vs_main(input: DebrisVSInput) -> DebrisVSOutput {
    var out: DebrisVSOutput;

    // 实例位置投影到裁剪空间
    let center_clip = camera.view_proj * vec4<f32>(input.instance_pos, 1.0);

    // billboard 大小随距离自适应
    let dist = length(camera.camera_pos - input.instance_pos);
    let base_size = 0.15 + input.instance_speed * 0.05;
    let scale = base_size * center_clip.w / max(dist, 0.1);

    // 在裁剪空间中偏移形成 billboard
    let offset = input.quad_pos * scale;
    out.clip_position = vec4<f32>(
        center_clip.x + offset.x,
        center_clip.y + offset.y,
        center_clip.z,
        center_clip.w,
    );

    out.uv = input.quad_pos;
    out.speed = input.instance_speed;
    out.life = input.instance_life;

    return out;
}
"#;

/// 碎片粒子片段着色器（圆形发光点，颜色基于速度，additive blending）
pub(crate) const DEBRIS_FRAGMENT_SHADER: &str = r#"
struct DebrisVSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) speed: f32,
    @location(2) life: f32,
};

@fragment
fn debris_fs_main(input: DebrisVSOutput) -> @location(0) vec4<f32> {
    let d = length(input.uv);
    if (d > 1.0) {
        discard;
    }

    // 圆形发光点：中心亮，边缘衰减
    let intensity = pow(1.0 - d, 2.0);

    // 颜色基于速度：速度高 = 白蓝色，速度低 = 橙红色
    let fast_col = vec3<f32>(0.85, 0.92, 1.0);
    let slow_col = vec3<f32>(1.0, 0.4, 0.1);
    let t = clamp(input.speed * 0.5, 0.0, 1.0);
    var col = mix(slow_col, fast_col, t);

    // life 衰减（碎片寿命越长越暗）
    let life_factor = 1.0 / (1.0 + input.life * 0.3);
    col = col * intensity * life_factor * 2.5;

    return vec4<f32>(col, intensity * life_factor);
}
"#;

/// 轨迹点顶点着色器
pub(crate) const TRAIL_VERTEX_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct TrailVSInput {
    @location(0) quad_pos: vec2<f32>,
    @location(1) instance_pos: vec3<f32>,
    @location(2) instance_color_type: f32,
    @location(3) instance_shape_type: f32,
    @location(4) instance_fade: f32,
};

struct TrailVSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color_type: f32,
    @location(2) shape_type: f32,
    @location(3) fade: f32,
};

@vertex
fn trail_vs_main(input: TrailVSInput) -> TrailVSOutput {
    var out: TrailVSOutput;
    let center_clip = camera.view_proj * vec4<f32>(input.instance_pos, 1.0);
    let dist = length(camera.camera_pos - input.instance_pos);
    let scale = 0.14 * center_clip.w / max(dist, 0.1);
    let offset = input.quad_pos * scale;
    out.clip_position = vec4<f32>(
        center_clip.x + offset.x,
        center_clip.y + offset.y,
        center_clip.z,
        center_clip.w,
    );
    out.uv = input.quad_pos;
    out.color_type = input.instance_color_type;
    out.shape_type = input.instance_shape_type;
    out.fade = input.instance_fade;
    return out;
}
"#;

/// 轨迹点片段着色器
pub(crate) const TRAIL_FRAGMENT_SHADER: &str = r#"
struct TrailVSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color_type: f32,
    @location(2) shape_type: f32,
    @location(3) fade: f32,
};

fn sdf_square(p: vec2<f32>, half: f32) -> f32 {
    let d = abs(p) - vec2<f32>(half, half);
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sdf_triangle(p: vec2<f32>, r: f32) -> f32 {
    let k = sqrt(3.0);
    var p2 = vec2<f32>(abs(p.x), -p.y);
    p2.x = p2.x - r * 0.5;
    p2.y = p2.y - r * k / 6.0;
    let c = vec2<f32>(-0.5, k * 0.5);
    let m = min(dot(p2, c), 0.0);
    p2 = p2 - vec2<f32>(c.x * m, c.y * m);
    let d = vec2<f32>(length(p2), r * 0.5 - abs(p.x)) + p2 * vec2<f32>(-sign(p2.y), 0.0);
    return -min(d.x, d.y) * sign(max(d.x, d.y));
}

@fragment
fn trail_fs_main(input: TrailVSOutput) -> @location(0) vec4<f32> {
    var alpha: f32;
    let edge = 0.08;

    if (input.shape_type < 0.5) {
        // 方形（黑洞）
        let d = sdf_square(input.uv, 0.75);
        if (d > 0.0) { discard; }
        alpha = clamp(-d / edge, 0.0, 1.0);
    } else {
        // 三角形（天体）
        let d = sdf_triangle(input.uv, 0.9);
        if (d > 0.0) { discard; }
        alpha = clamp(-d / edge, 0.0, 1.0);
    }

    var col: vec3<f32>;
    if (input.color_type < 0.5) {
        // 黑洞轨迹：橙色
        col = vec3<f32>(1.0, 0.55, 0.2);
    } else if (input.color_type < 1.5) {
        // 天体轨迹：青色
        col = vec3<f32>(0.3, 0.9, 1.0);
    } else if (input.color_type < 2.5) {
        // 预览天体轨迹：黄色
        col = vec3<f32>(1.0, 0.9, 0.3);
    } else {
        // 预览黑洞轨迹：粉紫色
        col = vec3<f32>(0.9, 0.4, 1.0);
    }

    // fade: 0=最早(暗), 1=最新(亮)
    let fade_alpha = 0.15 + input.fade * 0.85;
    let fade_bright = 0.35 + input.fade * 0.65;
    let final_alpha = alpha * fade_alpha;
    let final_col = col * fade_bright;
    return vec4<f32>(final_col * final_alpha * 2.0, final_alpha);
}
"#;

/// Tendex 线顶点着色器：将线渲染为面向相机的四边形（ribbon）
/// 每个线段 6 顶点（2 个三角形），强度同时调制不透明度和线宽
pub(crate) const TENDEX_VERTEX_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_inv: mat4x4<f32>,
    proj_inv: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct TendexVertexInput {
    @location(0) center: vec3<f32>,
    @location(1) line_dir: vec3<f32>,
    @location(2) half_len: f32,
    @location(3) corner: vec2<f32>,
    @location(4) color_sign: f32,
    @location(5) intensity: f32,
    @location(6) base_thickness: f32,
};

struct TendexVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color_sign: f32,
    @location(1) intensity: f32,
};

@vertex
fn tendex_vs_main(input: TendexVertexInput) -> TendexVertexOutput {
    var out: TendexVertexOutput;

    // 视线方向（从中心指向相机）
    let view_dir = normalize(camera.camera_pos - input.center);

    // 线方向（单位向量）
    let axis = normalize(input.line_dir);

    // 垂直方向：垂直于线和视线，即 ribbon 的宽度方向
    let perp = normalize(cross(axis, view_dir));

    // 厚度：强度越高越粗（0.2x ~ 1.5x 基准厚度）
    let thickness = input.base_thickness * (0.2 + 1.3 * input.intensity);

    // 计算世界空间顶点位置
    // corner.x = 沿轴方向偏移（-1 ~ +1）
    // corner.y = 垂直方向偏移（-1 ~ +1）
    let world_pos = input.center
        + axis * input.corner.x * input.half_len
        + perp * input.corner.y * thickness * 0.5;

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.color_sign = input.color_sign;
    out.intensity = input.intensity;
    return out;
}
"#;

/// Tendex 线片段着色器：红/蓝半透明，强度调制不透明度
pub(crate) const TENDEX_FRAGMENT_SHADER: &str = r#"
struct TendexVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color_sign: f32,
    @location(1) intensity: f32,
};

@fragment
fn tendex_fs_main(input: TendexVertexOutput) -> @location(0) vec4<f32> {
    let base_alpha = 0.75;
    // 强度映射：低强度几乎不可见，高强度接近 base_alpha
    let alpha = base_alpha * input.intensity;
    if (alpha < 0.02) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    if (input.color_sign > 0.0) {
        // 拉伸（潮汐拉伸方向，红色）
        return vec4<f32>(0.95, 0.2, 0.1, alpha);
    } else {
        // 压缩（潮汐压缩方向，蓝色）
        return vec4<f32>(0.1, 0.35, 0.95, alpha);
    }
}
"#;
