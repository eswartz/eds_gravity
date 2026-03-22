use crate::assets::*;
use crate::game::Sphere;
use crate::game::Star;
use crate::game::gravity::GravityObject;
use eds_bevy_common::*;

use bevy::prelude::*;
use avian3d::prelude::*;
use rand::RngExt;

pub(crate) const ID: &str = "level0";
pub(crate) const NAME: &str = "Level 0";

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(
                OnEnter(ProgramState::New),
                register_level
            )
            .add_systems(
                OnEnter(LevelState::LevelLoaded),
                on_level_loaded
                    .run_if(is_in_level(ID))
            )
            // .add_systems(
            //     Update,
            //     decay_physics
            //         .run_if(not(is_paused))
            //         .run_if(in_state(LevelState::Playing))
            //         .run_if(is_in_level(ID))
            // )
        ;
    }
}

fn register_level(mut list: ResMut<LevelList>, maps: Res<MapAssets>) {
    list.0.push(LevelInfo {
        id: ID.to_string(),
        label: NAME.to_string(),
        scene: maps.level_0.clone()
    });
}

fn on_level_loaded(
    mut commands: Commands,
    world: Res<WorldMarkerEntity>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    const ITEM_SIZE: f32 = 0.666;
    const ITEM_MASS: f32 = 50.0 * 2.0;

    // Spawn cube stacks
    let mat = materials.add(Color::srgb(0.2, 0.7, 0.9));
    let mesh = meshes.add(Sphere::new(ITEM_SIZE / 2.0));
    // let mesh = meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE));

    #[allow(unused)]
    let cuboid_size = ITEM_SIZE * 0.95;
    #[allow(unused)]
    let cuboid_round = (ITEM_SIZE - cuboid_size) / 2.0;

    const ITEM_GAP: f32 = 0.25;
    let axis_scale = Vec3::splat(ITEM_SIZE + ITEM_GAP);

    let gravity_object = GravityObject {
        radius: ITEM_SIZE,
        density: 1e5,
        ..default()
    };
    let center = Vec3::new(-5.0, 5.0, 5.0);
    const D: i32 = 8;
    let mut rng = rand::rng();
    for x in -D..D {
        for y in 0..D*2 {
            for z in -D..D {
                let position = Vec3::new(x as f32, y as f32, z as f32) * axis_scale + center;
                commands.spawn((
                    (
                        ChildOf(world.0),
                        Name::new("SPHERE"),
                        Star,
                        Spawned,
                        CrosshairTargetable,

                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(position),
                    ),
                    (
                        gravity_object.clone(),
                        Collider::sphere(ITEM_SIZE / 2.),
                        RigidBody::Dynamic,
                        // Collider::round_cuboid(cuboid_size, cuboid_size, cuboid_size, cuboid_round),
                        Restitution::new(0.05),
                        Friction::new(0.),
                        SleepThreshold {
                            linear: 0.125,
                            angular: 0.125,
                        },
                        LinearDamping(0.1),
                        AngularDamping(0.1),
                        Mass(ITEM_MASS + rng.random_range(0.0 .. 1000.0)),
                        CenterOfMass::default(),
                        CollisionMargin(0.),
                        GravityScale(0.),
                    ),
                ));
            }
        }
    }

    // commands.insert_resource(Spawning(false));
    // commands.insert_resource(SpawnDelay(Duration::from_secs(1)));
    // commands.insert_resource(SpawnTimer(Timer::new(Duration::from_secs(1), TimerMode::Repeating)));
    // commands.insert_resource(ShakeTime(Duration::ZERO));
}

// fn decay_physics(
//     mut coll_q: Query<
//         (&LinearVelocity, &AngularVelocity, &mut LinearDamping, &mut AngularDamping),
//         (With<Spawned>, With<Sphere>, Without<Sleeping>)
//     >) {

//     coll_q.par_iter_mut().for_each(|(vel, ang, mut lin_damp, mut ang_damp)| {
//         if vel.0.length_squared() >= LIVE_VEL_LEN_SQ {
//             // Turn down damping when moving.
//             if lin_damp.0 > LIVE_LIN_DAMP {
//                 lin_damp.0 = LIVE_LIN_DAMP;
//             }
//         } else if vel.0.length_squared() < SLEEP_VEL_LEN_SQ {
//             if lin_damp.0 < SLEEP_LIN_DAMP {
//                 lin_damp.0 = SLEEP_LIN_DAMP;
//             }
//         }

//         if ang.0.length_squared() >= LIVE_ANG_LEN_SQ {
//             // Turn down damping when rotating.
//             if ang_damp.0 > LIVE_ANG_DAMP {
//                 ang_damp.0 = LIVE_ANG_DAMP;
//             }
//         } else if ang.0.length_squared() < SLEEP_ANG_LEN_SQ {
//             if ang_damp.0 < SLEEP_ANG_DAMP {
//                 ang_damp.0 = SLEEP_ANG_DAMP;
//             }
//         }
//     });
// }
