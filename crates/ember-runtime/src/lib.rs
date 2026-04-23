pub mod app;
pub mod ecs;
pub mod input;
pub mod physics;
pub mod render;

#[cfg(feature = "native")]
pub mod native;

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod wasm;

pub use app::{App, CollisionSystem, Ember, EmberConfig, System};
pub use ecs::{Component, EntityBuilder, EntityId, Time, World};
pub use input::{InputState, Key};
pub use physics::{
    physics_step, Collider, ColliderShape, CollisionEvent, DynamicBody, EventQueue, PhysicsWorld,
    StaticBody, Vec2,
};
pub use render::{
    draw_circles, draw_rects, draw_texts, Circle, Color, DrawQueue, Position, Rect, Renderer2D,
    Text,
};
