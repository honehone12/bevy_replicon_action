use std::{
    collections::{vec_deque::Iter, VecDeque}, 
    marker::PhantomData
};
use bevy::{
    utils::SystemTime,
    prelude::*
};
use bevy_replicon::{
    client::confirm_history::ConfirmHistory,
    server::server_tick::ServerTick, 
    prelude::*, 
};
use anyhow::bail;
use super::{network_entity::NetworkEntity, network_event::NetworkEvent};

pub struct EventSnapshot<E: NetworkEvent> {
    event: E,
    received_timestamp: f64,
    tick: u32
}

impl<E: NetworkEvent> EventSnapshot<E> {
    #[inline]
    pub fn new(event: E, received_timestamp: f64, tick: u32) -> Self {
        Self{
            event,
            received_timestamp,
            tick
        }
    }

    #[inline]
    pub fn event(&self) -> &E {
        &self.event
    }

    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    #[inline]
    pub fn received_timestamp(&self) -> f64 {
        self.received_timestamp
    }

    #[inline]
    pub fn index(&self) -> usize {
        self.event.index()
    }

    #[inline]
    pub fn timestamp(&self) -> f64 {
        self.event.timestamp()
    } 
}

#[derive(Component)]
pub struct EventSnapshots<E: NetworkEvent> {
    deq: VecDeque<EventSnapshot<E>>,
    max_size: usize,
    frontier_index: usize
}

impl<E: NetworkEvent> EventSnapshots<E> {
    #[inline]
    pub fn with_capacity(max_size: usize) -> Self {
        Self { 
            deq: VecDeque::with_capacity(max_size), 
            frontier_index: 0,
            max_size 
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.deq.len()
    }

    #[inline]
    pub fn latest_snapshot(&self) -> Option<&EventSnapshot<E>> {
        self.deq.back()
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<&EventSnapshot<E>> {
        self.deq.get(index)
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, EventSnapshot<E>> {
        self.deq.iter()
    }

    #[inline]
    pub fn sort_with_index(&mut self) {
        self.deq.make_contiguous().sort_by_key(|s| s.index());
    }

    #[inline]
    pub fn pop_front(&mut self) {
        self.deq.pop_front();
    }

    #[inline]
    pub fn insert(&mut self, event: E, tick: u32)
    -> anyhow::Result<()> {
        if self.max_size == 0 {
            bail!("zero size deque");
        }

        let received_timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs_f64();

        if event.timestamp() >= received_timestamp {
            bail!(
                "timestamp: {} is older than now: {}",
                event.timestamp(),
                received_timestamp
            );
        }

        if let Some(latest_snap) = self.latest_snapshot() {
            if tick < latest_snap.tick {
                bail!(
                    "tick: {tick} is older than latest snapshot: {}", 
                    latest_snap.tick
                );
            }

            if event.timestamp() <= latest_snap.timestamp() {
                bail!(
                    "timestamp: {} is older than latest: {}",
                    event.timestamp(),
                    latest_snap.timestamp(),
                );
            }

            debug_assert!(received_timestamp >= latest_snap.received_timestamp());
        }

        if event.index() < self.frontier_index {
            bail!(
                "event index: {} is older than frontier: {}", 
                event.index(), self.frontier_index
            );
        } 

        if self.deq.len() >= self.max_size {
            self.deq.pop_front();
        }

        self.deq.push_back(EventSnapshot::new(event, received_timestamp, tick));
        Ok(())
    }

    #[inline]
    pub fn frontier(&mut self) -> Iter<'_, EventSnapshot<E>> {
        if let Some(begin) = self.deq.iter().position(
            |e| e.index() >= self.frontier_index
        ) {
            // buffer is not empty here
            self.frontier_index = self.deq.back().unwrap().index() + 1;
            self.deq.range(begin..)
        } else {
            self.deq.range(0..0)
        }
    }
}

fn server_populate_client_event_snapshots<E: NetworkEvent>(
    mut events: EventReader<FromClient<E>>,
    mut query: Query<(&NetworkEntity, &mut EventSnapshots<E>)>,
    server_tick: Res<ServerTick>
) {
    let tick = server_tick.get();
    for FromClient { client_id, event } in events.read() {
        if let Err(e) = event.validate() {
            warn!("discarding: {e}");
            continue;
        }

        for (net_e, mut snaps) in query.iter_mut() {
            if net_e.client_id() != *client_id {
                continue;
            }

            match snaps.insert(event.clone(), tick) {
                Ok(()) => debug!(
                    "inserted event snapshot at tick: {} len: {}", 
                    tick, snaps.len()
                ),
                Err(e) => warn!("discarding: {e}")
            }
        }
    }
}

fn client_populate_client_event_snapshots<E: NetworkEvent>(
    mut query: Query<(&mut EventSnapshots<E>, &ConfirmHistory)>,
    mut events: EventReader<E>,
) {
    for event in events.read() {
        if let Err(e) = event.validate() {
            warn!("discarding: {e}");
            continue;
        }

        for (mut snaps, confirmed_tick) in query.iter_mut() {
            let tick = confirmed_tick.last_tick().get();
            match snaps.insert(event.clone(), tick) {
                Ok(()) => debug!(
                    "inserted event snapshot at tick: {} len: {}", 
                    tick, snaps.len()
                ),
                Err(e) => warn!("discarding: {e}")
            }
        }
    }
}

pub struct NetworkEventSnapshotPlugin<E: NetworkEvent>{
    pub channel_kind: ChannelKind,
    pub phantom: PhantomData<E>
}

impl<E: NetworkEvent> Plugin for NetworkEventSnapshotPlugin<E> {
    fn build(&self, app: &mut App) {
        if app.world.contains_resource::<RepliconServer>() {
            app.add_client_event::<E>(self.channel_kind)
            .add_systems(PreUpdate, 
                server_populate_client_event_snapshots::<E>
                .after(ServerSet::Receive)    
            );
        } else if app.world.contains_resource::<RepliconClient>() {
            app.add_client_event::<E>(self.channel_kind)
            .add_systems(PostUpdate, 
                client_populate_client_event_snapshots::<E>
            );
        } else {
            panic!("could not find replicon server nor client");
        }
    }
}
