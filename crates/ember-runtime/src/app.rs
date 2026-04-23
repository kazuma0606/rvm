#[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
use std::time::Instant;

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use web_time::Instant;

use crate::ecs::{Time, World};
use crate::input::InputState;
use crate::physics::{physics_step, CollisionEvent, EventQueue, PhysicsWorld};
use crate::render::DrawQueue;

pub type System = Box<dyn Fn(&mut World) + 'static>;
pub type CollisionSystem = Box<dyn Fn(CollisionEvent, &mut World) + 'static>;

pub struct App {
    pub world: World,
    startup_systems: Vec<System>,
    systems: Vec<System>,
    collision_systems: Vec<CollisionSystem>,
    last_tick: Instant,
    started: bool,
}

impl App {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(DrawQueue::new());
        world.insert_resource(InputState::new());
        Self {
            world,
            startup_systems: Vec::new(),
            systems: Vec::new(),
            collision_systems: Vec::new(),
            last_tick: Instant::now(),
            started: false,
        }
    }

    pub fn add_startup_system<F>(&mut self, system: F) -> &mut Self
    where
        F: Fn(&mut World) + 'static,
    {
        self.startup_systems.push(Box::new(system));
        self
    }

    pub fn add_system<F>(&mut self, system: F) -> &mut Self
    where
        F: Fn(&mut World) + 'static,
    {
        self.systems.push(Box::new(system));
        self
    }

    pub fn add_collision_system<F>(&mut self, system: F) -> &mut Self
    where
        F: Fn(CollisionEvent, &mut World) + 'static,
    {
        self.collision_systems.push(Box::new(system));
        self
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let delta = (now - self.last_tick).as_secs_f32();
        self.last_tick = now;
        self.tick_with_delta(delta);
    }

    pub fn tick_with_delta(&mut self, delta: f32) {
        if !self.started {
            for system in &self.startup_systems {
                system(&mut self.world);
            }
            self.started = true;
        }
        if let Some(time) = self.world.get_resource_mut::<Time>() {
            time.update(delta);
        }
        for system in &self.systems {
            system(&mut self.world);
        }
        self.dispatch_collision_events();
    }

    fn dispatch_collision_events(&mut self) {
        if self.collision_systems.is_empty() {
            return;
        }
        let Some(mut queue) = self.world.remove_resource::<EventQueue<CollisionEvent>>() else {
            return;
        };
        let events = queue.drain().collect::<Vec<_>>();
        self.world.insert_resource(queue);
        for event in events {
            for system in &self.collision_systems {
                system(event, &mut self.world);
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct EmberConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub background: [f64; 4],
}

impl Default for EmberConfig {
    fn default() -> Self {
        Self {
            title: "Ember".to_string(),
            width: 800,
            height: 600,
            background: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

pub struct Ember {
    config: EmberConfig,
    app: App,
}

impl Ember {
    pub fn new() -> Self {
        Self {
            config: EmberConfig::default(),
            app: App::new(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.config.title = title.into();
        self
    }

    pub fn window(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    pub fn background(mut self, r: f64, g: f64, b: f64, a: f64) -> Self {
        self.config.background = [r, g, b, a];
        self
    }

    pub fn system<F>(mut self, system: F) -> Self
    where
        F: Fn(&mut World) + 'static,
    {
        self.app.add_system(system);
        self
    }

    pub fn startup_system<F>(mut self, system: F) -> Self
    where
        F: Fn(&mut World) + 'static,
    {
        self.app.add_startup_system(system);
        self
    }

    pub fn on_collision<F>(mut self, system: F) -> Self
    where
        F: Fn(CollisionEvent, &mut World) + 'static,
    {
        self.app.add_collision_system(system);
        self
    }

    pub fn physics(mut self, physics: PhysicsWorld) -> Self {
        self.app.world.insert_resource(physics);
        self.app
            .world
            .insert_resource(EventQueue::<CollisionEvent>::default());
        self.app.add_system(physics_step);
        self
    }

    pub fn config(&self) -> &EmberConfig {
        &self.config
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    #[cfg(feature = "native")]
    pub fn run(self) {
        crate::native::run(self.config, self.app);
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm", not(feature = "native")))]
    pub fn run(self) {
        crate::wasm::run(self.config, self.app);
    }
}

impl Default for Ember {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Debug, PartialEq)]
    struct Counter(i32);

    #[test]
    fn app_tick_updates_time_and_runs_systems_in_order() {
        let calls = Rc::new(RefCell::new(Vec::<i32>::new()));
        let mut app = App::new();
        app.world.insert_resource(Counter(0));

        let startup_calls = Rc::clone(&calls);
        app.add_startup_system(move |world| {
            world.resource_mut::<Counter>().0 += 100;
            startup_calls.borrow_mut().push(0);
        });

        let first_calls = Rc::clone(&calls);
        app.add_system(move |world| {
            world.resource_mut::<Counter>().0 += 1;
            first_calls.borrow_mut().push(1);
        });

        let second_calls = Rc::clone(&calls);
        app.add_system(move |world| {
            world.resource_mut::<Counter>().0 *= 10;
            second_calls.borrow_mut().push(2);
        });

        app.tick_with_delta(0.25);
        app.tick_with_delta(0.25);

        assert_eq!(app.world.resource::<Counter>().0, 10110);
        assert_eq!(calls.borrow().as_slice(), &[0, 1, 2, 1, 2]);
        assert_eq!(app.world.resource::<Time>().delta(), 0.25);
        assert_eq!(app.world.resource::<Time>().fps(), 4.0);
    }

    #[test]
    fn ember_builder_stores_window_options() {
        let ember = Ember::new()
            .title("Breakout")
            .window(1024, 768)
            .background(0.1, 0.2, 0.3, 1.0);

        assert_eq!(ember.config().title, "Breakout");
        assert_eq!(ember.config().width, 1024);
        assert_eq!(ember.config().height, 768);
        assert_eq!(ember.config().background, [0.1, 0.2, 0.3, 1.0]);
    }

    #[test]
    fn ember_builder_registers_physics_world() {
        let ember = Ember::new().physics(PhysicsWorld::new().gravity(1.0, 2.0));

        assert_eq!(
            ember.app().world.resource::<PhysicsWorld>().gravity_value(),
            crate::physics::Vec2::new(1.0, 2.0)
        );
    }

    #[test]
    fn app_dispatches_collision_events_to_registered_systems() {
        let seen = Rc::new(RefCell::new(Vec::<CollisionEvent>::new()));
        let mut app = App::new();
        app.world
            .insert_resource(EventQueue::<CollisionEvent>::default());
        app.world
            .resource_mut::<EventQueue<CollisionEvent>>()
            .push(CollisionEvent {
                entity_a: 1,
                entity_b: 2,
            });

        let seen_events = Rc::clone(&seen);
        app.add_collision_system(move |event, world| {
            seen_events.borrow_mut().push(event);
            world.insert_resource(Counter(42));
        });

        app.tick_with_delta(1.0 / 60.0);

        assert_eq!(
            seen.borrow().as_slice(),
            &[CollisionEvent {
                entity_a: 1,
                entity_b: 2
            }]
        );
        assert_eq!(app.world.resource::<Counter>().0, 42);
        assert!(app
            .world
            .resource::<EventQueue<CollisionEvent>>()
            .is_empty());
    }
}
