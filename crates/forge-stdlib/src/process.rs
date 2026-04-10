use std::convert::TryFrom;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

pub fn args() -> Vec<String> {
    std::env::args().collect()
}

pub fn exit(code: i64) -> ! {
    let normalized =
        i32::try_from(code).unwrap_or_else(|_| if code < 0 { i32::MIN } else { i32::MAX });
    std::process::exit(normalized)
}

pub fn run<C, A>(cmd: C, args: A) -> Result<String, String>
where
    C: AsRef<str>,
    A: IntoIterator,
    A::Item: AsRef<str>,
{
    let command_name = cmd.as_ref().to_string();
    let mut command = Command::new(&command_name);
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
    command.args(&args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = command
        .output()
        .map_err(|err| format!("failed to run '{}': {}", command_name, err))?;

    if !output.status.success() {
        return Err(format!(
            "command '{}' failed with exit status {}",
            command_name, output.status
        ));
    }

    String::from_utf8(output.stdout).map_err(|err| {
        format!(
            "command '{}' produced non-utf8 output: {}",
            command_name, err
        )
    })
}

#[cfg(unix)]
pub fn on_signal<F>(signal: impl AsRef<str>, handler: F) -> Result<(), String>
where
    F: Fn() + Send + Sync + 'static,
{
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    let signal_number = signal_from_name_unix(signal.as_ref())?;
    let mut signals = Signals::new([signal_number])
        .map_err(|err| format!("failed to register signal handler: {}", err))?;
    let handler = Arc::new(handler);

    thread::spawn(move || {
        for _ in signals.forever() {
            handler();
        }
    });

    Ok(())
}

#[cfg(unix)]
fn signal_from_name_unix(name: &str) -> Result<i32, String> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    let normalized = name.trim().to_ascii_uppercase();
    let trimmed = normalized.strip_prefix("SIG").unwrap_or(&normalized);
    match trimmed {
        "INT" => Ok(SIGINT),
        "TERM" => Ok(SIGTERM),
        "HUP" => Ok(SIGHUP),
        _ => Err(format!("unsupported signal: {}", name)),
    }
}

#[cfg(not(unix))]
pub fn on_signal<F>(signal: impl AsRef<str>, handler: F) -> Result<(), String>
where
    F: Fn() + Send + Sync + 'static,
{
    let name = signal.as_ref().to_string();
    let handler = Arc::new(handler);
    // On Windows, only SIGINT (Ctrl+C) is supported via ctrlc crate or std::panic
    // For now, provide a best-effort: spawn a thread that immediately fires for SIGINT
    if name.to_ascii_uppercase().contains("INT") {
        thread::spawn(move || {
            // no-op stub on Windows — would require ctrlc crate for real support
            let _ = handler;
        });
        Ok(())
    } else {
        Err(format!("signal '{}' is not supported on Windows", name))
    }
}
