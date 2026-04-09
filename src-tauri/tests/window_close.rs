#![cfg(target_os = "windows")]

use std::{
    path::PathBuf,
    ptr::null_mut,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, PostMessageW, WM_CLOSE,
    },
};

const WINDOW_TITLE: &str = "Marky";

#[test]
fn native_close_message_exits_the_app() {
    assert!(
        !is_marky_window_open(),
        "Marky is already running. Close it before running the native window-close smoke test."
    );

    let exe_path = find_app_binary();
    let mut child = Command::new(&exe_path)
        .spawn()
        .unwrap_or_else(|error| panic!("failed to launch {:?}: {error}", exe_path));

    let hwnd = wait_for_window(child.id(), Duration::from_secs(15));
    assert!(
        hwnd != null_mut(),
        "failed to find Marky window for pid {}",
        child.id()
    );

    let posted = unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    assert_ne!(posted, 0, "failed to post WM_CLOSE to Marky window");

    let exited = wait_for_exit(&mut child, Duration::from_secs(10));
    if !exited {
        let _ = child.kill();
    }

    assert!(exited, "Marky did not exit after WM_CLOSE");
}

fn find_app_binary() -> PathBuf {
    if let Ok(path) = std::env::var("MARKY_APP_EXE") {
        return PathBuf::from(path);
    }

    for key in [
        "CARGO_BIN_EXE_barebones-markdown-viewer",
        "CARGO_BIN_EXE_barebones_markdown_viewer",
    ] {
        if let Ok(path) = std::env::var(key) {
            return PathBuf::from(path);
        }
    }

    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("barebones-markdown-viewer.exe");
    assert!(
        fallback.is_file(),
        "failed to find app binary at {:?}",
        fallback
    );
    fallback
}

fn wait_for_window(process_id: u32, timeout: Duration) -> HWND {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(hwnd) = find_window_for_process(process_id) {
            return hwnd;
        }

        thread::sleep(Duration::from_millis(100));
    }

    null_mut()
}

fn is_marky_window_open() -> bool {
    let mut found = false;

    unsafe {
        EnumWindows(
            Some(enum_marky_windows_callback),
            &mut found as *mut bool as LPARAM,
        );
    }

    found
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child
            .try_wait()
            .expect("failed to poll child process")
            .is_some()
        {
            return true;
        }

        thread::sleep(Duration::from_millis(100));
    }

    false
}

fn find_window_for_process(process_id: u32) -> Option<HWND> {
    let mut search = WindowSearch {
        process_id,
        result: None,
    };

    unsafe {
        EnumWindows(
            Some(enum_windows_callback),
            &mut search as *mut WindowSearch as LPARAM,
        );
    }

    search.result
}

struct WindowSearch {
    process_id: u32,
    result: Option<HWND>,
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> i32 {
    let search = &mut *(lparam as *mut WindowSearch);

    if !matches_process(hwnd, search.process_id) || !matches_title(hwnd) {
        return 1;
    }

    search.result = Some(hwnd);
    0
}

unsafe extern "system" fn enum_marky_windows_callback(hwnd: HWND, lparam: LPARAM) -> i32 {
    let found = &mut *(lparam as *mut bool);
    if matches_title(hwnd) && IsWindowVisible(hwnd) != 0 {
        *found = true;
        return 0;
    }

    1
}

unsafe fn matches_process(hwnd: HWND, process_id: u32) -> bool {
    if IsWindowVisible(hwnd) == 0 {
        return false;
    }

    let mut window_process_id = 0;
    GetWindowThreadProcessId(hwnd, &mut window_process_id);
    window_process_id == process_id
}

unsafe fn matches_title(hwnd: HWND) -> bool {
    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return false;
    }

    let mut buffer = vec![0u16; length as usize + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied <= 0 {
        return false;
    }

    String::from_utf16_lossy(&buffer[..copied as usize]) == WINDOW_TITLE
}
