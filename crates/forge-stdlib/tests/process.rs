use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use forge_stdlib::process::{args, on_signal, run};
#[cfg(unix)]
use signal_hook::consts::SIGINT;
#[cfg(unix)]
use signal_hook::low_level::raise;

#[cfg(unix)]
const ECHO_CMD: (&str, &[&str]) = ("echo", &["forge-stdlib"]);

#[cfg(windows)]
const ECHO_CMD: (&str, &[&str]) = ("cmd", &["/C", "echo", "forge-stdlib"]);

#[test]
fn test_args_returns_list() {
    let values = args();
    assert!(!values.is_empty());
}

#[test]
fn test_run_echo_command() {
    let (cmd, args) = ECHO_CMD;
    let output = run(cmd, args).expect("command should succeed");
    assert!(output.contains("forge-stdlib"));
}

#[cfg(unix)]
#[test]
fn test_on_signal_sigint() {
    let flag = Arc::new(AtomicBool::new(false));
    let handler_flag = flag.clone();
    on_signal("SIGINT", move || {
        handler_flag.store(true, Ordering::SeqCst);
    })
    .expect("should register signal handler");

    raise(SIGINT).expect("raise should work");
    for _ in 0..10 {
        if flag.load(Ordering::SeqCst) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(flag.load(Ordering::SeqCst));
}
