//! 在 Island 内启动 shell 命令并流式推送输出；支持 `ISLAND_CONFIRM:提示` 在岛上确认（写入 stdin）。

use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliTaskPayload {
    pub task_id: u64,
    pub kind: String,
    pub text: Option<String>,
    pub exit_code: Option<i32>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct RunningTask {
    task_id: u64,
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
}

static RUNNING: Mutex<Option<RunningTask>> = Mutex::new(None);

fn emit(app: &AppHandle, p: CliTaskPayload) {
    let _ = app.emit("cli-task", &p);
}

fn parse_confirm_line(line: &str) -> Option<String> {
    let t = line.trim();
    let prefix = "ISLAND_CONFIRM:";
    if let Some(rest) = t.strip_prefix(prefix) {
        let s = rest.trim();
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn spawn_shell_command(command: &str, cwd: Option<&str>) -> Result<Child, String> {
    let mut c = Command::new("cmd.exe");
    c.arg("/C").arg(command);
    if let Some(d) = cwd {
        if !d.trim().is_empty() {
            c.current_dir(d);
        }
    }
    c.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
    c.spawn().map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
fn spawn_shell_command(_command: &str, _cwd: Option<&str>) -> Result<Child, String> {
    Err("cli_spawn 仅支持 Windows".to_string())
}

/// 终止当前任务（若有）。返回被中止的 task_id，供调用方决定是否补发事件。
fn cli_abort_internal() -> Option<u64> {
    let mut g = match RUNNING.lock() {
        Ok(x) => x,
        Err(e) => e.into_inner(),
    };
    let rt = g.take()?;
    let id = rt.task_id;
    if let Ok(mut c) = rt.child.lock() {
        if let Some(mut child) = c.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    Some(id)
}

#[tauri::command]
pub fn cli_spawn(app: AppHandle, command: String, cwd: Option<String>) -> Result<u64, String> {
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("命令为空".into());
    }

    let _ = cli_abort_internal();

    let cwd_ref = cwd.as_deref();
    let mut child = spawn_shell_command(&command, cwd_ref)?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().ok_or("无法读取 stdout")?;
    let stderr = child.stderr.take().ok_or("无法读取 stderr")?;

    let task_id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let child_arc: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(Some(child)));
    let stdin_arc: Arc<Mutex<Option<std::process::ChildStdin>>> = Arc::new(Mutex::new(stdin));

    {
        let mut g = RUNNING.lock().map_err(|e| e.to_string())?;
        *g = Some(RunningTask {
            task_id,
            child: Arc::clone(&child_arc),
            stdin: Arc::clone(&stdin_arc),
        });
    }

    emit(
        &app,
        CliTaskPayload {
            task_id,
            kind: "started".into(),
            text: Some(command.clone()),
            exit_code: None,
        },
    );

    let app_out = app.clone();
    let app_err = app.clone();
    let app_wait = app.clone();
    let child_wait = Arc::clone(&child_arc);

    std::thread::spawn(move || {
        let h_out = {
            let app = app_out.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    if let Some(prompt) = parse_confirm_line(&line) {
                        emit(
                            &app,
                            CliTaskPayload {
                                task_id,
                                kind: "confirm".into(),
                                text: Some(prompt),
                                exit_code: None,
                            },
                        );
                    }
                    emit(
                        &app,
                        CliTaskPayload {
                            task_id,
                            kind: "stdout".into(),
                            text: Some(line),
                            exit_code: None,
                        },
                    );
                }
            })
        };
        let h_err = {
            let app = app_err.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if let Some(prompt) = parse_confirm_line(&line) {
                        emit(
                            &app,
                            CliTaskPayload {
                                task_id,
                                kind: "confirm".into(),
                                text: Some(prompt),
                                exit_code: None,
                            },
                        );
                    }
                    emit(
                        &app,
                        CliTaskPayload {
                            task_id,
                            kind: "stderr".into(),
                            text: Some(line),
                            exit_code: None,
                        },
                    );
                }
            })
        };
        let _ = h_out.join();
        let _ = h_err.join();

        let code = {
            let mut lock = child_wait.lock().unwrap_or_else(|e| e.into_inner());
            lock.take()
                .and_then(|mut ch| ch.wait().ok().and_then(|s| s.code()))
        };

        {
            let mut g = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(rt) = g.as_ref() {
                if rt.task_id == task_id {
                    g.take();
                }
            }
        }

        emit(
            &app_wait,
            CliTaskPayload {
                task_id,
                kind: "done".into(),
                text: None,
                exit_code: code,
            },
        );
    });

    Ok(task_id)
}

#[tauri::command]
pub fn cli_abort(app: AppHandle) -> Result<(), String> {
    if let Some(task_id) = cli_abort_internal() {
        emit(
            &app,
            CliTaskPayload {
                task_id,
                kind: "done".into(),
                text: Some("已中止".into()),
                exit_code: Some(-1),
            },
        );
    }
    Ok(())
}

/// 向当前任务 stdin 写入一行（自动加换行）。用于确认 `y` / `n` 等。
#[tauri::command]
pub fn cli_submit_stdin(line: String) -> Result<(), String> {
    let g = RUNNING.lock().map_err(|e| e.to_string())?;
    let Some(rt) = g.as_ref() else {
        return Err("没有运行中的 CLI 任务".into());
    };
    let mut stdin = rt.stdin.lock().map_err(|_| "stdin 锁失败".to_string())?;
    let Some(w) = stdin.as_mut() else {
        return Err("stdin 不可用".into());
    };
    let mut s = line;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    w.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cli_confirm(accept: bool) -> Result<(), String> {
    let answer = if accept { "y" } else { "n" };
    cli_submit_stdin(answer.to_string())
}
