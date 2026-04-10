use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

use forge_stdlib::event::{EventBus, EventHandlerId, EventMode};

#[test]
fn test_event_bus_register_and_emit() {
    let bus = EventBus::new(EventMode::Sync);
    let state = Arc::new(Mutex::new(Vec::new()));
    let state_clone = Arc::clone(&state);
    bus.on(move |value: &usize| {
        let mut guard = state_clone.lock().unwrap();
        guard.push(*value);
    });
    bus.emit(5usize);
    let guard = state.lock().unwrap();
    assert_eq!(guard.as_slice(), &[5]);
}

#[test]
fn test_event_bus_once_runs_once() {
    let bus = EventBus::new(EventMode::Sync);
    let counter = Arc::new(Mutex::new(0));
    let id = bus.once({
        let counter = Arc::clone(&counter);
        move |_: &usize| {
            let mut guard = counter.lock().unwrap();
            *guard += 1;
        }
    });
    bus.emit(1usize);
    bus.emit(2usize);
    assert!(!bus.off(&id));
    assert_eq!(*counter.lock().unwrap(), 1);
}

#[test]
fn test_event_bus_off_unsubscribes() {
    let bus = EventBus::new(EventMode::Sync);
    let counter = Arc::new(Mutex::new(0));
    let handler_id = bus.on({
        let counter = Arc::clone(&counter);
        move |_: &usize| {
            let mut guard = counter.lock().unwrap();
            *guard += 1;
        }
    });
    assert!(bus.off(&handler_id));
    bus.emit(3usize);
    assert_eq!(*counter.lock().unwrap(), 0);
}

#[test]
fn test_event_bus_async_mode() {
    let bus = EventBus::new(EventMode::Async);
    let flag = Arc::new(Mutex::new(false));
    let flag_clone = Arc::clone(&flag);
    bus.on(move |_: &usize| {
        let mut guard = flag_clone.lock().unwrap();
        *guard = true;
    });
    bus.emit(42usize);
    sleep(Duration::from_millis(30));
    assert!(*flag.lock().unwrap());
}

#[test]
fn test_multiple_handlers_for_same_event() {
    let bus = EventBus::new(EventMode::Sync);
    let list = Arc::new(Mutex::new(Vec::new()));
    let list_a = Arc::clone(&list);
    let list_b = Arc::clone(&list);
    bus.on(move |value: &usize| {
        let mut guard = list_a.lock().unwrap();
        guard.push(*value);
    });
    bus.on(move |value: &usize| {
        let mut guard = list_b.lock().unwrap();
        guard.push(*value + 1);
    });
    bus.emit(1usize);
    let guard = list.lock().unwrap();
    assert_eq!(guard.as_slice(), &[1, 2]);
}

#[test]
fn test_event_does_not_cross_types() {
    let bus = EventBus::new(EventMode::Sync);
    let flag = Arc::new(Mutex::new(false));
    let flag_clone = Arc::clone(&flag);
    bus.on(move |_: &usize| {
        let mut guard = flag_clone.lock().unwrap();
        *guard = true;
    });
    bus.emit(true);
    assert!(!*flag.lock().unwrap());
}
