use pulldown_cmark::{html, Options, Parser};
use serde::Serialize;
use std::{
    cmp::Ordering,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
use tauri::{Emitter, Manager};

const OPEN_DOCUMENT_EVENT: &str = "viewer://document-opened";
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPayload {
    file_name: String,
    path: String,
    html: String,
    source: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderFileEntry {
    name: String,
    path: String,
    relative_path: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderPayload {
    document: DocumentPayload,
    folder_path: String,
    files: Vec<FolderFileEntry>,
}

#[tauri::command]
fn get_launch_document() -> Result<Option<DocumentPayload>, String> {
    let cwd = env::current_dir().map_err(error_message)?;
    open_document_from_args(env::args_os(), &cwd)
}

#[tauri::command]
fn open_markdown_path(path: String) -> Result<DocumentPayload, String> {
    load_document(&PathBuf::from(path))
}

#[tauri::command]
async fn open_markdown_folder(folder_path: String) -> Result<FolderPayload, String> {
    let folder_path = PathBuf::from(folder_path);
    tauri::async_runtime::spawn_blocking(move || load_folder(&folder_path))
        .await
        .map_err(error_message)?
}

#[tauri::command]
fn follow_link(
    current_path: Option<String>,
    href: String,
) -> Result<Option<DocumentPayload>, String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return Ok(None);
    }

    if is_external_href(href) {
        open::that(href).map_err(error_message)?;
        return Ok(None);
    }

    let Some(current_path) = current_path else {
        return Err("This link needs an opened file so it can resolve a local path.".into());
    };

    let next_path = resolve_link_path(Path::new(&current_path), href)?;
    Ok(Some(load_document(&next_path)?))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            focus_main_window(app);

            let cwd = PathBuf::from(cwd);
            if let Ok(Some(document)) =
                open_document_from_args(args.into_iter().map(OsString::from), &cwd)
            {
                let _ = app.emit(OPEN_DOCUMENT_EVENT, document);
            }
        }))
        .invoke_handler(tauri::generate_handler![
            follow_link,
            get_launch_document,
            open_markdown_folder,
            open_markdown_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn focus_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn open_document_from_args<I>(args: I, cwd: &Path) -> Result<Option<DocumentPayload>, String>
where
    I: IntoIterator<Item = OsString>,
{
    for arg in args.into_iter().skip(1) {
        let path = resolve_cli_path(&arg, cwd);
        if path.is_file() {
            return load_document(&path).map(Some);
        }
    }

    Ok(None)
}

fn resolve_cli_path(arg: &OsString, cwd: &Path) -> PathBuf {
    let path = PathBuf::from(arg);
    if path.is_absolute() {
        return path;
    }

    cwd.join(path)
}

fn resolve_link_path(current_path: &Path, href: &str) -> Result<PathBuf, String> {
    let href = href
        .split('#')
        .next()
        .unwrap_or_default()
        .strip_prefix("file://")
        .unwrap_or(href);

    let separator = std::path::MAIN_SEPARATOR.to_string();
    let href = href.replace(['\\', '/'], &separator);
    let href_path = PathBuf::from(href);

    let path = if href_path.is_absolute() {
        href_path
    } else {
        let Some(parent) = current_path.parent() else {
            return Err("The current file does not have a parent directory.".into());
        };
        parent.join(href_path)
    };

    path.canonicalize().map_err(error_message)
}

fn load_document(path: &Path) -> Result<DocumentPayload, String> {
    let absolute_path = path.canonicalize().map_err(error_message)?;
    let bytes = fs::read(&absolute_path).map_err(error_message)?;
    let source = String::from_utf8_lossy(&bytes).into_owned();

    Ok(build_document_payload(absolute_path, source))
}

fn build_document_payload(path: PathBuf, source: String) -> DocumentPayload {
    DocumentPayload {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        path: path.to_string_lossy().into_owned(),
        html: render_markdown(&source),
        source,
    }
}

fn load_folder(directory: &Path) -> Result<FolderPayload, String> {
    let folder_path = directory.canonicalize().map_err(error_message)?;
    let mut files = collect_markdown_paths(&folder_path)?;
    files.sort_by(compare_markdown_paths);

    let Some(initial_path) = select_folder_document(&files) else {
        return Err("The selected folder does not contain a markdown file.".into());
    };

    let file_entries = files
        .iter()
        .map(|path| FolderFileEntry {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            path: path.to_string_lossy().into_owned(),
            relative_path: path
                .strip_prefix(&folder_path)
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path.to_string_lossy().into_owned()),
        })
        .collect();

    Ok(FolderPayload {
        document: load_document(&initial_path)?,
        folder_path: folder_path.to_string_lossy().into_owned(),
        files: file_entries,
    })
}

fn collect_markdown_paths(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    collect_markdown_paths_into(directory, &mut paths)?;
    Ok(paths)
}

fn collect_markdown_paths_into(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(error_message)?;

    for entry in entries {
        let entry = entry.map_err(error_message)?;
        let path = entry.path();

        if path.is_dir() {
            collect_markdown_paths_into(&path, paths)?;
            continue;
        }

        if path.is_file() && is_markdown_path(&path) {
            paths.push(path);
        }
    }

    Ok(())
}

fn render_markdown(source: &str) -> String {
    let parser = Parser::new_ext(source, markdown_options());
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    ammonia::clean(&html_output)
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options
}

fn is_external_href(href: &str) -> bool {
    matches!(
        href.split(':').next(),
        Some("http" | "https" | "mailto" | "tel")
    )
}

fn is_markdown_path(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };

    let extension = extension.to_string_lossy().to_ascii_lowercase();
    MARKDOWN_EXTENSIONS.contains(&extension.as_str())
}

fn compare_markdown_paths(left: &PathBuf, right: &PathBuf) -> Ordering {
    folder_priority(left)
        .cmp(&folder_priority(right))
        .then_with(|| {
            left.to_string_lossy()
                .to_ascii_lowercase()
                .cmp(&right.to_string_lossy().to_ascii_lowercase())
        })
}

fn folder_priority(path: &Path) -> usize {
    let Some(name) = path.file_name() else {
        return 3;
    };

    let file_name = name.to_string_lossy().to_ascii_lowercase();
    if file_name.starts_with("readme.") {
        return 0;
    }
    if file_name.starts_with("index.") {
        return 1;
    }
    2
}

fn select_folder_document(files: &[PathBuf]) -> Option<PathBuf> {
    files.first().cloned()
}

fn error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
