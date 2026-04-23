use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};

pub type EntityId = u64;

pub trait Component: Any + 'static {}

impl<T: Any + 'static> Component for T {}

#[derive(Debug, Clone, Copy)]
pub struct Time {
    delta: f32,
    elapsed: f32,
    fps: f32,
}

impl Time {
    pub fn delta(&self) -> f32 {
        self.delta
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub fn update(&mut self, delta: f32) {
        let delta = delta.max(0.0);
        self.delta = delta;
        self.elapsed += delta;
        self.fps = if delta > 0.0 { 1.0 / delta } else { 0.0 };
    }
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: 0.0,
            elapsed: 0.0,
            fps: 0.0,
        }
    }
}

#[derive(Default)]
pub struct World {
    next_id: EntityId,
    entities: HashSet<EntityId>,
    components: HashMap<TypeId, HashMap<EntityId, Box<dyn Any + 'static>>>,
    resources: HashMap<TypeId, Box<dyn Any + 'static>>,
}

impl World {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entities: HashSet::new(),
            components: HashMap::new(),
            resources: HashMap::new(),
        }
    }

    pub fn spawn(&mut self) -> EntityBuilder<'_> {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.insert(id);
        EntityBuilder { id, world: self }
    }

    pub fn despawn(&mut self, id: EntityId) -> bool {
        let existed = self.entities.remove(&id);
        for storage in self.components.values_mut() {
            storage.remove(&id);
        }
        existed
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.entities.contains(&id)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn insert_component<C: Component>(&mut self, id: EntityId, component: C) {
        assert!(
            self.entities.contains(&id),
            "cannot insert component for missing entity {id}"
        );
        self.components
            .entry(TypeId::of::<C>())
            .or_default()
            .insert(id, Box::new(component));
    }

    pub fn has_component<C: Component>(&self, id: EntityId) -> bool {
        self.components
            .get(&TypeId::of::<C>())
            .and_then(|storage| storage.get(&id))
            .is_some()
    }

    pub fn get_component<C: Component>(&self, id: EntityId) -> Option<&C> {
        self.components
            .get(&TypeId::of::<C>())?
            .get(&id)?
            .downcast_ref::<C>()
    }

    pub fn get_component_mut<C: Component>(&mut self, id: EntityId) -> Option<&mut C> {
        self.components
            .get_mut(&TypeId::of::<C>())?
            .get_mut(&id)?
            .downcast_mut::<C>()
    }

    pub fn query<C: Component>(&mut self) -> Vec<(EntityId, &mut C)> {
        self.components
            .get_mut(&TypeId::of::<C>())
            .map(|storage| {
                storage
                    .iter_mut()
                    .filter_map(|(id, boxed)| boxed.downcast_mut::<C>().map(|c| (*id, c)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn query2<A: Component, B: Component>(&mut self) -> Vec<(EntityId, &mut A, &mut B)> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "query2 requires distinct component types"
        );
        let a_type = TypeId::of::<A>();
        let b_type = TypeId::of::<B>();
        let Some(a_storage_ptr) = self.components.get_mut(&a_type).map(|s| s as *mut _) else {
            return Vec::new();
        };
        let Some(b_storage_ptr) = self.components.get_mut(&b_type).map(|s| s as *mut _) else {
            return Vec::new();
        };

        // SAFETY: A and B TypeId values are distinct, so these pointers refer to
        // different component storages inside the same HashMap.
        unsafe {
            let a_storage: &mut HashMap<EntityId, Box<dyn Any + 'static>> = &mut *a_storage_ptr;
            let b_storage: &mut HashMap<EntityId, Box<dyn Any + 'static>> = &mut *b_storage_ptr;
            let ids = a_storage
                .keys()
                .filter(|id| b_storage.contains_key(id))
                .copied()
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| {
                    let a = a_storage.get_mut(&id)?.downcast_mut::<A>()? as *mut A;
                    let b = b_storage.get_mut(&id)?.downcast_mut::<B>()? as *mut B;
                    Some((id, &mut *a, &mut *b))
                })
                .collect()
        }
    }

    pub fn query3<A: Component, B: Component, C: Component>(
        &mut self,
    ) -> Vec<(EntityId, &mut A, &mut B, &mut C)> {
        let a_type = TypeId::of::<A>();
        let b_type = TypeId::of::<B>();
        let c_type = TypeId::of::<C>();
        assert!(
            a_type != b_type && a_type != c_type && b_type != c_type,
            "query3 requires distinct component types"
        );
        let Some(a_storage_ptr) = self.components.get_mut(&a_type).map(|s| s as *mut _) else {
            return Vec::new();
        };
        let Some(b_storage_ptr) = self.components.get_mut(&b_type).map(|s| s as *mut _) else {
            return Vec::new();
        };
        let Some(c_storage_ptr) = self.components.get_mut(&c_type).map(|s| s as *mut _) else {
            return Vec::new();
        };

        // SAFETY: A, B, and C TypeId values are distinct, so these pointers refer
        // to different component storages inside the same HashMap.
        unsafe {
            let a_storage: &mut HashMap<EntityId, Box<dyn Any + 'static>> = &mut *a_storage_ptr;
            let b_storage: &mut HashMap<EntityId, Box<dyn Any + 'static>> = &mut *b_storage_ptr;
            let c_storage: &mut HashMap<EntityId, Box<dyn Any + 'static>> = &mut *c_storage_ptr;
            let ids = a_storage
                .keys()
                .filter(|id| b_storage.contains_key(id) && c_storage.contains_key(id))
                .copied()
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| {
                    let a = a_storage.get_mut(&id)?.downcast_mut::<A>()? as *mut A;
                    let b = b_storage.get_mut(&id)?.downcast_mut::<B>()? as *mut B;
                    let c = c_storage.get_mut(&id)?.downcast_mut::<C>()? as *mut C;
                    Some((id, &mut *a, &mut *b, &mut *c))
                })
                .collect()
        }
    }

    pub fn query_single<C: Component>(&mut self) -> Option<(EntityId, &mut C)> {
        self.query::<C>().into_iter().next()
    }

    pub fn insert_resource<R: Any + 'static>(&mut self, resource: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(resource));
    }

    pub fn resource<R: Any + 'static>(&self) -> &R {
        self.get_resource::<R>()
            .unwrap_or_else(|| panic!("missing resource {}", std::any::type_name::<R>()))
    }

    pub fn resource_mut<R: Any + 'static>(&mut self) -> &mut R {
        self.get_resource_mut::<R>()
            .unwrap_or_else(|| panic!("missing resource {}", std::any::type_name::<R>()))
    }

    pub fn get_resource<R: Any + 'static>(&self) -> Option<&R> {
        self.resources.get(&TypeId::of::<R>())?.downcast_ref::<R>()
    }

    pub fn get_resource_mut<R: Any + 'static>(&mut self) -> Option<&mut R> {
        self.resources
            .get_mut(&TypeId::of::<R>())?
            .downcast_mut::<R>()
    }

    pub fn remove_resource<R: Any + 'static>(&mut self) -> Option<R> {
        self.resources
            .remove(&TypeId::of::<R>())?
            .downcast::<R>()
            .ok()
            .map(|boxed| *boxed)
    }
}

pub struct EntityBuilder<'w> {
    id: EntityId,
    world: &'w mut World,
}

impl<'w> EntityBuilder<'w> {
    pub fn with<C: Component>(self, component: C) -> Self {
        self.world.insert_component(self.id, component);
        self
    }

    pub fn build(self) -> EntityId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Debug, PartialEq)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[derive(Debug, PartialEq)]
    struct Health(i32);

    #[test]
    fn spawn_with_components_and_despawn() {
        let mut world = World::new();
        let id = world
            .spawn()
            .with(Position { x: 1.0, y: 2.0 })
            .with(Velocity { dx: 3.0, dy: 4.0 })
            .build();

        assert!(world.contains(id));
        assert!(world.has_component::<Position>(id));
        assert!(world.has_component::<Velocity>(id));
        assert_eq!(world.get_component::<Position>(id).unwrap().x, 1.0);
        assert!(world.despawn(id));
        assert!(!world.contains(id));
        assert!(!world.has_component::<Position>(id));
    }

    #[test]
    fn query_components_mutably() {
        let mut world = World::new();
        world
            .spawn()
            .with(Position { x: 0.0, y: 0.0 })
            .with(Velocity { dx: 1.0, dy: 2.0 })
            .build();
        world.spawn().with(Position { x: 10.0, y: 10.0 }).build();

        for (_, pos, vel) in world.query2::<Position, Velocity>() {
            pos.x += vel.dx;
            pos.y += vel.dy;
        }

        let moved = world
            .query::<Position>()
            .into_iter()
            .any(|(_, pos)| pos.x == 1.0 && pos.y == 2.0);
        assert!(moved);
    }

    #[test]
    fn query3_and_single_work() {
        let mut world = World::new();
        let id = world
            .spawn()
            .with(Position { x: 0.0, y: 0.0 })
            .with(Velocity { dx: 1.0, dy: 2.0 })
            .with(Health(5))
            .build();

        let rows = world.query3::<Position, Velocity, Health>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, id);

        let (single_id, health) = world.query_single::<Health>().unwrap();
        assert_eq!(single_id, id);
        health.0 -= 1;
        assert_eq!(world.get_component::<Health>(id).unwrap().0, 4);
    }

    #[test]
    fn resources_and_time_update() {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.resource_mut::<Time>().update(0.5);

        let time = world.resource::<Time>();
        assert_eq!(time.delta(), 0.5);
        assert_eq!(time.elapsed(), 0.5);
        assert_eq!(time.fps(), 2.0);
    }
}
