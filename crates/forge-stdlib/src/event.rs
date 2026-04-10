use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Modes in which the bus delivers events.
#[derive(Clone, Copy, Debug)]
pub enum EventMode {
    Sync,
    Async,
    Ordered,
}

/// Identifier returned when registering a handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventHandlerId {
    event: String,
    id: usize,
}

#[derive(Clone)]
struct Handler {
    id: usize,
    once: bool,
    callback: Arc<dyn Fn(&Arc<dyn Any + Send + Sync>) + Send + Sync>,
}

impl Handler {
    fn call(&self, event: &Arc<dyn Any + Send + Sync>) {
        (self.callback)(event);
    }
}

/// Simple event bus that routes typed events to registered listeners.
pub struct EventBus {
    mode: EventMode,
    handlers: Mutex<HashMap<String, Vec<Handler>>>,
    next_id: AtomicUsize,
}

impl EventBus {
    pub fn new(mode: EventMode) -> Self {
        Self {
            mode,
            handlers: Mutex::new(HashMap::new()),
            next_id: AtomicUsize::new(1),
        }
    }

    pub fn emit<E>(&self, event: E)
    where
        E: Any + Send + Sync + 'static,
    {
        let event_name = type_name::<E>();
        let payload: Arc<dyn Any + Send + Sync> = Arc::new(event);
        let handlers = {
            let map = self.handlers.lock().unwrap();
            map.get(event_name).cloned().unwrap_or_default()
        };

        let mut once_ids = Vec::new();

        for handler in &handlers {
            if handler.once {
                once_ids.push(handler.id);
            }
            match self.mode {
                EventMode::Async => {
                    let handler = handler.callback.clone();
                    let payload = Arc::clone(&payload);
                    thread::spawn(move || handler(&payload));
                }
                _ => {
                    handler.call(&payload);
                }
            }
        }

        if !once_ids.is_empty() {
            let mut map = self.handlers.lock().unwrap();
            if let Some(list) = map.get_mut(event_name) {
                list.retain(|h| !once_ids.contains(&h.id));
            }
        }
    }

    pub fn on<E, F>(&self, handler: F) -> EventHandlerId
    where
        E: Any + Send + Sync + 'static,
        F: Fn(&E) + Send + Sync + 'static,
    {
        self.register(handler, false, type_name::<E>())
    }

    pub fn once<E, F>(&self, handler: F) -> EventHandlerId
    where
        E: Any + Send + Sync + 'static,
        F: Fn(&E) + Send + Sync + 'static,
    {
        self.register(handler, true, type_name::<E>())
    }

    pub fn off(&self, id: &EventHandlerId) -> bool {
        let mut map = self.handlers.lock().unwrap();
        if let Some(list) = map.get_mut(&id.event) {
            let before = list.len();
            list.retain(|handler| handler.id != id.id);
            return before > list.len();
        }
        false
    }

    fn register<E, F>(&self, handler: F, once: bool, event_name: &'static str) -> EventHandlerId
    where
        E: Any + Send + Sync + 'static,
        F: Fn(&E) + Send + Sync + 'static,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let callback = Arc::new(move |event: &Arc<dyn Any + Send + Sync>| {
            if let Some(value) = event.as_ref().downcast_ref::<E>() {
                handler(value);
            }
        });

        let mut map = self.handlers.lock().unwrap();
        let entry = map.entry(event_name.to_string()).or_default();
        entry.push(Handler { id, once, callback });

        EventHandlerId {
            event: event_name.to_string(),
            id,
        }
    }
}

fn type_name<T: ?Sized>() -> &'static str {
    std::any::type_name::<T>()
}
