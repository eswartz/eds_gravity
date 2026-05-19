

use eds_bevy_common::*;
use bevy::prelude::*;
#[cfg(feature = "input_bei")]
use bevy_enhanced_input::prelude::*;

#[cfg(feature = "input_bei")]
pub fn assign_extra_actions(
    mut commands: Commands,
    include: impl Bundle + Clone,
) {
    commands.spawn((
        include.clone(),
        Action::<actions::ChangeCamera>::new(),
        bindings![
            KeyCode::KeyV.with_mod_keys(MOD_CTRL_COMMAND),
        ],
    ));
}
