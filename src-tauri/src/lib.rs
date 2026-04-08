use pulldown_cmark::{html, Options, Parser};
use serde::Serialize;
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPayload {
    file_name: String,
    path: String,
    html: String,
    source: String,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![get_launch_document, open_markdown_path])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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

fn error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
