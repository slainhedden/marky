use ammonia::Builder as HtmlSanitizer;
use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::{
    borrow::Cow,
    cmp::Ordering,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use syntect::{
    html::{ClassStyle, ClassedHTMLGenerator},
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};
use tauri::{Emitter, Manager};

const OPEN_DOCUMENT_EVENT: &str = "viewer://document-opened";
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];
const CODE_CLASS_PREFIXES: &[&str] = &["syn-", "language-", "code-block", "diff-line"];
const DOCUMENT_RENDER_CACHE_LIMIT: usize = 16;
const MAX_HIGHLIGHTED_CODE_BYTES: usize = 64 * 1024;
const MAX_HIGHLIGHTED_CODE_LINES: usize = 1200;
const MAX_DIFF_RENDER_LINES: usize = 1200;
const CLUTTER_FOLDER_NAMES: &[&str] = &[
    ".git",
    ".next",
    ".pytest_cache",
    ".venv",
    "__pycache__",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "venv",
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPayload {
    file_name: String,
    path: Option<String>,
    directory: Option<String>,
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
    include_clutter: bool,
}

#[derive(Clone)]
struct CachedDocumentRender {
    path: String,
    source: String,
    html: String,
}

enum NormalizedFenceLanguage {
    PlainText,
    Diff,
    Syntax {
        syntax_token: String,
        class_token: String,
    },
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
async fn open_markdown_folder(
    folder_path: String,
    include_clutter: Option<bool>,
) -> Result<FolderPayload, String> {
    let folder_path = PathBuf::from(folder_path);
    let include_clutter = include_clutter.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || load_folder(&folder_path, include_clutter))
        .await
        .map_err(error_message)?
}

#[tauri::command]
async fn save_markdown_path(path: String, source: String) -> Result<DocumentPayload, String> {
    let path = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || save_document(path, source))
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
            open_markdown_path,
            save_markdown_path
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
    let html = get_cached_document_html(&absolute_path, &source)
        .unwrap_or_else(|| render_and_cache_document_html(&absolute_path, &source));

    Ok(build_document_payload(absolute_path, source, html))
}

fn save_document(path: PathBuf, source: String) -> Result<DocumentPayload, String> {
    let absolute_path = path.canonicalize().map_err(error_message)?;
    fs::write(&absolute_path, source.as_bytes()).map_err(error_message)?;
    let html = render_and_cache_document_html(&absolute_path, &source);
    Ok(build_document_payload(absolute_path, source, html))
}

fn build_document_payload(path: PathBuf, source: String, html: String) -> DocumentPayload {
    DocumentPayload {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        path: Some(path.to_string_lossy().into_owned()),
        directory: path.parent().map(|parent| parent.to_string_lossy().into_owned()),
        html,
        source,
    }
}

fn get_cached_document_html(path: &Path, source: &str) -> Option<String> {
    let cache = get_document_render_cache().lock().unwrap();
    let path = path.to_string_lossy();

    cache
        .iter()
        .find(|entry| entry.path == path && entry.source == source)
        .map(|entry| entry.html.clone())
}

fn render_and_cache_document_html(path: &Path, source: &str) -> String {
    let html = render_markdown(source);
    let mut cache = get_document_render_cache().lock().unwrap();
    let path = path.to_string_lossy().into_owned();

    if let Some(index) = cache.iter().position(|entry| entry.path == path) {
        cache.remove(index);
    }

    cache.insert(
        0,
        CachedDocumentRender {
            path,
            source: source.to_string(),
            html: html.clone(),
        },
    );

    if cache.len() > DOCUMENT_RENDER_CACHE_LIMIT {
        cache.truncate(DOCUMENT_RENDER_CACHE_LIMIT);
    }

    html
}

fn get_document_render_cache() -> &'static Mutex<Vec<CachedDocumentRender>> {
    static DOCUMENT_RENDER_CACHE: OnceLock<Mutex<Vec<CachedDocumentRender>>> = OnceLock::new();
    DOCUMENT_RENDER_CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

fn load_folder(directory: &Path, include_clutter: bool) -> Result<FolderPayload, String> {
    let folder_path = directory.canonicalize().map_err(error_message)?;
    let mut files = collect_markdown_paths(&folder_path, include_clutter)?;
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
        include_clutter,
    })
}

fn collect_markdown_paths(directory: &Path, include_clutter: bool) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    collect_markdown_paths_into(directory, &mut paths, include_clutter)?;
    Ok(paths)
}

fn collect_markdown_paths_into(
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    include_clutter: bool,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(error_message)?;

    for entry in entries {
        let entry = entry.map_err(error_message)?;
        let path = entry.path();

        if path.is_dir() {
            if !include_clutter && is_clutter_directory(&path) {
                continue;
            }

            collect_markdown_paths_into(&path, paths, include_clutter)?;
            continue;
        }

        if path.is_file() && is_markdown_path(&path) {
            paths.push(path);
        }
    }

    Ok(())
}

fn render_markdown(source: &str) -> String {
    sanitize_html(&render_markdown_html(source))
}

fn render_markdown_html(source: &str) -> String {
    if !contains_fenced_code_block(source) {
        let mut html_output = String::with_capacity(initial_html_capacity(source));
        html::push_html(&mut html_output, Parser::new_ext(source, markdown_options()));
        return html_output;
    }

    let parser = Parser::new_ext(source, markdown_options());
    let mut events = parser.into_iter();
    let mut buffered_events = Vec::new();
    let mut html_output = String::with_capacity(initial_html_capacity(source));

    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                flush_markdown_events(&mut html_output, &mut buffered_events);

                let mut code = String::new();
                for code_event in events.by_ref() {
                    match code_event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(text) | Event::Code(text) | Event::Html(text) => {
                            code.push_str(text.as_ref());
                        }
                        Event::SoftBreak | Event::HardBreak => code.push('\n'),
                        _ => {}
                    }
                }

                html_output.push_str(&render_code_block(normalize_fence_language(info.as_ref()), &code));
            }
            _ => buffered_events.push(event),
        }
    }

    flush_markdown_events(&mut html_output, &mut buffered_events);
    html_output
}

fn contains_fenced_code_block(source: &str) -> bool {
    source.contains("```") || source.contains("~~~")
}

fn initial_html_capacity(source: &str) -> usize {
    source.len().saturating_add(source.len() / 2)
}

fn render_code_block(language: NormalizedFenceLanguage, code: &str) -> String {
    match language {
        NormalizedFenceLanguage::PlainText => render_plain_code_block("plain", code),
        NormalizedFenceLanguage::Diff => render_diff_block(code),
        NormalizedFenceLanguage::Syntax {
            syntax_token,
            class_token,
        } => render_syntax_highlighted_block(&syntax_token, &class_token, code),
    }
}

fn render_syntax_highlighted_block(syntax_token: &str, class_token: &str, code: &str) -> String {
    if exceeds_highlight_threshold(code) {
        return render_plain_code_block(class_token, code);
    }

    let Some(syntax) = find_syntax_by_token(syntax_token) else {
        return render_plain_code_block("plain", code);
    };

    let syntax_set = get_syntax_set();
    let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
        syntax,
        syntax_set,
        ClassStyle::SpacedPrefixed { prefix: "syn-" },
    );

    let owned_code = (!code.is_empty() && !code.ends_with('\n')).then(|| format!("{code}\n"));
    let code_for_highlighting = if let Some(code_with_newline) = owned_code.as_deref() {
        code_with_newline
    } else {
        code
    };

    for line in LinesWithEndings::from(code_for_highlighting) {
        if html_generator
            .parse_html_for_line_which_includes_newline(line)
            .is_err()
        {
            return render_plain_code_block(class_token, code);
        }
    }

    render_wrapped_code_block(
        "code-block code-block--highlighted",
        class_token,
        &html_generator.finalize(),
    )
}

fn exceeds_highlight_threshold(code: &str) -> bool {
    if code.len() > MAX_HIGHLIGHTED_CODE_BYTES {
        return true;
    }

    code.lines().take(MAX_HIGHLIGHTED_CODE_LINES + 1).count() > MAX_HIGHLIGHTED_CODE_LINES
}

fn render_plain_code_block(class_token: &str, code: &str) -> String {
    render_wrapped_code_block("code-block", class_token, &escape_html(code))
}

fn render_diff_block(code: &str) -> String {
    if exceeds_diff_render_threshold(code) {
        return render_plain_code_block("diff", code);
    }

    let mut inner_html = String::new();
    for line in code.split_inclusive('\n') {
        inner_html.push_str(&render_diff_line(line));
    }

    render_wrapped_code_block("code-block code-block--diff", "diff", &inner_html)
}

fn exceeds_diff_render_threshold(code: &str) -> bool {
    code.lines().take(MAX_DIFF_RENDER_LINES + 1).count() > MAX_DIFF_RENDER_LINES
}

fn render_diff_line(line: &str) -> String {
    let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
    let mut escaped_line = escape_html(line_without_newline);
    if line.ends_with('\n') {
        escaped_line.push('\n');
    }

    format!(
        "<span class=\"diff-line {}\">{}</span>",
        diff_line_class(line_without_newline),
        escaped_line
    )
}

fn diff_line_class(line: &str) -> &'static str {
    if line.starts_with("+++") || line.starts_with("---") {
        return "diff-line--file";
    }
    if line.starts_with("@@") {
        return "diff-line--hunk";
    }
    if line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("rename from ")
        || line.starts_with("rename to ")
        || line.starts_with("similarity index ")
        || line.starts_with("Binary files ")
        || line.starts_with("GIT binary patch")
    {
        return "diff-line--meta";
    }
    if line.starts_with('+') {
        return "diff-line--add";
    }
    if line.starts_with('-') {
        return "diff-line--remove";
    }
    "diff-line--context"
}

fn render_wrapped_code_block(pre_classes: &str, class_token: &str, inner_html: &str) -> String {
    format!(
        "<pre class=\"{pre_classes}\"><code class=\"language-{class_token}\">{inner_html}</code></pre>"
    )
}

fn sanitize_html(html: &str) -> String {
    get_html_sanitizer().clean(html).to_string()
}

fn get_html_sanitizer() -> &'static HtmlSanitizer<'static> {
    static HTML_SANITIZER: OnceLock<HtmlSanitizer<'static>> = OnceLock::new();
    HTML_SANITIZER.get_or_init(|| {
        let mut sanitizer = HtmlSanitizer::default();
        sanitizer.add_tags(&["span"]);
        sanitizer.add_generic_attributes(&["class"]);
        sanitizer.attribute_filter(|tag, attribute, value| {
            if attribute != "class" {
                return Some(Cow::Borrowed(value));
            }

            if !matches!(tag, "pre" | "code" | "span") {
                return None;
            }

            let filtered = value
                .split_whitespace()
                .filter(|class_name| is_allowed_code_class(class_name))
                .collect::<Vec<_>>();

            if filtered.is_empty() {
                None
            } else {
                Some(Cow::Owned(filtered.join(" ")))
            }
        });

        sanitizer
    })
}

fn normalize_fence_language(info: &str) -> NormalizedFenceLanguage {
    let info = info.trim();
    if info.is_empty() {
        return NormalizedFenceLanguage::PlainText;
    }

    let mut parts = info.split_whitespace();
    let first_token = parts.next().unwrap();
    if first_token.eq_ignore_ascii_case("git")
        && matches!(parts.next(), Some(token) if token.eq_ignore_ascii_case("diff"))
    {
        return NormalizedFenceLanguage::Diff;
    }

    let lower_token = first_token.to_ascii_lowercase();
    let token = lower_token
        .strip_prefix("language-")
        .unwrap_or(&lower_token)
        .to_string();

    match token.as_str() {
        "diff" | "patch" => return NormalizedFenceLanguage::Diff,
        "md" | "text" | "plaintext" | "none" => return NormalizedFenceLanguage::PlainText,
        _ => {}
    }

    let syntax_token = match token.as_str() {
        "js" => Some("javascript".to_string()),
        "ts" => Some("typescript".to_string()),
        "py" => Some("python".to_string()),
        "sh" | "shell" | "zsh" => Some("bash".to_string()),
        "ps1" | "ps" => Some("powershell".to_string()),
        "yml" => Some("yaml".to_string()),
        "rb" => Some("ruby".to_string()),
        "rs" => Some("rust".to_string()),
        _ => {
            if find_syntax_by_token(first_token).is_some() {
                Some(first_token.to_string())
            } else if find_syntax_by_token(&token).is_some() {
                Some(token)
            } else {
                None
            }
        }
    };

    let Some(syntax_token) = syntax_token else {
        return NormalizedFenceLanguage::PlainText;
    };

    NormalizedFenceLanguage::Syntax {
        class_token: sanitize_language_class_token(&syntax_token),
        syntax_token,
    }
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options
}

fn flush_markdown_events<'a>(html_output: &mut String, buffered_events: &mut Vec<Event<'a>>) {
    if buffered_events.is_empty() {
        return;
    }

    html::push_html(html_output, buffered_events.drain(..));
}

fn get_syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn find_syntax_by_token(token: &str) -> Option<&'static SyntaxReference> {
    get_syntax_set().find_syntax_by_token(token)
}

fn sanitize_language_class_token(token: &str) -> String {
    let mut class_token = String::new();
    let mut previous_was_separator = false;

    for ch in token.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            class_token.push(ch);
            previous_was_separator = false;
            continue;
        }

        if !previous_was_separator {
            class_token.push('-');
            previous_was_separator = true;
        }
    }

    let trimmed = class_token.trim_matches('-');
    if trimmed.is_empty() {
        "plain".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_allowed_code_class(class_name: &str) -> bool {
    CODE_CLASS_PREFIXES
        .iter()
        .any(|prefix| class_name.starts_with(prefix))
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
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
    files.iter()
        .find(|path| !is_clutter_markdown_path(path))
        .cloned()
        .or_else(|| files.first().cloned())
}

fn is_clutter_markdown_path(path: &Path) -> bool {
    path.components().any(|component| {
        let std::path::Component::Normal(segment) = component else {
            return false;
        };

        is_clutter_folder_name(segment.to_string_lossy().as_ref())
    })
}

fn is_clutter_directory(path: &Path) -> bool {
    path.file_name()
        .map(|name| is_clutter_folder_name(name.to_string_lossy().as_ref()))
        .unwrap_or(false)
}

fn is_clutter_folder_name(name: &str) -> bool {
    let folder_name = name.to_ascii_lowercase();
    CLUTTER_FOLDER_NAMES.contains(&folder_name.as_str())
}

fn error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::{is_clutter_markdown_path, load_document, load_folder, render_markdown, save_document};
    use std::path::Path;
    use std::{
        env,
        fs,
        path::PathBuf,
        time::Instant,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn highlighted_fence_keeps_syntect_classes() {
        let html = render_markdown("```python\nprint('hello')\n```");

        assert!(html.contains("code-block--highlighted"));
        assert!(html.contains("language-python"));
        assert!(html.contains("syn-"));
    }

    #[test]
    fn unknown_language_falls_back_to_plain_text() {
        let html = render_markdown("```madeup\nhello\n```");

        assert!(html.contains("<pre class=\"code-block\">"));
        assert!(html.contains("language-plain"));
        assert!(!html.contains("code-block--highlighted"));
    }

    #[test]
    fn diff_fence_renders_added_and_removed_lines() {
        let html = render_markdown("```diff\n-old\n+new\n@@ line @@\n```");

        assert!(html.contains("diff-line--remove"));
        assert!(html.contains("diff-line--add"));
        assert!(html.contains("diff-line--hunk"));
    }

    #[test]
    fn diff_renderer_escapes_html() {
        let html = render_markdown("```diff\n+<script>alert(1)</script>\n```");

        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn raw_html_still_has_unsafe_attributes_stripped() {
        let html = render_markdown("<span onclick=\"alert(1)\">safe</span>");

        assert!(html.contains("<span>safe</span>"));
        assert!(!html.contains("onclick"));
    }

    #[test]
    fn regular_markdown_links_survive_sanitization() {
        let html = render_markdown("[example](https://example.com)");

        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains(">example</a>"));
    }

    #[test]
    fn save_document_updates_source_and_html() {
        let temp_dir = create_temp_test_dir("save");
        let file_path = temp_dir.join("note.md");
        fs::write(&file_path, "# Before\n").unwrap();

        let saved = save_document(file_path.clone(), "# After\n".to_string()).unwrap();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "# After\n");
        assert_eq!(saved.source, "# After\n");
        assert!(saved.html.contains("<h1>After</h1>"));

        remove_temp_test_dir(temp_dir);
    }

    #[test]
    fn load_folder_picks_sorted_first_markdown_file() {
        let temp_dir = create_temp_test_dir("folder");
        fs::write(temp_dir.join("notes.md"), "# Notes\n").unwrap();
        fs::write(temp_dir.join("README.md"), "# Readme\n").unwrap();

        let folder = load_folder(&temp_dir, true).unwrap();

        assert!(folder.document.path.unwrap().ends_with("README.md"));
        assert_eq!(folder.files.len(), 2);

        remove_temp_test_dir(temp_dir);
    }

    #[test]
    fn load_folder_skips_clutter_markdown_for_initial_document() {
        let temp_dir = create_temp_test_dir("folder-clutter");
        let clutter_dir = temp_dir.join(".venv").join("docs");
        fs::create_dir_all(&clutter_dir).unwrap();
        fs::write(clutter_dir.join("api.md"), "# API\n").unwrap();
        fs::write(temp_dir.join("guide.md"), "# Guide\n").unwrap();

        let folder = load_folder(&temp_dir, true).unwrap();

        assert!(folder.document.path.unwrap().ends_with("guide.md"));
        assert_eq!(folder.files.len(), 2);

        remove_temp_test_dir(temp_dir);
    }

    #[test]
    fn clutter_detection_matches_hidden_folder_names() {
        assert!(is_clutter_markdown_path(Path::new("/tmp/project/.venv/docs/api.md")));
        assert!(!is_clutter_markdown_path(Path::new("/tmp/project/docs/api.md")));
    }

    #[test]
    #[ignore]
    fn benchmark_render_markdown_plain_document() {
        let source = create_plain_benchmark_markdown(500);
        benchmark_render_case("render_plain", &source, 60);
    }

    #[test]
    #[ignore]
    fn benchmark_render_markdown_code_heavy_document() {
        let source = create_code_heavy_benchmark_markdown(180);
        benchmark_render_case("render_code_heavy", &source, 30);
    }

    #[test]
    #[ignore]
    fn benchmark_render_markdown_huge_code_block_document() {
        let source = create_huge_code_benchmark_markdown(6000);
        benchmark_render_case("render_huge_code_block", &source, 10);
    }

    #[test]
    #[ignore]
    fn benchmark_render_markdown_huge_diff_block_document() {
        let source = create_huge_diff_benchmark_markdown(6000);
        benchmark_render_case("render_huge_diff_block", &source, 10);
    }

    #[test]
    #[ignore]
    fn benchmark_load_document_plain_file() {
        let temp_dir = create_temp_test_dir("bench-load");
        let file_path = temp_dir.join("benchmark.md");
        fs::write(&file_path, create_plain_benchmark_markdown(500)).unwrap();

        let mut durations = Vec::new();
        for _ in 0..5 {
            let _ = load_document(&file_path).unwrap();
        }

        for _ in 0..60 {
            let start = Instant::now();
            let payload = load_document(&file_path).unwrap();
            let elapsed = start.elapsed();
            assert!(!payload.html.is_empty());
            durations.push(elapsed.as_secs_f64() * 1000.0);
        }

        print_benchmark_result("load_plain_file", &durations);
        remove_temp_test_dir(temp_dir);
    }

    #[test]
    #[ignore]
    fn benchmark_load_folder_with_clutter_tree() {
        let temp_dir = create_temp_test_dir("bench-folder");
        fs::write(temp_dir.join("README.md"), "# Readme\n").unwrap();

        for index in 0..80 {
            let docs_dir = temp_dir.join("docs").join(format!("section-{index:03}"));
            fs::create_dir_all(&docs_dir).unwrap();
            fs::write(
                docs_dir.join("page.md"),
                format!("# Section {index}\n\n{}", create_plain_benchmark_markdown(10)),
            )
            .unwrap();
        }

        for index in 0..30 {
            let clutter_dir = temp_dir
                .join("node_modules")
                .join(format!("package-{index:03}"))
                .join("docs");
            fs::create_dir_all(&clutter_dir).unwrap();
            fs::write(
                clutter_dir.join("generated.md"),
                format!("# Generated {index}\n\n{}", create_plain_benchmark_markdown(10)),
            )
            .unwrap();
        }

        let mut durations = Vec::new();
        for _ in 0..5 {
            let _ = load_folder(&temp_dir, false).unwrap();
        }

        for _ in 0..30 {
            let start = Instant::now();
            let payload = load_folder(&temp_dir, false).unwrap();
            let elapsed = start.elapsed();
            assert!(!payload.files.is_empty());
            durations.push(elapsed.as_secs_f64() * 1000.0);
        }

        print_benchmark_result("load_folder_skipping_clutter", &durations);
        remove_temp_test_dir(temp_dir);
    }

    #[test]
    #[ignore]
    fn benchmark_load_folder_including_clutter_tree() {
        let temp_dir = create_temp_test_dir("bench-folder-full");
        fs::write(temp_dir.join("README.md"), "# Readme\n").unwrap();

        for index in 0..80 {
            let docs_dir = temp_dir.join("docs").join(format!("section-{index:03}"));
            fs::create_dir_all(&docs_dir).unwrap();
            fs::write(
                docs_dir.join("page.md"),
                format!("# Section {index}\n\n{}", create_plain_benchmark_markdown(10)),
            )
            .unwrap();
        }

        for index in 0..30 {
            let clutter_dir = temp_dir
                .join("node_modules")
                .join(format!("package-{index:03}"))
                .join("docs");
            fs::create_dir_all(&clutter_dir).unwrap();
            fs::write(
                clutter_dir.join("generated.md"),
                format!("# Generated {index}\n\n{}", create_plain_benchmark_markdown(10)),
            )
            .unwrap();
        }

        let mut durations = Vec::new();
        for _ in 0..5 {
            let _ = load_folder(&temp_dir, true).unwrap();
        }

        for _ in 0..30 {
            let start = Instant::now();
            let payload = load_folder(&temp_dir, true).unwrap();
            let elapsed = start.elapsed();
            assert!(!payload.files.is_empty());
            durations.push(elapsed.as_secs_f64() * 1000.0);
        }

        print_benchmark_result("load_folder_including_clutter", &durations);
        remove_temp_test_dir(temp_dir);
    }

    fn benchmark_render_case(label: &str, source: &str, iterations: usize) {
        let mut durations = Vec::new();

        for _ in 0..5 {
            let html = render_markdown(source);
            assert!(!html.is_empty());
        }

        for _ in 0..iterations {
            let start = Instant::now();
            let html = render_markdown(source);
            let elapsed = start.elapsed();
            assert!(!html.is_empty());
            durations.push(elapsed.as_secs_f64() * 1000.0);
        }

        print_benchmark_result(label, &durations);
    }

    fn print_benchmark_result(label: &str, durations_ms: &[f64]) {
        let total = durations_ms.iter().sum::<f64>();
        let average = total / durations_ms.len() as f64;
        let best = durations_ms.iter().copied().fold(f64::INFINITY, f64::min);
        let worst = durations_ms.iter().copied().fold(0.0, f64::max);
        println!(
            "BENCH {label} avg_ms={average:.3} best_ms={best:.3} worst_ms={worst:.3} iterations={}",
            durations_ms.len()
        );
    }

    fn create_plain_benchmark_markdown(section_count: usize) -> String {
        let mut source = String::new();
        for index in 0..section_count {
            source.push_str(&format!(
                "## Section {index}\n\nThis is a paragraph with a [link](https://example.com/{index}) and some `inline code`.\n\n- Item one\n- Item two\n- Item three\n\n> Quoted text for section {index}.\n\n"
            ));
        }
        source
    }

    fn create_code_heavy_benchmark_markdown(block_count: usize) -> String {
        let mut source = String::new();
        for index in 0..block_count {
            source.push_str(&format!(
                "## Example {index}\n\n```rust\nfn sample_{index}() {{\n    println!(\"hello {index}\");\n}}\n```\n\n```diff\n-old line {index}\n+new line {index}\n@@ hunk {index} @@\n```\n\n"
            ));
        }
        source
    }

    fn create_huge_code_benchmark_markdown(line_count: usize) -> String {
        let mut source = String::from("## Huge Block\n\n```rust\n");
        for index in 0..line_count {
            source.push_str(&format!(
                "fn sample_{index}() {{ println!(\"line {index}: {{}}\", {index}); }}\n"
            ));
        }
        source.push_str("```\n");
        source
    }

    fn create_huge_diff_benchmark_markdown(line_count: usize) -> String {
        let mut source = String::from("## Huge Diff\n\n```diff\n");
        for index in 0..line_count {
            let prefix = match index % 4 {
                0 => "+",
                1 => "-",
                2 => "@@ hunk @@ ",
                _ => " ",
            };
            source.push_str(&format!("{prefix}line {index}\n"));
        }
        source.push_str("```\n");
        source
    }

    fn create_temp_test_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "barebones-markdown-viewer-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn remove_temp_test_dir(path: PathBuf) {
        fs::remove_dir_all(path).unwrap();
    }
}
