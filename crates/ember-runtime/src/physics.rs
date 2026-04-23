use std::collections::HashMap;
use std::sync::Mutex;

use rapier2d::prelude::CollisionEvent as RapierCollisionEvent;
use rapier2d::prelude::*;

use crate::ecs::{EntityId, Time, World};
use crate::render::Position;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicBody {
    pub velocity: Vec2,
    pub restitution: f32,
    pub friction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticBody {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColliderShape {
    Rect { w: f32, h: f32 },
    Circle { radius: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Collider {
    pub shape: ColliderShape,
}

impl Collider {
    pub const fn rect(w: f32, h: f32) -> Self {
        Self {
            shape: ColliderShape::Rect { w, h },
        }
    }

    pub const fn circle(radius: f32) -> Self {
        Self {
            shape: ColliderShape::Circle { radius },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionEvent {
    pub entity_a: EntityId,
    pub entity_b: EntityId,
}

#[derive(Debug)]
pub struct EventQueue<T> {
    events: Vec<T>,
}

impl<T> Default for EventQueue<T> {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl<T> EventQueue<T> {
    pub fn push(&mut self, event: T) {
        self.events.push(event);
    }

    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.events.drain(..)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

pub struct PhysicsWorld {
    gravity: Vec2,
    pipeline: PhysicsPipeline,
    integration_parameters: IntegrationParameters,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    entity_bodies: HashMap<EntityId, (RigidBodyHandle, ColliderHandle)>,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            gravity: Vec2::new(0.0, 0.0),
            pipeline: PhysicsPipeline::new(),
            integration_parameters: IntegrationParameters::default(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            entity_bodies: HashMap::new(),
        }
    }

    pub fn gravity(mut self, x: f32, y: f32) -> Self {
        self.gravity = Vec2::new(x, y);
        self
    }

    pub fn gravity_value(&self) -> Vec2 {
        self.gravity
    }

    pub fn registered_len(&self) -> usize {
        self.entity_bodies.len()
    }

    pub fn step(&mut self, world: &mut World, dt: f32) {
        self.sync_removed_bodies(world);
        self.sync_new_bodies(world);
        self.sync_static_positions(world);
        self.integration_parameters.dt = dt.max(0.0);
        let gravity = vector![self.gravity.x, self.gravity.y];
        let collider_entities = self
            .entity_bodies
            .iter()
            .map(|(entity, (_, collider))| (*collider, *entity))
            .collect::<HashMap<_, _>>();
        let event_collector = RapierEventCollector::new(&collider_entities);
        self.pipeline.step(
            &gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &event_collector,
        );
        let collision_events = event_collector.into_events();
        self.sync_dynamic_positions(world);
        if !collision_events.is_empty() {
            let queue = ensure_collision_queue(world);
            for event in collision_events {
                queue.push(event);
            }
        }
    }

    fn sync_new_bodies(&mut self, world: &mut World) {
        let dynamic = world
            .query3::<Position, Collider, DynamicBody>()
            .into_iter()
            .map(|(id, position, collider, body)| (*position, *collider, *body, id))
            .collect::<Vec<_>>();
        for (position, collider, body, id) in dynamic {
            if self.entity_bodies.contains_key(&id) {
                continue;
            }
            let rigid_body = RigidBodyBuilder::dynamic()
                .translation(vector![position.x, position.y])
                .linvel(vector![body.velocity.x, body.velocity.y])
                .build();
            let body_handle = self.rigid_body_set.insert(rigid_body);
            let collider_handle = self.collider_set.insert_with_parent(
                collider_builder(collider)
                    .restitution(body.restitution)
                    .friction(body.friction)
                    .active_events(ActiveEvents::COLLISION_EVENTS)
                    .build(),
                body_handle,
                &mut self.rigid_body_set,
            );
            self.entity_bodies
                .insert(id, (body_handle, collider_handle));
        }

        let static_bodies = world
            .query3::<Position, Collider, StaticBody>()
            .into_iter()
            .map(|(id, position, collider, _)| (*position, *collider, id))
            .collect::<Vec<_>>();
        for (position, collider, id) in static_bodies {
            if self.entity_bodies.contains_key(&id) {
                continue;
            }
            let rigid_body = RigidBodyBuilder::kinematic_position_based()
                .translation(vector![position.x, position.y])
                .build();
            let body_handle = self.rigid_body_set.insert(rigid_body);
            let collider_handle = self.collider_set.insert_with_parent(
                collider_builder(collider)
                    .restitution(1.0)
                    .friction(0.0)
                    .active_events(ActiveEvents::COLLISION_EVENTS)
                    .build(),
                body_handle,
                &mut self.rigid_body_set,
            );
            self.entity_bodies
                .insert(id, (body_handle, collider_handle));
        }
    }

    fn sync_removed_bodies(&mut self, world: &World) {
        let removed = self
            .entity_bodies
            .iter()
            .filter_map(|(entity, (body_handle, _))| {
                (!world.contains(*entity)).then_some((*entity, *body_handle))
            })
            .collect::<Vec<_>>();
        for (entity, body_handle) in removed {
            self.rigid_body_set.remove(
                body_handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
            self.entity_bodies.remove(&entity);
        }
    }

    fn sync_static_positions(&mut self, world: &mut World) {
        let positions = world
            .query2::<Position, StaticBody>()
            .into_iter()
            .map(|(id, position, _)| (id, *position))
            .collect::<Vec<_>>();
        for (id, position) in positions {
            let Some((body_handle, _)) = self.entity_bodies.get(&id).copied() else {
                continue;
            };
            let Some(body) = self.rigid_body_set.get_mut(body_handle) else {
                continue;
            };
            body.set_next_kinematic_translation(vector![position.x, position.y]);
        }
    }

    fn sync_dynamic_positions(&mut self, world: &mut World) {
        let dynamic_ids = world
            .query::<DynamicBody>()
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in dynamic_ids {
            let Some((body_handle, _)) = self.entity_bodies.get(&id).copied() else {
                continue;
            };
            let Some(body) = self.rigid_body_set.get(body_handle) else {
                continue;
            };
            let translation = body.translation();
            if let Some(position) = world.get_component_mut::<Position>(id) {
                position.x = translation.x;
                position.y = translation.y;
            }
            if let Some(dynamic) = world.get_component_mut::<DynamicBody>(id) {
                let velocity = body.linvel();
                dynamic.velocity = Vec2::new(velocity.x, velocity.y);
            }
        }
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

struct RapierEventCollector<'a> {
    collider_entities: &'a HashMap<ColliderHandle, EntityId>,
    events: Mutex<Vec<CollisionEvent>>,
}

impl<'a> RapierEventCollector<'a> {
    fn new(collider_entities: &'a HashMap<ColliderHandle, EntityId>) -> Self {
        Self {
            collider_entities,
            events: Mutex::new(Vec::new()),
        }
    }

    fn into_events(self) -> Vec<CollisionEvent> {
        self.events.into_inner().unwrap_or_default()
    }
}

impl EventHandler for RapierEventCollector<'_> {
    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        event: RapierCollisionEvent,
        _contact_pair: Option<&ContactPair>,
    ) {
        if !event.started() {
            return;
        }
        let Some(entity_a) = self.collider_entities.get(&event.collider1()).copied() else {
            return;
        };
        let Some(entity_b) = self.collider_entities.get(&event.collider2()).copied() else {
            return;
        };
        if let Ok(mut events) = self.events.lock() {
            events.push(CollisionEvent { entity_a, entity_b });
        }
    }

    fn handle_contact_force_event(
        &self,
        _dt: Real,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _contact_pair: &ContactPair,
        _total_force_magnitude: Real,
    ) {
    }
}

fn ensure_collision_queue(world: &mut World) -> &mut EventQueue<CollisionEvent> {
    if world.get_resource::<EventQueue<CollisionEvent>>().is_none() {
        world.insert_resource(EventQueue::<CollisionEvent>::default());
    }
    world.resource_mut::<EventQueue<CollisionEvent>>()
}

pub fn physics_step(world: &mut World) {
    let dt = world
        .get_resource::<Time>()
        .map(Time::delta)
        .unwrap_or(1.0 / 60.0);
    let Some(mut physics) = world.remove_resource::<PhysicsWorld>() else {
        return;
    };
    physics.step(world, dt);
    world.insert_resource(physics);
}

fn collider_builder(collider: Collider) -> ColliderBuilder {
    match collider.shape {
        ColliderShape::Rect { w, h } => ColliderBuilder::cuboid(w * 0.5, h * 0.5),
        ColliderShape::Circle { radius } => ColliderBuilder::ball(radius),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physics_world_builder_sets_gravity() {
        let physics = PhysicsWorld::new().gravity(0.0, -9.8);
        assert_eq!(physics.gravity_value(), Vec2::new(0.0, -9.8));
    }

    #[test]
    fn collider_constructors_record_shape() {
        assert_eq!(
            Collider::rect(10.0, 20.0).shape,
            ColliderShape::Rect { w: 10.0, h: 20.0 }
        );
        assert_eq!(
            Collider::circle(5.0).shape,
            ColliderShape::Circle { radius: 5.0 }
        );
    }

    #[test]
    fn physics_step_registers_and_moves_dynamic_body() {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.resource_mut::<Time>().update(0.5);
        world.insert_resource(PhysicsWorld::new().gravity(0.0, 0.0));
        let id = world
            .spawn()
            .with(Position::new(0.0, 0.0))
            .with(Collider::circle(2.0))
            .with(DynamicBody {
                velocity: Vec2::new(10.0, 0.0),
                restitution: 1.0,
                friction: 0.0,
            })
            .build();

        physics_step(&mut world);

        assert_eq!(world.resource::<PhysicsWorld>().registered_len(), 1);
        assert!(world.get_component::<Position>(id).unwrap().x > 0.0);
    }

    #[test]
    fn physics_step_queues_collision_events() {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.resource_mut::<Time>().update(1.0 / 60.0);
        world.insert_resource(PhysicsWorld::new().gravity(0.0, 0.0));
        let dynamic_id = world
            .spawn()
            .with(Position::new(0.0, 0.0))
            .with(Collider::circle(5.0))
            .with(DynamicBody {
                velocity: Vec2::new(0.0, 0.0),
                restitution: 1.0,
                friction: 0.0,
            })
            .build();
        let static_id = world
            .spawn()
            .with(Position::new(0.0, 0.0))
            .with(Collider::rect(10.0, 10.0))
            .with(StaticBody {})
            .build();

        physics_step(&mut world);

        let events = world
            .resource_mut::<EventQueue<CollisionEvent>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(events.iter().any(|event| {
            (event.entity_a == dynamic_id && event.entity_b == static_id)
                || (event.entity_a == static_id && event.entity_b == dynamic_id)
        }));
    }

    #[test]
    fn dynamic_body_reflects_from_static_body() {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(PhysicsWorld::new().gravity(0.0, 0.0));
        let dynamic_id = world
            .spawn()
            .with(Position::new(0.0, 0.0))
            .with(Collider::circle(2.0))
            .with(DynamicBody {
                velocity: Vec2::new(60.0, 0.0),
                restitution: 1.0,
                friction: 0.0,
            })
            .build();
        world
            .spawn()
            .with(Position::new(12.0, 0.0))
            .with(Collider::rect(4.0, 20.0))
            .with(StaticBody {})
            .build();

        for _ in 0..30 {
            world.resource_mut::<Time>().update(1.0 / 60.0);
            physics_step(&mut world);
        }

        assert!(
            world
                .get_component::<DynamicBody>(dynamic_id)
                .unwrap()
                .velocity
                .x
                < 0.0
        );
    }

    #[test]
    fn physics_step_removes_despawned_bodies() {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.resource_mut::<Time>().update(1.0 / 60.0);
        world.insert_resource(PhysicsWorld::new().gravity(0.0, 0.0));
        let id = world
            .spawn()
            .with(Position::new(0.0, 0.0))
            .with(Collider::circle(5.0))
            .with(DynamicBody {
                velocity: Vec2::new(0.0, 0.0),
                restitution: 1.0,
                friction: 0.0,
            })
            .build();

        physics_step(&mut world);
        assert_eq!(world.resource::<PhysicsWorld>().registered_len(), 1);

        assert!(world.despawn(id));
        physics_step(&mut world);

        assert_eq!(world.resource::<PhysicsWorld>().registered_len(), 0);
    }

    #[test]
    fn event_queue_drains_events() {
        let mut queue = EventQueue::default();
        queue.push(CollisionEvent {
            entity_a: 1,
            entity_b: 2,
        });
        assert_eq!(queue.len(), 1);
        let events = queue.drain().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert!(queue.is_empty());
    }
}
