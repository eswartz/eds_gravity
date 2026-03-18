use avian3d::{math::{Scalar, Vector}, prelude::{AngularVelocity, CenterOfMass, LinearVelocity, Mass, Physics}};
use bevy::{
    prelude::*, render::
        render_resource::*, shader::ShaderRef

};
use bevy_app_compute::prelude::{AppComputePlugin, AppComputeWorker, AppComputeWorkerBuilder, AppComputeWorkerPlugin, ComputeShader, ComputeWorker};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use avian3d::schedule::PhysicsTime;

use eds_bevy_common::{is_paused, states_sets::ProgramState};

use crate::game::gravity::{CollisionUpdate, ForceType, GalaxyEdits, GalaxyParams, GravityObject};
// use crate::galaxy::{report_gravity, CollisionUpdate, GalaxyEdits, GalaxyParams, GravityObject};

const WORKGROUP_SIZE: u32 = 64;

const MAX_ENTITIES: usize = 32768;

#[allow(unused)]
pub(crate) struct GravityComputePlugin;

impl Plugin for GravityComputePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(AppComputePlugin)
            .add_plugins(AppComputeWorkerPlugin::<GravityComputeWorker>::default())
            .init_resource::<Uniforms>()
            .init_resource::<GravityComputeState>()
            .add_systems(
                OnTransition{ exited: ProgramState::New, entered: ProgramState::InGame },
                reset_gravity
                // .in_set(ComputeSet)
                .run_if(not(is_paused))
            )
            .add_systems(
                PreUpdate,
                (
                    handle_outputs,
                    register_new_objects,
                    update_uniforms,
                ).chain()
                // .in_set(ComputeSet)
                .run_if(not(is_paused))
                .run_if(in_state(ProgramState::InGame))
            )
            .add_systems(
                Update,
                (
                    handle_inputs,
                    // report_gravity.run_if(|| false),
                ).chain()
                // .in_set(ComputeSet)
                .run_if(not(is_paused))
                .run_if(in_state(ProgramState::InGame))
            )
        ;
    }

}

const SHADER_ASSET_PATH: &str = "shaders/gravity_compute.wgsl";

pub(crate) const POINT_INFO_FLAG_FORCE_NONE: u32 = 0x0;
pub(crate) const POINT_INFO_FLAG_FORCE_GRAVITY: u32 = 0x1;
pub(crate) const POINT_INFO_FLAG_FORCE_ATTRACT_REPEL: u32 = 0x2;
#[allow(unused)]
pub(crate) const POINT_INFO_FLAG_FORCE_TYPE_MASK: u32 = 0x3;

pub(crate) const POINT_INFO_FLAG_HAS_STATIC: u32 = 0x80;

#[derive(TypePath)]
struct GravityComputeShader;

impl ComputeShader for GravityComputeShader {
    fn shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
    fn entry_point<'a>() -> &'a str {
        "update"
    }
}

#[derive(Resource)]
pub(crate) struct GravityComputeWorker;

impl ComputeWorker for GravityComputeWorker {
    fn build(world: &mut World) -> AppComputeWorker<Self> {
        let worker = AppComputeWorkerBuilder::new(world)
            // Add a uniform variable
            .add_uniform("uniforms", &Uniforms::default())
            .add_uniform("dt", &0.0f32)

            // Add a staging buffer, it will be available from
            // both CPU and GPU land.
            .add_staging("point_info", &vec![PointInfo::default(); MAX_ENTITIES])
            .add_staging("point_info_new", &vec![PointInfo::default(); MAX_ENTITIES])
            // .add_staging("forces", &vec![Vec4::default(); MAX_ENTITIES])
            .add_staging("colliders", &vec![UVec4::default(); MAX_ENTITIES])
            .add_staging("collider_count", &0u32)

            // Create a compute pass from your compute shader
            // and define used variables
            .add_pass::<GravityComputeShader>([(MAX_ENTITIES as u32 / WORKGROUP_SIZE) as _, 1, 1],
                &[
                    "uniforms",
                    "dt",
                    "point_info",
                    "point_info_new",
                    // "forces",
                    "colliders",
                    "collider_count",
                ])
            .add_swap("point_info", "point_info_new")
            .one_shot()
            .build();

        worker
    }
}

#[derive(Resource, Clone)]
pub struct GravityComputeState {
    generation: u64,
    prev_generation: u64,
    run_once: bool,
}

impl Default for GravityComputeState {
    fn default() -> Self {
        Self {
            run_once: true,
            generation: 0,
            prev_generation: 0,
        }
    }
}

impl GravityComputeState {
    #[track_caller]
    pub fn touch(&mut self) {
        if self.prev_generation == self.generation {
            // use std::panic::Location;
            // println!("dirty from {:?}", Location::caller());
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.run_once || self.generation != self.prev_generation
    }

    pub fn clean(&mut self) {
        self.generation = self.prev_generation;
        self.run_once = false;
    }

    pub fn is_run_once(&self) -> bool {
        self.run_once
    }

    pub fn run_once(&mut self) {
        self.run_once = true;
    }
}

/// One entry in the point_info array.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Resource, Reflect, ShaderType, Pod, Zeroable)]
#[reflect(Resource, Clone, Default)]
#[type_path = "game"]
struct PointInfo {
    position: Vec3,
    mass: f32,

    velocity: Vec3,
    radius: f32,

    ang_vel: Vec3,
    ent_hi: u32,

    center_of_mass: Vec3,
    ent_lo: u32,

    /// POINT_INFO_FLAG_xxx mask.
    flags: u32,
    strength : f32,
    _unused0 : u32,
    _unused1 : u32,
}

impl PointInfo {
    fn with_entity(self, ent: Entity) -> Self {
        Self {
            ent_hi: (ent.to_bits() >> 32) as _,
            ent_lo: ent.to_bits() as _,
            .. self
        }
    }

    fn try_get_entity(&self) -> Option<Entity> {
        let ent_bits = (self.ent_hi as u64) << 32 | self.ent_lo as u64;
        Entity::try_from_bits(ent_bits)
    }
}

#[repr(C)]
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, ShaderType, Pod, Zeroable)]
struct Uniforms {
    gravity_constant: f32,
    num_elements: u32,
    grav_scales_by_linear_distance: u32,
    gravity_distance_scale: f32,
    _unused: Vec3,
}

/// Update uniforms as world state or user settings change.
fn update_uniforms(
    mut worker: ResMut<AppComputeWorker<GravityComputeWorker>>,
    mut uniforms: ResMut<Uniforms>,
    mut generation: ResMut<GravityComputeState>,
    phys_time: Res<Time<Physics>>,
    grav_params: Res<GalaxyParams>,
    q: Query<Entity, With<GravityObject>>,
)
{
    if phys_time.is_paused() {
        return;
    }

    let cur_uniforms = Uniforms {
        gravity_constant: grav_params.gravity,
        grav_scales_by_linear_distance: grav_params.grav_scales_by_linear_distance as u32,
        gravity_distance_scale: grav_params.gravity_distance_scale,
        num_elements: q.iter().len() as _,
        _unused: default(),
    };

    if uniforms.set_if_neq(cur_uniforms) {
        debug!("new uniforms {uniforms:?}");
        worker.write("uniforms", &cur_uniforms);
        generation.touch();
    }
}

/// When a new object is created, write its results into the compute state.
fn register_new_objects(
    mut edits: ResMut<GalaxyEdits>,
    q: Query<Entity, Added<GravityObject>>,
)
{
    for ent in q.iter() {
        edits.edited.insert(ent);
    }
}

fn reset_gravity(world: &mut World) {
    let worker = GravityComputeWorker::build(world);
    world.insert_resource(worker);
}

/// Process the changes made by the previous compute worker step.
///
fn handle_outputs(
    mut worker: ResMut<AppComputeWorker<GravityComputeWorker>>,
    phys_time: Res<Time<Physics>>,
    uniforms: Res<Uniforms>,
    edits: Res<GalaxyEdits>,
    galaxy_params: Res<GalaxyParams>,
    mut phys_q: Query<(
        &mut Transform,
        &mut LinearVelocity,
        &mut AngularVelocity,
        // &mut ExternalForce,
        // &CenterOfMass,
        // &Mass,
        &GravityObject,
    ), With<GravityObject>>,
    mut coll_writer: MessageWriter<CollisionUpdate>,
) {
    if phys_time.is_paused() || !galaxy_params.enable_gravity {
        return;
    }

    if !worker.ready() {
        debug!("not ready");
        return;
    }

    // Apply data from previous run.
    let points = worker.read_vec::<PointInfo>("point_info_new");

    let mut moved = 0;
    let mut edited = 0;

    for point in points.iter() {
        let Some(ent) = point.try_get_entity() else {
            // log::warn!("out at {index}");
            break
        };
        if edits.edited.contains(&ent) {
            edited += 1;
            continue;
        }
        let Ok((mut xfrm, mut vel, mut ang_vel, _)) = phys_q.get_mut(ent) else {
            // May have just been deleted.
            warn!("unknown entity {ent}");
            continue;
        };
        if point.position.is_finite() && point.velocity.is_finite() {
            if point.velocity != Vec3::ZERO {
                moved += 1;
            }
            xfrm.translation = point.position;
            vel.0 = point.velocity;
        } else {
            // dbg!(index, point);
            vel.0 = Vector::ZERO;
        }
        if point.ang_vel.is_finite() {
            ang_vel.0 = point.ang_vel;
            // Apply here to avoid "editing"
            if ang_vel.0.length() > galaxy_params.max_spin as Scalar {
                ang_vel.0 = ang_vel.0 * galaxy_params.spin_decay as Scalar;
            }
        } else {
            // dbg!(index, point);
            ang_vel.0 = Vector::ZERO;
        }
    }
    if edited > 0 {
        info!("edited {edited} of {}", uniforms.num_elements);
    }
    if moved < uniforms.num_elements {
        info!("moved {moved} of {}", uniforms.num_elements);
    }

    let collisions = worker.read_vec::<UVec4>("colliders");
    let collision_count = worker.read::<u32>("collider_count");

    for collv in collisions.iter().take(collision_count as usize) {
        let Some(ent1) = Entity::try_from_bits(collv.x as u64 | (collv.y as u64) << 32) else { continue };
        let Some(ent2) = Entity::try_from_bits(collv.z as u64 | (collv.w as u64) << 32) else { continue };
        coll_writer.write(CollisionUpdate(ent1, ent2));
    }
    if collision_count > 0 {
        debug!("handled {collision_count} collisions");
        worker.write("collider_count", &0u32);
    }
}


/// Track changes in the world (should be rare except for new entities,
/// user-moved entities, or colliders) and update point_info[_new] if needed.
fn handle_inputs(
    mut worker: ResMut<AppComputeWorker<GravityComputeWorker>>,
    phys_time: Res<Time<Physics>>,
    galaxy_params: Res<GalaxyParams>,
    mut generation: ResMut<GravityComputeState>,
    mut edits: ResMut<GalaxyEdits>,
    phys_q: Query<(
        Entity,
        &Transform,
        &LinearVelocity,
        &AngularVelocity,
        &Mass,
        &CenterOfMass,
        &GravityObject,
    )>,
)
{
    // If something changed outside the gravity simulation
    // (as detected from the CPU side and reflected in a change to
    // the generation count), recreate the point info array.
    let something_changed = generation.is_dirty() || !edits.edited.is_empty();
    if !worker.ready() && !something_changed {
        return
    }

    // Copy before it might be cleared.
    let run_once = generation.is_run_once();

    if worker.ready() && something_changed {
        let mut points = worker.read_vec::<PointInfo>("point_info");

        // Gather data from previous run, so we can overwrite edited bits and rewrite the whole blob.
        let mut point_map = HashMap::new();
        let mut removed = 0;
        for point in &points {
            // Was the point assigned? (All entities up til the end are.)
            let Some(ent) = point.try_get_entity() else { break };
            // Does its entity still exist?
            let Ok(_) = phys_q.get(ent) else {
                removed += 1;
                continue
            };
            point_map.insert(ent, *point);
        }

        if removed > 0 {
            debug!("removed {removed}");
        }

        // Synchronize points to world if:
        // 1) the point is new.
        // 2) there was a user edit.
        // 3) a collision is occurring, so make sure we honor the physics collisions
        // to hopefully minimize the chance of phasing through the world
        let mut synced = 0;
        for (ent, xfrm, vel, ang, mass, com, obj) in phys_q.iter() {
            if !point_map.contains_key(&ent) || edits.edited.contains(&ent) || obj.is_colliding {
                let (mut flags, strength) = match obj.force_type {
                    ForceType::None => (POINT_INFO_FLAG_FORCE_NONE, 0.0),
                    ForceType::Attract => (POINT_INFO_FLAG_FORCE_ATTRACT_REPEL, 1.0),
                    ForceType::Repel => (POINT_INFO_FLAG_FORCE_ATTRACT_REPEL, -1.0),
                    ForceType::Gravity => (POINT_INFO_FLAG_FORCE_GRAVITY, 1.0),
                };
                if obj.is_static {
                    flags |= POINT_INFO_FLAG_HAS_STATIC;
                }
                let point = PointInfo {
                    position: xfrm.translation,
                    mass: mass.0,
                    velocity: vel.0, //.as_vec3(),
                    radius: obj.radius,
                    ang_vel: ang.0, //.as_vec3(),
                    ent_hi: default(),
                    center_of_mass: com.0,
                    ent_lo: default(),
                    flags,
                    strength,
                    _unused0: default(),
                    _unused1: default(),
                }.with_entity(ent);

                if point_map.insert(ent, point).is_some() {
                    synced += 1;
                }
            }
        }
        if synced > 0 {
            debug!("synced: {synced}");
        }

        points.fill(PointInfo::default());

        // Order is random but this is not important.
        for (index, point) in point_map.values().take(MAX_ENTITIES).enumerate() {
            points[index] = *point;
        }

        worker.write_slice("point_info", &points);
        worker.write_slice("point_info_new", &points);

        // Mark updated.
        generation.clean();
        edits.edited.clear();
    }

    if run_once || (!phys_time.is_paused() && galaxy_params.enable_gravity) {
        let dt = phys_time.delta_secs();
        worker.write("dt", &dt);
        // println!("run {run_once} with dt = {dt} when paused {}", time.is_paused());
        worker.execute();
    }
}
