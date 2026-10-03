use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::time::{timeout, timeout_at, Instant};

use crate::error::AppError;
use crate::pm3::binary::{Pm3Binary, Pm3Source};
use crate::pm3::output_parser::strip_ansi;
use crate::pm3::{binary, ports};
use crate::settings;

/// Payload emitted as `pm3-output` events for the live terminal panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm3OutputPayload {
    pub text: String,
    pub is_error: bool,
}

/// Emit raw PM3 output to the frontend terminal panel.
pub fn emit_output<R: Runtime>(app: &AppHandle<R>, text: &str, is_error: bool) {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = app.emit(
            "pm3-output",
            Pm3OutputPayload {
                text: trimmed.to_string(),
                is_error,
            },
        );
    }
}

/// Maximum time to wait for a PM3 subprocess to complete (30 seconds).
const PM3_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to keep reading output after a streaming process reports exit.
const STREAM_DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Inactivity limit for a firmware flash. The client itself waits up to 20 s
/// (`-w`) for the bootloader port to reappear between phases.
const PM3_FLASH_TIMEOUT: Duration = Duration::from_secs(300);

/// Reject invocations that could smuggle extra PM3 commands or flags.
/// The PM3 CLI's `-c` flag treats `;` as a delimiter, so a crafted value
/// like "AA;lf t55xx wipe" would execute two commands. Block this at the
/// chokepoint so no caller can accidentally pass through unsanitised input.
fn validate_invocation(port: &str, cmd: &str) -> Result<(), AppError> {
    if !ports::is_valid_port(port) {
        return Err(AppError::CommandFailed(format!("Invalid port: {}", port)));
    }
    if cmd.contains(';') || cmd.contains('\n') || cmd.contains('\r') {
        return Err(AppError::CommandFailed(
            "Invalid characters in command".into(),
        ));
    }
    Ok(())
}

/// The only place the PM3 client is launched. Resolves the binary to an
/// absolute path (custom setting, bundled sidecar, PATH, known locations)
/// and spawns it with `args`.
fn spawn_pm3<R: Runtime>(
    app: &AppHandle<R>,
    args: &[&str],
) -> Result<(tauri::async_runtime::Receiver<CommandEvent>, CommandChild), AppError> {
    let bin = binary::resolve(settings::custom_client_path(app).as_deref())?;
    spawn_binary(app, &bin, args)
}

fn spawn_binary<R: Runtime>(
    app: &AppHandle<R>,
    bin: &Pm3Binary,
    args: &[&str],
) -> Result<(tauri::async_runtime::Receiver<CommandEvent>, CommandChild), AppError> {
    app.shell()
        .command(bin.path.as_os_str())
        .args(args)
        .spawn()
        .map_err(|e| {
            AppError::CommandFailed(format!(
                "Failed to spawn proxmark3 ({}): {}",
                bin.path.display(),
                e
            ))
        })
}

/// Output of a finished PM3 process, accumulated the same way the shell
/// plugin's `Command::output()` does.
#[derive(Debug, Default)]
struct CollectedOutput {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Drain a PM3 process's events until it terminates or `deadline` passes.
async fn collect_output(
    rx: &mut tauri::async_runtime::Receiver<CommandEvent>,
    deadline: Instant,
) -> Result<CollectedOutput, tokio::time::error::Elapsed> {
    let mut out = CollectedOutput::default();
    while let Some(event) = timeout_at(deadline, rx.recv()).await? {
        match event {
            CommandEvent::Stdout(bytes) => {
                out.stdout.push_str(&String::from_utf8_lossy(&bytes));
                out.stdout.push('\n');
            }
            CommandEvent::Stderr(bytes) => {
                out.stderr.push_str(&String::from_utf8_lossy(&bytes));
                out.stderr.push('\n');
            }
            CommandEvent::Terminated(payload) => out.code = payload.code,
            CommandEvent::Error(_) => {}
            _ => {} // Future CommandEvent variants
        }
    }
    Ok(out)
}

/// Map a finished one-shot PM3 run to cleaned stdout or an error.
/// PM3 exits with -5 (251 as an unsigned exit status) on its own timeouts.
fn interpret_output(cmd: &str, out: CollectedOutput) -> Result<String, AppError> {
    let code = out.code.unwrap_or(-1);
    match code {
        0 => Ok(strip_ansi(&out.stdout)),
        -5 | 251 => Err(AppError::Timeout(format!("PM3 timed out running: {}", cmd))),
        _ => {
            let detail = if out.stderr.trim().is_empty() {
                strip_ansi(&out.stdout)
            } else {
                strip_ansi(&out.stderr)
            };
            Err(AppError::CommandFailed(format!(
                "Exit code {}: {}",
                code, detail
            )))
        }
    }
}

/// Internal PM3 execution that does NOT emit to the frontend.
/// Handles: invocation validation, binary lookup, process spawn, output
/// collection, ANSI stripping, and timeout. A process that outlives the
/// timeout is killed so it can't keep the serial port open.
async fn execute_pm3<R: Runtime>(
    app: &AppHandle<R>,
    port: &str,
    cmd: &str,
) -> Result<String, AppError> {
    execute_pm3_with_timeout(app, port, cmd, PM3_COMMAND_TIMEOUT).await
}

async fn execute_pm3_with_timeout<R: Runtime>(
    app: &AppHandle<R>,
    port: &str,
    cmd: &str,
    limit: Duration,
) -> Result<String, AppError> {
    validate_invocation(port, cmd)?;

    let (mut rx, child) = spawn_pm3(app, &["-p", port, "-f", "-c", cmd])?;

    match collect_output(&mut rx, Instant::now() + limit).await {
        Ok(out) => interpret_output(cmd, out),
        Err(_) => {
            let _ = child.kill();
            Err(AppError::Timeout(format!(
                "PM3 command timed out after {}s: {}",
                limit.as_secs(),
                cmd
            )))
        }
    }
}

/// Run a single PM3 command: spawns `proxmark3 -p {port} -f -c "{cmd}"`,
/// waits for the process to exit (with a 30-second timeout), then returns cleaned stdout.
/// If the subprocess hangs (e.g., USB cable pulled), it is killed after the timeout.
///
/// Emits the command being run and its output to the frontend terminal panel.
///
/// Not externally cancellable: the child handle isn't exposed. That's fine for
/// the short commands routed here (LF writes finish in under 2 seconds); use
/// `run_command_streaming()` for anything long-running.
pub async fn run_command<R: Runtime>(
    app: &AppHandle<R>,
    port: &str,
    cmd: &str,
) -> Result<String, AppError> {
    emit_output(app, &format!("pm3 --> {}", cmd), false);
    match execute_pm3(app, port, cmd).await {
        Ok(output) => {
            emit_output(app, &output, false);
            Ok(output)
        }
        Err(e) => {
            emit_output(app, &e.to_string(), true);
            Err(e)
        }
    }
}

/// Result of launching the client with `--version`, for diagnostics.
pub struct ClientProbe {
    pub path: PathBuf,
    pub source: Pm3Source,
    pub exit_code: Option<i32>,
    /// Cleaned stdout followed by stderr.
    pub output: String,
}

pub enum ProbeError {
    NotFound,
    Spawn(String),
    TimedOut(PathBuf),
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Check that the resolved client actually starts (catches missing DLLs,
/// wrong architecture, quarantine) without touching any serial port.
pub async fn probe_client<R: Runtime>(app: &AppHandle<R>) -> Result<ClientProbe, ProbeError> {
    let bin = binary::resolve(settings::custom_client_path(app).as_deref())
        .map_err(|_| ProbeError::NotFound)?;
    let (mut rx, child) =
        spawn_binary(app, &bin, &["--version"]).map_err(|e| ProbeError::Spawn(e.to_string()))?;

    match collect_output(&mut rx, Instant::now() + PROBE_TIMEOUT).await {
        Ok(out) => Ok(ClientProbe {
            path: bin.path,
            source: bin.source,
            exit_code: out.code,
            output: strip_ansi(&format!("{}{}", out.stdout, out.stderr)),
        }),
        Err(_) => {
            let _ = child.kill();
            Err(ProbeError::TimedOut(bin.path))
        }
    }
}

// ---------------------------------------------------------------------------
// HF Operation State — holds child process for cancellation + dump file path
// ---------------------------------------------------------------------------

/// Managed state for long-running HF operations (autopwn, dump, write).
/// Stored via `app.manage()` in `lib.rs`.
pub struct HfOperationState {
    /// Running child process — `take()` to kill via `CommandChild::kill(self)`.
    pub child: Mutex<Option<CommandChild>>,
    /// Dump file path set by autopwn after completion (e.g. "hf-mf-01020304-dump.bin").
    pub dump_path: Mutex<Option<String>>,
}

impl HfOperationState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            dump_path: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming command execution (HF operations, firmware flash)
// ---------------------------------------------------------------------------

/// Run a PM3 command with streaming output, supporting long timeouts and
/// cancellation. Unlike `run_command()` which waits for exit, this streams
/// lines as they arrive.
///
/// - Each stdout/stderr line is emitted as a `pm3-output` event (live terminal).
/// - A per-line callback `on_line` is invoked for real-time parsing (e.g. autopwn
///   progress events). The callback receives the cleaned line text.
/// - The child process is stored in `hf_state.child` so `cancel_hf_operation`
///   can kill it mid-run; a cancelled run returns `AppError::Cancelled`.
/// - `timeout_secs` is an inactivity limit; the process is killed when it fires.
/// - Returns the accumulated cleaned output on success.
pub async fn run_command_streaming<R: Runtime, F>(
    app: &AppHandle<R>,
    port: &str,
    cmd: &str,
    timeout_secs: u64,
    hf_state: &HfOperationState,
    on_line: F,
) -> Result<String, AppError>
where
    F: FnMut(&str),
{
    validate_invocation(port, cmd)?;
    emit_output(app, &format!("pm3 --> {}", cmd), false);
    run_streaming(
        app,
        &["-p", port, "-f", "-c", cmd],
        Duration::from_secs(timeout_secs),
        &hf_state.child,
        on_line,
    )
    .await
}

/// Flash `image` to the PM3 on `port` (`proxmark3 <port> --flash --image <image> -w`).
/// Streams output like `run_command_streaming()`; the child is parked in
/// `child_slot` so `cancel_flash` can kill it.
pub async fn run_flash<R: Runtime, F>(
    app: &AppHandle<R>,
    port: &str,
    image: &str,
    child_slot: &Mutex<Option<CommandChild>>,
    on_line: F,
) -> Result<String, AppError>
where
    F: FnMut(&str),
{
    validate_invocation(port, "")?;
    emit_output(app, "pm3 --> --flash --image fullimage.elf", false);
    run_streaming(
        app,
        &[port, "--flash", "--image", image, "-w"],
        PM3_FLASH_TIMEOUT,
        child_slot,
        on_line,
    )
    .await
}

/// Spawn PM3 with `args`, park the child in `child_slot`, and stream its
/// output. If the slot is empty once the stream ends, a cancel command took
/// and killed the child: report `AppError::Cancelled` so callers can leave the
/// wizard state to the cancel path instead of racing it with an error.
async fn run_streaming<R: Runtime, F>(
    app: &AppHandle<R>,
    args: &[&str],
    inactivity_timeout: Duration,
    child_slot: &Mutex<Option<CommandChild>>,
    mut on_line: F,
) -> Result<String, AppError>
where
    F: FnMut(&str),
{
    let (rx, child) = match spawn_pm3(app, args) {
        Ok(spawned) => spawned,
        Err(e) => {
            emit_output(app, &e.to_string(), true);
            return Err(e);
        }
    };

    // Never bail out between spawn and parking the child: an untracked
    // client can't be cancelled or killed.
    *child_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);

    let result = read_stream_with_timeout(rx, inactivity_timeout, &mut |line, is_error| {
        emit_output(app, line, is_error);
        on_line(line);
    })
    .await;

    let child = child_slot.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(child) = child else {
        emit_output(app, "[=] Operation cancelled", false);
        return Err(AppError::Cancelled);
    };

    match result {
        Ok(output) => Ok(output),
        Err(e) => {
            // Timeout or pipe error while the client may still be running.
            let _ = child.kill();
            emit_output(app, &e.to_string(), true);
            Err(e)
        }
    }
}

/// Read from a `CommandEvent` receiver until the process exits, passing each
/// cleaned, non-empty line to `on_line(line, is_error)`. Returns the
/// accumulated output. Kept free of `AppHandle` so it can be unit-tested
/// without a Tauri runtime.
async fn read_stream_with_timeout<F>(
    mut rx: tauri::async_runtime::Receiver<CommandEvent>,
    inactivity_timeout: Duration,
    on_line: &mut F,
) -> Result<String, AppError>
where
    F: FnMut(&str, bool),
{
    let mut accumulated = String::new();
    let mut exit_code: Option<i32> = None;
    // Set once Terminated arrives; output may still be in flight until then.
    let mut drain_deadline: Option<Instant> = None;

    loop {
        let next = match drain_deadline {
            Some(deadline) => match timeout_at(deadline, rx.recv()).await {
                Ok(next) => next,
                Err(_) => break,
            },
            None => match timeout(inactivity_timeout, rx.recv()).await {
                Ok(next) => next,
                Err(_) => {
                    return Err(AppError::Timeout(format!(
                        "PM3 produced no output for {}s",
                        inactivity_timeout.as_secs()
                    )));
                }
            },
        };
        let Some(event) = next else {
            // Channel closed: the process exited and both pipes hit EOF.
            break;
        };
        match event {
            CommandEvent::Stdout(bytes) => {
                handle_stream_line(&bytes, false, on_line, &mut accumulated);
            }
            CommandEvent::Stderr(bytes) => {
                handle_stream_line(&bytes, true, on_line, &mut accumulated);
            }
            CommandEvent::Error(msg) => {
                // run_streaming emits the returned error to the terminal.
                return Err(AppError::CommandFailed(format!("Process error: {}", msg)));
            }
            CommandEvent::Terminated(payload) => {
                // The shell plugin can deliver Terminated before the last
                // lines of a process that exits quickly, so keep reading
                // until the pipes close instead of stopping here.
                exit_code = payload.code;
                drain_deadline = Some(Instant::now() + STREAM_DRAIN_GRACE);
            }
            _ => {} // Future CommandEvent variants — ignore
        }
    }

    match exit_code {
        Some(0) => Ok(accumulated),
        Some(-5) | Some(251) => Err(AppError::Timeout("PM3 subprocess timed out".into())),
        Some(code) => Err(AppError::CommandFailed(format!(
            "PM3 exited with code {}",
            code
        ))),
        None => Err(AppError::CommandFailed(
            "PM3 was terminated by a signal".into(),
        )),
    }
}

fn handle_stream_line<F>(bytes: &[u8], is_error: bool, on_line: &mut F, accumulated: &mut String)
where
    F: FnMut(&str, bool),
{
    let line = String::from_utf8_lossy(bytes);
    let cleaned = strip_ansi(&line);
    let trimmed = cleaned.trim();
    if !trimmed.is_empty() {
        on_line(trimmed, is_error);
        accumulated.push_str(trimmed);
        accumulated.push('\n');
    }
}

/// Probe serial ports with `hw version` to find a connected PM3.
/// Returns (port, model, firmware) on success.
///
/// Probe order comes from `ports::detection_order()`: the preferred port from
/// Settings, enumerated ports that look like a PM3 (USB VID/PID), other USB
/// serial ports, then the legacy fixed guesses.
///
/// Uses friendly, hacker-casual terminal output. All probe messages are green
/// (non-error) except the final "not found" message.
pub async fn detect_device<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(String, String, String), AppError> {
    // Pick a random init message for personality
    let init_msgs = [
        "[=] Sniffing USB bus... come out, Proxmark",
        "[=] Deploying port tentacles...",
        "[=] Hunting for hardware... stay still",
        "[=] Scanning the wire... don't be shy",
        "[=] Reaching out to the other side...",
    ];
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % init_msgs.len();
    emit_output(app, init_msgs[idx], false);

    // Resolve the client up front: without it no port can be probed.
    match binary::resolve(settings::custom_client_path(app).as_deref()) {
        Ok(bin) => emit_output(
            app,
            &format!("[=] PM3 client: {}", bin.path.display()),
            false,
        ),
        Err(e) => {
            emit_output(
                app,
                "[!!] Proxmark3 client not found. Check installation.",
                true,
            );
            emit_platform_install_hints(app);
            return Err(e);
        }
    }

    let enumerated = ports::list_ports();
    if let Ok(found) = &enumerated {
        for port in found.iter().filter(|p| p.likely_pm3) {
            emit_output(
                app,
                &format!("[+] Proxmark3 USB device on {}", port.name),
                false,
            );
        }
    }
    let preferred = settings::current(app).preferred_port;
    let candidates = ports::detection_order(
        preferred.as_deref(),
        &enumerated,
        &ports::fallback_candidates(),
    );

    for port in &candidates {
        emit_output(app, &format!("[=] Knocking on {}...", port), false);

        match execute_pm3(app, port, "hw version").await {
            Ok(output) => {
                if let Some((model, firmware)) = parse_hw_version(&output) {
                    emit_output(
                        app,
                        &format!("[+] Target acquired: {} on {}", model, port),
                        false,
                    );
                    emit_output(app, &format!("[+] Firmware: {}", firmware), false);
                    return Ok((port.clone(), model, firmware));
                }
                // Got output but couldn't parse hw version -- wrong device
                emit_output(app, &format!("[-] {} -- wrong device", port), false);
            }
            Err(e) => {
                // Capabilities mismatch means the PM3 device IS present on this
                // port but the firmware doesn't match the client version. Treat
                // it as a successful detection -- the firmware check step will
                // handle the mismatch and offer to flash.
                let err_msg = e.to_string();
                if err_msg.to_lowercase().contains("capabilities") {
                    emit_output(
                        app,
                        &format!(
                            "[+] Target acquired: Proxmark3 on {} (firmware mismatch)",
                            port
                        ),
                        false,
                    );
                    return Ok((
                        port.clone(),
                        "Proxmark3".to_string(),
                        "mismatched".to_string(),
                    ));
                }

                // Distinguish "no response" (spawn succeeded but device didn't respond)
                // from other errors. If spawn itself failed (binary not found), that
                // affects ALL ports, so propagate immediately.
                if err_msg.contains("Failed to spawn proxmark3") {
                    emit_output(
                        app,
                        "[!!] Proxmark3 binary could not be started. Check installation.",
                        true,
                    );
                    return Err(e);
                }

                emit_output(app, &format!("[-] {} -- no response", port), false);
            }
        }
    }

    emit_output(app, "[!!] No Proxmark3 found.", true);
    emit_output(
        app,
        "[=] Try a different USB cable (some are charge-only)",
        false,
    );
    emit_platform_detect_hints(app);
    Err(AppError::DeviceNotFound)
}

fn emit_platform_detect_hints<R: Runtime>(app: &AppHandle<R>) {
    let hints: &[&str] = if cfg!(target_os = "windows") {
        &[
            "[=] Check Device Manager for a COM port",
            "[=] PM3 Easy: may need CH340 driver (wch-ic.com)",
        ]
    } else if cfg!(target_os = "macos") {
        &["[=] Check System Information > USB, and ls /dev/tty.usbmodem*"]
    } else {
        &[
            "[=] Check ls /dev/ttyACM* and that your user is in the dialout group",
            "[=] ModemManager can grab the port: stop it or add a udev rule",
        ]
    };
    for hint in hints {
        emit_output(app, hint, false);
    }
}

fn emit_platform_install_hints<R: Runtime>(app: &AppHandle<R>) {
    let hint = if cfg!(target_os = "windows") {
        "[=] Reinstall Phosphor, or check antivirus quarantine for proxmark3.exe"
    } else if cfg!(target_os = "macos") {
        "[=] brew install rfidresearchgroup/proxmark3/proxmark3, or set the path in Settings"
    } else {
        "[=] Install the Iceman proxmark3 client, or set its path in Settings"
    };
    emit_output(app, hint, false);
}

fn parse_hw_version(output: &str) -> Option<(String, String)> {
    use crate::pm3::version::parse_detailed_hw_version;

    let info = parse_detailed_hw_version(output);

    // Pick best version source: os_version (device firmware) > client_version
    let version_str = if !info.os_version.is_empty() {
        info.os_version
    } else if !info.client_version.is_empty() {
        info.client_version
    } else {
        // No version found — check if it's at least a PM3 device
        if output.to_lowercase().contains("proxmark") {
            return Some((info.model, "unknown".to_string()));
        }
        return None;
    };

    // Extract clean short version like "v4.20728" for sidebar display
    let firmware = extract_short_version(&version_str);
    Some((info.model, firmware))
}

/// Extract a short version string like "v4.20728" from a full version string
/// like "Iceman/master/v4.20728-358-ga2ba91043-suspect".
fn extract_short_version(version_str: &str) -> String {
    // Find 'v' followed by a digit
    let v_pos = version_str.char_indices().find(|&(i, c)| {
        c == 'v'
            && version_str.get(i + 1..i + 2).map_or(false, |s| {
                s.as_bytes().first().map_or(false, |b| b.is_ascii_digit())
            })
    });

    if let Some((pos, _)) = v_pos {
        let rest = &version_str[pos..];
        // Version is "v" + digits/dots, stop at anything else
        let end = rest
            .find(|c: char| c != 'v' && !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        rest[..end].to_string()
    } else {
        version_str.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_shell::process::TerminatedPayload;

    fn finished(code: Option<i32>, stdout: &str, stderr: &str) -> CollectedOutput {
        CollectedOutput {
            code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn invocation_rejects_separators_and_bad_ports() {
        assert!(validate_invocation("COM3", "hw version").is_ok());
        assert!(validate_invocation("COM3", "lf search;lf t55xx wipe").is_err());
        assert!(validate_invocation("COM3", "hw version\nhw reset").is_err());
        assert!(validate_invocation("COM3", "hw version\rhw reset").is_err());
        assert!(validate_invocation("--flash", "hw version").is_err());
        assert!(validate_invocation("COM3;hw reset", "hw version").is_err());
    }

    #[test]
    fn exit_zero_returns_clean_stdout() {
        let out = interpret_output("hw version", finished(Some(0), "\x1b[32mOK\x1b[0m\n", ""));
        assert_eq!(out.unwrap().trim(), "OK");
    }

    #[test]
    fn pm3_timeout_codes_map_to_timeout() {
        for code in [-5, 251] {
            let err = interpret_output("lf search", finished(Some(code), "", "")).unwrap_err();
            assert!(matches!(err, AppError::Timeout(_)));
        }
    }

    #[test]
    fn failure_prefers_stderr_and_keeps_capabilities_text() {
        let err = interpret_output(
            "hw version",
            finished(Some(1), "banner", "[!!] capabilities mismatch\n"),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Exit code 1"));
        assert!(msg.contains("capabilities"));
        assert!(!msg.contains("banner"));
    }

    #[test]
    fn failure_falls_back_to_stdout_and_signal_maps_to_minus_one() {
        let err = interpret_output("hw version", finished(None, "no device", "  \n")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "PM3 command failed: Exit code -1: no device"
        );
    }

    #[tokio::test]
    async fn collect_output_gathers_streams_until_termination() {
        let (tx, mut rx) = tauri::async_runtime::channel(8);
        tx.send(CommandEvent::Stdout(b"line 1\n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Stderr(b"warn\n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Stdout(b"line 2\n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Terminated(TerminatedPayload {
            code: Some(0),
            signal: None,
        }))
        .await
        .unwrap();
        drop(tx);

        let out = collect_output(&mut rx, Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(out.code, Some(0));
        assert!(out.stdout.contains("line 1") && out.stdout.contains("line 2"));
        assert!(out.stderr.contains("warn"));
    }

    #[tokio::test]
    async fn stream_keeps_output_that_arrives_after_termination() {
        let (tx, rx) = tauri::async_runtime::channel(8);
        tx.send(CommandEvent::Stdout(b"[+] Flashing...\n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Terminated(TerminatedPayload {
            code: Some(0),
            signal: None,
        }))
        .await
        .unwrap();
        tx.send(CommandEvent::Stdout(b"[+] All done\n".to_vec()))
            .await
            .unwrap();
        drop(tx);

        let mut lines = Vec::new();
        let out = read_stream_with_timeout(rx, Duration::from_secs(5), &mut |l: &str, _| {
            lines.push(l.to_string())
        })
        .await
        .unwrap();
        assert!(out.contains("All done"));
        assert_eq!(lines, vec!["[+] Flashing...", "[+] All done"]);
    }

    #[tokio::test]
    async fn stream_marks_stderr_lines_and_cleans_ansi() {
        let (tx, rx) = tauri::async_runtime::channel(8);
        tx.send(CommandEvent::Stdout(b"\x1b[32m[+] ok\x1b[0m\n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Stderr(b"[!!] warn\n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Stdout(b"   \n".to_vec()))
            .await
            .unwrap();
        tx.send(CommandEvent::Terminated(TerminatedPayload {
            code: Some(0),
            signal: None,
        }))
        .await
        .unwrap();
        drop(tx);

        let mut lines = Vec::new();
        read_stream_with_timeout(rx, Duration::from_secs(5), &mut |l: &str, is_error| {
            lines.push((l.to_string(), is_error))
        })
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("[+] ok".to_string(), false),
                ("[!!] warn".to_string(), true)
            ]
        );
    }

    #[tokio::test]
    async fn stream_stops_draining_after_grace_period() {
        let (tx, rx) = tauri::async_runtime::channel(8);
        tx.send(CommandEvent::Terminated(TerminatedPayload {
            code: Some(0),
            signal: None,
        }))
        .await
        .unwrap();
        // `tx` stays alive, like a pipe held open by a leftover grandchild.
        let started = std::time::Instant::now();
        let out = read_stream_with_timeout(rx, Duration::from_secs(60), &mut |_: &str, _| {})
            .await
            .unwrap();
        assert!(out.is_empty());
        assert!(started.elapsed() < Duration::from_secs(10));
        drop(tx);
    }

    #[tokio::test]
    async fn collect_output_times_out_on_silent_process() {
        let (_tx, mut rx) = tauri::async_runtime::channel::<CommandEvent>(1);
        let result = collect_output(&mut rx, Instant::now() + Duration::from_millis(20)).await;
        assert!(result.is_err());
    }
}

/// End-to-end tests against a fake `proxmark3` shell script, spawned through
/// a mock Tauri app exactly like the real client. Covers the process
/// lifecycle the parser tests can't: timeouts kill the client, cancellation
/// is reported, and detection/flash argument shapes reach the binary.
#[cfg(all(test, unix))]
mod process_tests {
    use super::*;
    use crate::settings::{Pm3Settings, SettingsState};
    use std::path::{Path, PathBuf};
    use tauri::Manager;

    const FAKE_PM3: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo $$ > "$dir/pid"
if [ "$1" = "-p" ]; then port="$2"; else port="$1"; fi
case "$*" in
  *--flash*)
    echo "[+] Entering bootloader..."
    echo "[+] Writing segments for file: fullimage.elf"
    echo "[+] All done"
    exit 0 ;;
esac
case "$port" in
  /dev/ttyACM0) cat "$dir/hw_version.txt"; exit 0 ;;
  /dev/ttyACM1) echo "[!!] ERROR: capabilities mismatch" >&2; exit 1 ;;
  /dev/ttyACM2) exec sleep 30 ;;
  /dev/ttyACM3) echo "[+] line 1"; echo "[+] line 2"; exec sleep 30 ;;
  *) echo "[!!] ERROR: invalid serial port $port" >&2; exit 1 ;;
esac
"#;

    const HW_VERSION: &str = " [ Client ]\n  client: Iceman/master/v4.20728-234-g1a2b3c4d5\n [ ARM ]\n  os: Iceman/master/v4.20728-234-g1a2b3c4d5\n [ Hardware ]\n  --= uC: AT91SAM7S512 Rev B\n";

    struct Fixture {
        app: tauri::App<tauri::test::MockRuntime>,
        dir: PathBuf,
    }

    fn fixture(name: &str, preferred_port: Option<&str>) -> Fixture {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("phosphor-fake-pm3-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let client = dir.join("proxmark3");
        std::fs::write(&client, FAKE_PM3).unwrap();
        std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join("hw_version.txt"), HW_VERSION).unwrap();

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_shell::init())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        app.manage(SettingsState::new(Pm3Settings {
            preferred_port: preferred_port.map(String::from),
            client_path: Some(client.display().to_string()),
        }));
        Fixture { app, dir }
    }

    /// Wait for the fake client's PID to disappear (killed and reaped).
    async fn assert_process_gone(dir: &Path) {
        let pid = std::fs::read_to_string(dir.join("pid")).unwrap();
        for _ in 0..100 {
            let alive = std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            if !alive {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("fake proxmark3 (pid {}) is still running", pid.trim());
    }

    #[tokio::test]
    async fn one_shot_command_returns_output() {
        let f = fixture("ok", None);
        let out = run_command(f.app.handle(), "/dev/ttyACM0", "hw version")
            .await
            .unwrap();
        assert!(out.contains("AT91SAM7S512"));
    }

    #[tokio::test]
    async fn one_shot_failure_carries_stderr() {
        let f = fixture("fail", None);
        let err = run_command(f.app.handle(), "/dev/ttyACM1", "hw version")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("capabilities mismatch"));
    }

    #[tokio::test]
    async fn one_shot_timeout_kills_the_client() {
        let f = fixture("timeout", None);
        let err = execute_pm3_with_timeout(
            f.app.handle(),
            "/dev/ttyACM2",
            "hw version",
            Duration::from_millis(300),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Timeout(_)));
        assert_process_gone(&f.dir).await;
    }

    #[tokio::test]
    async fn detection_uses_preferred_port() {
        let f = fixture("detect", Some("/dev/ttyACM0"));
        let (port, _model, firmware) = detect_device(f.app.handle()).await.unwrap();
        assert_eq!(port, "/dev/ttyACM0");
        assert_eq!(firmware, "v4.20728");
    }

    #[tokio::test]
    async fn detection_treats_capabilities_mismatch_as_present() {
        let f = fixture("mismatch", Some("/dev/ttyACM1"));
        let (port, _model, firmware) = detect_device(f.app.handle()).await.unwrap();
        assert_eq!(port, "/dev/ttyACM1");
        assert_eq!(firmware, "mismatched");
    }

    #[tokio::test]
    async fn streaming_cancel_kills_client_and_reports_cancelled() {
        let f = fixture("cancel", None);
        let state = HfOperationState::new();
        let mut lines = Vec::new();

        let run = run_command_streaming(
            f.app.handle(),
            "/dev/ttyACM3",
            "hf mf autopwn",
            60,
            &state,
            |l| lines.push(l.to_string()),
        );
        let cancel = async {
            // Wait for the child to be parked, give it time to print, then
            // do what cancel_hf_operation does.
            loop {
                if state.child.lock().unwrap().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            let child = state.child.lock().unwrap().take().unwrap();
            child.kill().unwrap();
        };
        let (result, ()) = tokio::join!(run, cancel);

        assert!(matches!(result, Err(AppError::Cancelled)));
        assert_eq!(lines, vec!["[+] line 1", "[+] line 2"]);
        assert_process_gone(&f.dir).await;
    }

    #[tokio::test]
    async fn streaming_inactivity_timeout_kills_client() {
        let f = fixture("stall", None);
        let state = HfOperationState::new();
        let result = run_command_streaming(
            f.app.handle(),
            "/dev/ttyACM3",
            "hf mf autopwn",
            1,
            &state,
            |_| {},
        )
        .await;
        assert!(matches!(result, Err(AppError::Timeout(_))));
        assert!(state.child.lock().unwrap().is_none());
        assert_process_gone(&f.dir).await;
    }

    #[tokio::test]
    async fn flash_passes_port_positionally() {
        let f = fixture("flash", None);
        let slot = Mutex::new(None);
        let out = run_flash(
            f.app.handle(),
            "/dev/ttyACM0",
            "/tmp/fullimage.elf",
            &slot,
            |_| {},
        )
        .await
        .unwrap();
        assert!(out.contains("All done"));
        assert!(slot.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn probe_reports_client_output() {
        let f = fixture("probe", None);
        let probe = probe_client(f.app.handle()).await.ok().unwrap();
        assert_eq!(probe.source, Pm3Source::Custom);
        assert_eq!(probe.exit_code, Some(1));
        assert!(probe.output.contains("invalid serial port --version"));
    }

    #[tokio::test]
    async fn invalid_port_is_rejected_before_spawning() {
        let f = fixture("invalid", None);
        let err = run_command(f.app.handle(), "--flash", "hw version")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Invalid port"));
        assert!(!f.dir.join("pid").exists());
    }
}
