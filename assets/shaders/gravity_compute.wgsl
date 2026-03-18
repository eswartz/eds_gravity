// Configuration for gravity simulation.
struct Uniforms {
    gravity_constant: f32,
    num_elements: u32,
    grav_scales_by_linear_distance: u32,
    gravity_distance_scale: f32,
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<uniform> dt: f32;

// Sync these with gravity_compute.rs!
const POINT_INFO_FLAG_FORCE_NONE: u32 = 0;
const POINT_INFO_FLAG_FORCE_GRAVITY: u32 = 1;
const POINT_INFO_FLAG_FORCE_ATTRACT_REPEL: u32 = 2;
const POINT_INFO_FLAG_FORCE_TYPE_MASK: u32 = 3;
const POINT_INFO_FLAG_FORCE_TYPE_SHIFT: u32 = 0;

const POINT_INFO_FLAG_HAS_STATIC: u32 = 0x80;

struct PointInfo {
    position: vec3f,
    mass: f32,

    velocity: vec3f,
    radius: f32,

    ang_vel: vec3f,
    ent_hi: u32,

    com: vec3f,
    ent_lo: u32,

    /// POINT_INFO_FLAG_xxx mask.
    flags: u32,
    strength : f32,
    _unused0 : u32,
    _unused1 : u32,
}
@group(0) @binding(2) var <storage, read_write> point_info: array<PointInfo>;
@group(0) @binding(3) var <storage, read_write> point_info_new: array<PointInfo>;

// @group(0) @binding(2) var <storage, read_write> forces: array<vec4f>;

@group(0) @binding(4) var <storage, read_write> colliders: array<vec4u>;
@group(0) @binding(5) var <storage, read_write> collider_count: atomic<u32>;

// Multiplies a quaternion and a 3D vector, returning the rotated vector.
fn quat_mul_vec3(quat: vec4f, rhs: vec3f) -> vec3f {
    // quat: xyzw -> xb yc zd wa
    let s = 2.0 / dot(quat, quat);
    let a = quat.x;
    let b = quat.y;
    let c = quat.z;
    let d = quat.w;
    let bs = b * s;
    let cs = c * s;
    let ds = d * s;
    let ab = a * bs;
    let ac = a * cs;
    let ad = a * ds;
    let bb = b * bs;
    let bc = b * cs;
    let bd = b * ds;
    let cc = c * cs;
    let cd = c * ds;
    let dd = d * ds;
    let m = mat3x3f(
        1.0 - cc - dd, bc - ad, bd + ac,
        bc + ad, 1 - bb - dd, cd - ab,
        bd - ac, cd + ab, 1 - bb - cc
    );
    return m * rhs;
}

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    let location = invocation_id.x;

    let grav_scales_by_linear_distance: bool = uniforms.grav_scales_by_linear_distance != 0;
    var g = uniforms.gravity_constant;
    if grav_scales_by_linear_distance {
        g *= uniforms.gravity_distance_scale;
    }
    let num_elements = uniforms.num_elements;

    if location >= num_elements {
        return;
    }

    // let dt = uniforms.dt;

    // Our point.
    var info1 = point_info[location];

    if dt == 0.0
    || info1.mass <= 0.0
    || (info1.ent_hi | info1.ent_lo) == 0
    || (info1.flags & POINT_INFO_FLAG_HAS_STATIC) != 0
    {
        point_info_new[location] = info1;
        return;
    }

    var total_force_other = vec3f(0.0, 0.0, 0.0);
    var ang_delta = vec3f(0.0, 0.0, 0.0);

    // Check every other point.
    for (var i: u32 = 0; i < num_elements; i++) {
        if i == location {
            continue;
        }

        let info2 = point_info[i];

        if info2.mass <= 0.0
        || (info2.ent_hi | info2.ent_lo) == 0
        {
            continue;
        }

        let info1c: vec3f = info1.position - info1.com;
        let info2c: vec3f = info2.position - info2.com;
        let dist_diff: vec3f = info1c - info2c;

        let dist_sq = dot(dist_diff, dist_diff);
        let dist = sqrt(dist_sq);

        // Normal case, not touching. Apply force on each toward each other.
        let mass1 = info1.mass;
        let mass2 = info2.mass;

        // Reorganize attract/repel operations to avoid floating point precision issues.
        // (The gravity constant is very small and masses are very large.
        // Two successive mass multiplications overflow f32!)
        let g_m2_over_dsq = g * mass2 / dist_sq;

        let force_type_2 = (info2.flags & POINT_INFO_FLAG_FORCE_TYPE_MASK) >> POINT_INFO_FLAG_FORCE_TYPE_SHIFT;

        switch force_type_2 {
            case POINT_INFO_FLAG_FORCE_GRAVITY: {
                if dist >= info1.radius + info2.radius {
                    // Compute instantaneous gravitational forces.
                    var dist_fac: vec3f;
                    if grav_scales_by_linear_distance {
                        // Incorrect logic (since dist_sq is handled above) but keeps the galaxy smaller.
                        dist_fac = dist_diff;
                    } else {
                        dist_fac = normalize(dist_diff);
                    }

                    let force_other = g_m2_over_dsq * dist_fac;
                    total_force_other -= force_other;
                } else if dist > 0 {
                    // Objects intersect, by definition there's no gravity force.
                    // But, they are "colliding" so we can pretend they cause each other to spin.

                    // Totally bogus computation.
                    let com_eff = (normalize(dist_diff) - info1.com) / (info1.radius + info2.radius);
                    // let d = dist / (info1.radius + info2.radius);
                    // ang_delta += dt * info2.velocity * com_eff * 3.14159;
                    ang_delta += dt * info2.velocity * (mass2 / mass1) * com_eff * 3.14159 * 0.01;

                    // Remember unique collisions.
                    if location < i {
                        let index = min(arrayLength(&colliders) - 1, atomicAdd(&collider_count, u32(1)));
                        colliders[index] = vec4u(info1.ent_lo, info1.ent_hi, info2.ent_lo, info2.ent_hi);
                    }

                    if info1.radius <= info2.radius {
                        // Bump self toward the boundary of info2.
                        // let outer_position = info2.position + normalize(dist_diff) * (info1.radius + info2.radius);
                        let outer_position = info2.position + normalize(dist_diff) * (info1.radius + info2.radius);
                        // info1.position = (info1.position * 0.9 * mass2 + outer_position * mass1 * 0.1) / (mass1 + mass2);
                        info1.position = (info1.position * 0.9 + outer_position * 0.1);

                        // let bounce_force = 0.5 * dot(info1.velocity, info2.velocity);
                        // total_force_other += dt * bounce_force / mass2;
                        // point_info_new[i].velocity -= dt * bounce_force / mass1;

                        // ang_delta += dt * bounce_force / mass1 / mass2; // bogus!
                    }
                }
            }

            case POINT_INFO_FLAG_FORCE_ATTRACT_REPEL: {
                if dist >= info1.radius + info2.radius {
                    let f = g_m2_over_dsq * normalize(dist_diff) * info1.strength;
                    total_force_other -= f;
                } else {
                    // Remember unique collisions.
                    if location < i {
                        let index = min(arrayLength(&colliders) - 1, atomicAdd(&collider_count, u32(1)));
                        colliders[index] = vec4u(info1.ent_lo, info1.ent_hi, info2.ent_lo, info2.ent_hi);
                    }

                    // Force self partially to the boundary of info2.
                    let outer_position = info2.position + normalize(dist_diff) * (info1.radius + info2.radius);
                    info1.position = (info1.position * mass1 + outer_position * mass2) / (mass1 + mass2);

                    let bounce_force = 0.5 * reflect(info1.velocity / mass2, info2.velocity / mass1);
                    total_force_other += dt * bounce_force;
                    ang_delta += dt * bounce_force; // bogus!
                    point_info_new[i].velocity -= dt * bounce_force;
                }
            }

            case POINT_INFO_FLAG_FORCE_NONE: {
            }
            default: {
            }
        }
    }

    // Update the position based on the total force.
    info1.velocity += dt * total_force_other;
    info1.position += info1.velocity * dt;
    info1.ang_vel += ang_delta;
    point_info_new[location] = info1;
}
