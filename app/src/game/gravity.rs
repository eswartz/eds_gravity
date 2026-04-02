use std::collections::VecDeque;

use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;
use strum::EnumIter;
use strum::Display;
use strum::EnumString;
use strum::IntoStaticStr;
use strum::VariantArray;

use crate::game::gravity_compute::GravityComputePlugin;

const ORBIT_POINTS: usize = 1024;

pub struct GravityPlugin;
impl Plugin for GravityPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GalaxyParams>()
            .register_type::<GravityObject>()
            .init_resource::<GalaxyParams>()
            .init_resource::<GalaxyEdits>()
            .add_message::<CollisionUpdate>()
            .insert_resource(GalaxyState::with_compute(true))
            .add_plugins(GravityComputePlugin)
            // .add_systems(Update,
            //     (damp_distance, damp_spin)
            //         .in_set(SimulationSet)
            //         .run_if(in_state(ProgramState::InGame))
            // )
            // .add_systems(
            //     Update,
            //     (
            //         simplify_orbits,
            //         track_orbits.run_if(|time: Res<Time<Physics>>| !time.is_paused()),
            //     )
            //     .chain()
            //     .after(report_gravity)
            //     .in_set(ComputeSet)
            //     .run_if(in_state(ProgramState::InGame))
            //     ,
            // )
            // .add_systems(
            //     Update,
            //     (
            //         draw_orbits.run_if(|gparams: Res<GalaxyParams>| gparams.draw_orbits),
            //         draw_center_of_mass
            //             .run_if(|gparams: Res<GalaxyParams>| gparams.draw_center_of_mass),
            //         update_state_stats,
            //     )
            //     .in_set(SimulationSet)
            //     .run_if(in_state(ProgramState::InGame)),
            // )
            ;
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect, EnumIter, EnumString, Display, IntoStaticStr, VariantArray)]
#[type_path = "game"]
pub enum ForceType {
    None,
    Gravity,
    Attract,
    Repel,
}


pub fn get_mass(radius: f32, density: f32) -> f32 {
    density * (4.0 / 3.0) * std::f32::consts::PI * radius.powf(3.0)
}

pub fn get_radius_for_mass(mass: f32, density: f32) -> f32 {
    (mass / (density * (4.0 / 3.0) * std::f32::consts::PI)).powf(1.0 / 3.0)
}

pub fn get_density_for_radius(mass: f32, radius: f32) -> f32 {
    mass / ((4.0 / 3.0) * std::f32::consts::PI * radius.powf(3.0))
}

pub fn get_stock_radius_density_for_mass_pow10(mass_p10: f32) -> (f32, f32) {
    // Adjust density and radius to match.
    let mass = 10.0f32.powf(mass_p10);
    let radius = (mass_p10 - 20.).max(0.1) * 3.0;
    (radius, get_density_for_radius(mass, radius))
}

/// When this exists, apply an orbit simplification pass.
#[derive(Resource, Default)]
pub struct SimplifyOrbitsRequest;

#[derive(Resource, PartialEq, Debug, Clone, Default)]
pub struct GalaxyState {
    pub using_compute: bool,
    com: Vec3,
    total_mass: f32,
    total_momentum: f64,
    num_objects: usize,
    num_selected: usize,
}

impl GalaxyState {
    fn with_compute(using_compute: bool) -> Self {
        Self {
            using_compute,
            com: Vec3::default(),
            total_mass: 0.0,
            total_momentum: 0.0,
            num_objects: 0,
            num_selected: 0,
        }
    }
    pub fn number_of_objects(&self) -> usize {
        self.num_objects
    }
    pub fn number_selected(&self) -> usize {
        self.num_selected
    }
    pub fn total_mass(&self) -> f32 {
        self.total_mass
    }
    pub fn total_momentum(&self) -> f64 {
        self.total_momentum
    }
    pub fn center_of_mass(&self) -> Vec3 {
        self.com
    }
}

/// Mark entities which were manually edited in the last frame,
/// to avoid losing edits from the gravity compute shader.
#[derive(Clone, Resource, PartialEq, Default, Reflect)]
#[reflect(Resource)]
#[type_path = "game"]
pub struct GalaxyEdits {
    pub edited: EntityHashSet,
}

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Clone, Default)]
#[type_path = "game"]
pub struct GalaxyParams {
    pub enable_gravity: bool,

    /// Gravity constant.
    pub gravity: f32,
    /// If true, use m¹ instead of m² to scale gravity.
    pub grav_scales_by_linear_distance: bool,
    /// Adjustment to gravity constant when `grav_scales_by_linear_distance` is true.
    pub gravity_distance_scale: f32,

    pub draw_center_of_mass: bool,
    pub draw_orbits: bool,
    pub draw_orbit_points: bool,

    pub orbit_history_size: usize,
    pub extend_orbit_lifetime: bool,
    pub draw_orbit_particles: bool,
    pub max_orbit_particle_emitters: u16,

    pub max_distance: f32,
    pub distance_decay: f32,
    pub max_spin: f32,
    pub spin_decay: f32,
}

impl Default for GalaxyParams {
    fn default() -> Self {
        Self {
            enable_gravity: true,
            // gravity: 6.674e-11 * 1e-9, // 6.674×10−11 m³⋅kg−1⋅s−2
            // gravity: 6.674e-11, // 6.674×10−11 m³⋅kg−1⋅s−2
            gravity: 1e-5,
            grav_scales_by_linear_distance: false,
            gravity_distance_scale: 1e-2,

            draw_center_of_mass: false,
            draw_orbits: true,
            draw_orbit_points: false,

            extend_orbit_lifetime: true,
            orbit_history_size: ORBIT_POINTS,

            draw_orbit_particles: true,
            max_orbit_particle_emitters: 64,

            max_distance: 1e5,
            distance_decay: 0.9,
            max_spin: std::f32::consts::FRAC_PI_2,
            spin_decay: 0.999,
        }
    }
}

#[derive(Message)]
pub(crate) struct CollisionUpdate(pub Entity, pub Entity);

/// This represents something in the "galaxy" which is affected by gravity.
#[derive(Component, Reflect, Clone)]
#[reflect(Component, Clone, Default)]
#[type_path = "game"]
// #[require(Saveable)]
pub struct GravityObject {
    pub radius: f32,
    pub density: f32,
    pub color: Color,
    /// Does the object have a non-trivial collider?
    /// If so, no need to manually check collisions (the physics system will).
    pub with_collider: bool,
    pub orbit_points: VecDeque<Vec3>,
    /// Accumulation of distances between orbit points,
    /// used when recomputing / compressing points.
    pub orbit_travel_distance: f32,
    /// How the object influences others in the simulation.
    pub force_type: ForceType,
    /// If set, object is immobile but can induce forces on others.
    pub is_static: bool,

    /// The entity is currently colliding, will be reset on OnCollisionEnd
    pub is_colliding: bool,
    /// The entity is about to be deleted; don't consider it in further tests.
    pub dead: bool,
}

impl Default for GravityObject {
    fn default() -> Self {
        Self {
            radius: 1.0,
            density: 1.0,
            color: default(),
            with_collider: false,
            orbit_points: default(),
            orbit_travel_distance: 0.0,
            force_type: ForceType::Gravity,
            is_static: false,
            is_colliding: false,
            dead: false,
        }
    }
}
