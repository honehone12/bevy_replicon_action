pub mod config;
pub mod level;
pub mod game_client;
pub mod game_server;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::renet::transport::NetcodeTransportError;
use serde::{Serialize, Deserialize};
use rand::prelude::*;
use crate::prelude::*;
use config::*;

pub struct GameCommonPlugin;

impl Plugin for GameCommonPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RepliconActionPlugin)
        .use_network_transform_2d(
            TranslationAxis::XZ,
            NetworkTransformUpdateFns::new(move_2d),
            PlayerMovementParams{
                base_speed: BASE_SPEED
            },
            NetworkTransformInterpolationConfig{
                network_tick_delta: DEV_NETWORK_TICK_DELTA64 
            },
            PredictionErrorThresholdConfig{
                translation_error_threshold: TRANSLATION_ERROR_THRESHOLD,
                prediction_error_count_threshold: PREDICTION_ERROR_COUNT_THRESHOLD
            }
        )
        .use_component_snapshot::<NetworkTranslation2D>()
        .use_component_snapshot::<NetworkYaw>()
        .use_replication_culling::<NetworkTranslation2D>(
            CullingConfig{
                culling_threshold: DISTANCE_CULLING_THREASHOLD,
                clean_up_on_disconnect: true
            }
        )
        .use_relevancy::<PlayerGroup>()
        .add_client_event::<NetworkFire>(ChannelKind::Ordered)
        .replicate::<PlayerPresentation>()
        .replicate::<PlayerGroup>();
    }
}

#[derive(Component, Serialize, Deserialize)]
pub struct PlayerPresentation {
    pub color: Color
}

impl PlayerPresentation {
    #[inline]
    pub fn random() -> Self {
        Self{
            color: Color::rgb(
                random(), 
                random(), 
                random()
            )
        }
    }
}

#[derive(Component, Serialize, Deserialize, Default)]
pub struct PlayerGroup {
    pub group: u8
}

impl PlayerGroup {
    #[inline]
    pub fn random() -> Self {
        let group = if random() {
            1
        } else {
            0
        };
        Self { group }
    }
}

impl RelevantGroup for PlayerGroup {
    fn is_relevant(&self, rhs: &Self) -> bool {
        self.group == rhs.group
    }
}

#[derive(Resource)]
pub struct PlayerMovementParams {
    pub base_speed: f32
}

#[derive(Event, Serialize, Deserialize, Clone)]
pub struct NetworkFire {
    pub index: usize,
    pub timestamp: f64
}

impl NetworkEvent for NetworkFire {
    fn index(&self) -> usize {
        self.index
    }

    fn timestamp(&self) -> f64 {
        self.timestamp
    }
}

pub fn move_2d(
    translation: &mut NetworkTranslation2D,
    movement: &NetworkMovement2D,
    params: &PlayerMovementParams,
    time: &Time<Fixed>
) {
    let mut dir = movement.linear_axis.normalize();
    dir.y *= -1.0;
    translation.0 += dir * (params.base_speed * time.delta_seconds())
}

pub fn handle_transport_error(mut errors: EventReader<NetcodeTransportError>) {
    for e in errors.read() {
        panic!("{e}")
    }
}

pub fn error(error: anyhow::Error) {
    panic!("{error}");
}
