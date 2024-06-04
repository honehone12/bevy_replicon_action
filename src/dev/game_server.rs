use bevy::{
    prelude::*, 
    utils::Uuid
};
use bevy_replicon::{
    prelude::*, 
    server::server_tick::ServerTick
};
use bevy_replicon_renet::renet::transport::NetcodeServerTransport;
use bevy_replicon_renet::renet::ClientId as RenetClientId;
use anyhow::anyhow;
use crate::{
    dev::{
        config::*,
        *
    },
    prelude::*
};

pub struct GameServerPlugin;

impl Plugin for GameServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(GameCommonPlugin)
        .add_plugins(ReplicationCullingPlugin{
            culling_threshold: DISTANCE_CULLING_THREASHOLD, 
            auto_clean: true,
            phantom: PhantomData::<NetworkTranslation2D>
        })
        .add_plugins(RelevancyPlugin(PhantomData::<PlayerGroup>))
        .add_systems(Update, (
            handle_transport_error,
            handle_server_event,
            handle_player_entity_event,
            handle_fire
        ).chain());
    }
}

fn handle_server_event(
    mut events: EventReader<ServerEvent>,
    netcode_server: Res<NetcodeServerTransport>,
) {
    for e in events.read() {
        match e {
            ServerEvent::ClientConnected { client_id } => {
                let user_data = match netcode_server.user_data(
                    RenetClientId::from_raw(client_id.get())
                ) {
                    Some(u) => u,
                    None => {
                        error(anyhow!("no user data for client: {}", client_id.get()));
                        return;
                    }
                };

                let uuid = match Uuid::from_slice(&user_data[0..16]) {
                    Ok(u) => u,
                    Err(e) => {
                        error(e.into());
                        return;
                    }
                };

                info!("client: {client_id:?} uuid: {uuid} connected");
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("client: {client_id:?} disconnected with reason: {reason}");
            }
        }
    }
}

fn handle_player_entity_event(
    mut commands: Commands,
    mut events: EventReader<PlayerEntityEvent>,
    server_tick: Res<ServerTick>,
) {
    for e in events.read() {
        if let PlayerEntityEvent::Spawned { client_id, entity } = e {
            let tick = server_tick.get();
            
            let trans_bundle = match NetworkTranslationBundle
            ::<NetworkTranslation2D>::new(
                default(),
                TranslationAxis::XZ, 
                tick, 
                DEV_MAX_UPDATE_SNAPSHOT_SIZE
            ) {
                Ok(b) => b,
                Err(e) => {
                    error(e.into());
                    return;
                }
            };
            
            let rot_bundle = match NetworkRotationBundle
            ::<NetworkAngle>::new(
                default(), 
                RotationAxis::Z,
                tick, 
                DEV_MAX_UPDATE_SNAPSHOT_SIZE
            ) {
                Ok(b) => b,
                Err(e) => {
                    error(e.into());
                    return;
                }
            };

            let movement_snaps = EventSnapshots::<NetworkMovement2D>
            ::with_capacity(DEV_MAX_UPDATE_SNAPSHOT_SIZE);

            let fire_snaps = EventSnapshots::<NetworkFire>
            ::with_capacity(DEV_MAX_SNAPSHOT_SIZE);

            let group = PlayerGroup::random();
            let group_id = group.group;

            commands.entity(*entity).insert((
                PlayerPresentation::random(),
                PlayerView,
                Culling::<NetworkTranslation2D>::default(),
                group,
                trans_bundle,
                rot_bundle,
                movement_snaps,
                fire_snaps
            ));

            info!("player: {client_id:?} spawned for group: {group_id}");
        }
    }
}

fn handle_fire(
    mut shooters: Query<(
        &NetworkEntity, 
        &mut EventSnapshots<NetworkFire>
    )>,
    query: Query<(
        &NetworkEntity, 
        &ComponentSnapshots<NetworkTranslation2D>
    )>,
) {
    for (shooter, mut fire_snaps) in shooters.iter_mut() {
        for fire in fire_snaps.frontier_ref() {
            info!(
                "player: {:?} fired at {}",
                shooter.client_id(), 
                fire.timestamp() 
            );
    
            for (net_e, snaps) in query.iter() {
                let is_shooter = net_e.client_id() == shooter.client_id();
    
                let cache = snaps.cache_ref();
                let index = match cache.iter()
                .rposition(|s| 
                    s.timestamp() <= fire.timestamp()
                ) {
                    Some(idx) => idx,
                    None => {
                        if cfg!(debug_assertions) {
                            panic!(
                                "could not find timestamp smaller than {}",
                                fire.timestamp()
                            );
                        } else {
                            warn!(
                                "could not find timestamp smaller than {}, skipping",
                                fire.timestamp()
                            );
                            continue;
                        }
                    }
                };
    
                // get by found index
                let snap = cache.get(index).unwrap();
                info!(
                    "found latest snap: shooter: {}, index: {}, timestamp: {}, translation: {}",
                    is_shooter, 
                    index, 
                    snap.timestamp(), 
                    snap.component().0
                );
            }
        }

        fire_snaps.cache();
    }
}
