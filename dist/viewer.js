const MARKDOWN_EXTENSIONS = ["md", "markdown", "mdown", "mkd"];

function createState() {
  return {
    document: null,
    files: [],
    folderPath: null,
    viewMode: "rendered"
  };
}

function getBrowserElements(documentObject) {
  return {
    documentShell: documentObject.querySelector("#document-shell"),
    emptyOpenFileButton: documentObject.querySelector("#empty-open-file-button"),
    emptyOpenFolderButton: documentObject.querySelector("#empty-open-folder-button"),
    emptyState: documentObject.querySelector("#empty-state"),
    errorBanner: documentObject.querySelector("#error-banner"),
    fileTree: documentObject.querySelector("#file-tree"),
    renderedView: documentObject.querySelector("#rendered-view"),
    sidebar: documentObject.querySelector("#sidebar"),
    sourceView: documentObject.querySelector("#source-view"),
    toggleViewButton: documentObject.querySelector("#toggle-view-button"),
    toolbarOpenFileButton: documentObject.querySelector("#toolbar-open-file-button"),
    toolbarOpenFolderButton: documentObject.querySelector("#toolbar-open-folder-button")
  };
}

export function readSingleDialogSelection(selection, label) {
  if (selection === null || selection === undefined) {
    return null;
  }

  if (typeof selection === "string") {
    return normalizeDialogPath(selection);
  }

  if (!Array.isArray(selection)) {
    throw new Error(`The ${label} dialog returned an unsupported value.`);
  }

  if (selection.length === 0) {
    return null;
  }

  if (selection.length !== 1 || typeof selection[0] !== "string") {
    throw new Error(`The ${label} dialog returned multiple paths.`);
  }

  return normalizeDialogPath(selection[0]);
}

function normalizeDialogPath(path) {
  if (!path.startsWith("file://")) {
    return path;
  }

  const url = new URL(path);
  const pathname = decodeURIComponent(url.pathname);
  if (/^\/[A-Za-z]:/.test(pathname)) {
    return pathname.slice(1);
  }

  return pathname;
}

export function createApp({
  elements,
  invoke,
  listen,
  appWindow,
  documentObject,
  windowObject,
  openDialog = async (options) => await invoke("plugin:dialog|open", { options })
}) {
  const state = createState();

  async function boot() {
    bindUi();
    applyShellState();
    await bindWindowEvents();

    const launchDocument = await invoke("get_launch_document");
    if (launchDocument) {
      showDocument(launchDocument);
    }
  }

  function bindUi() {
    for (const button of [elements.emptyOpenFileButton, elements.toolbarOpenFileButton]) {
      button?.addEventListener("click", () => {
        void runAction(openFile);
      });
    }

    for (const button of [elements.emptyOpenFolderButton, elements.toolbarOpenFolderButton]) {
      button?.addEventListener("click", () => {
        void runAction(openFolder);
      });
    }

    elements.toggleViewButton.addEventListener("click", toggleViewMode);
    elements.renderedView.addEventListener("click", (event) => {
      void runAction(async () => {
        await handleLinkClick(event);
      });
    });

    windowObject.addEventListener("keydown", (event) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void runAction(openFile);
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        void runAction(openFolder);
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key === "\\") {
        event.preventDefault();
        toggleViewMode();
      }
    });
  }

  async function openFile() {
    const selectedPath = readSingleDialogSelection(
      await openDialog({
        directory: false,
        multiple: false,
        filters: [{ name: "Markdown", extensions: MARKDOWN_EXTENSIONS }]
      }),
      "file"
    );

    if (!selectedPath || isCurrentPath(selectedPath)) {
      return;
    }

    clearFolderState();
    await openSingleFile(selectedPath);
  }

  async function openFolder() {
    const selectedPath = readSingleDialogSelection(
      await openDialog({
        directory: true,
        multiple: false
      }),
      "folder"
    );

    if (!selectedPath) {
      return;
    }

    const folderPayload = await invoke("open_markdown_folder", {
      folderPath: selectedPath
    });

    showFolder(folderPayload);
  }

  async function bindWindowEvents() {
    await listen("viewer://document-opened", ({ payload }) => {
      void runAction(async () => {
        clearFolderState();
        showDocument(payload);
      });
    });

    await appWindow.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        documentObject.body.classList.add("is-dragging");
        return;
      }

      if (event.payload.type === "drop") {
        documentObject.body.classList.remove("is-dragging");
        const [firstPath] = event.payload.paths;
        if (firstPath) {
          void runAction(async () => {
            if (isCurrentPath(firstPath)) {
              return;
            }

            clearFolderState();
            await openSingleFile(firstPath);
          });
        }
        return;
      }

      documentObject.body.classList.remove("is-dragging");
    });
  }

  async function openSingleFile(path) {
    const documentPayload = await invoke("open_markdown_path", { path });
    showDocument(documentPayload);
  }

  async function openPath(path) {
    if (!path || isCurrentPath(path)) {
      return;
    }

    await openSingleFile(path);
  }

  async function handleLinkClick(event) {
    const link = event.target.closest("a[href]");
    if (!link) {
      return;
    }

    const href = link.getAttribute("href")?.trim();
    if (!href || href.startsWith("#")) {
      return;
    }

    event.preventDefault();

    const nextDocument = await invoke("follow_link", {
      currentPath: state.document?.path ?? null,
      href
    });

    if (!nextDocument || isCurrentPath(nextDocument.path)) {
      return;
    }

    if (state.folderPath && isPathInFolder(nextDocument.path, state.folderPath)) {
      showDocument(nextDocument);
      return;
    }

    clearFolderState();
    showDocument(nextDocument);
  }

  function showFolder(folderPayload) {
    state.folderPath = folderPayload.folderPath;
    state.files = folderPayload.files;
    showDocument(folderPayload.document);
  }

  function showDocument(documentPayload) {
    state.document = documentPayload;
    state.viewMode = "rendered";

    elements.renderedView.innerHTML = documentPayload.html;
    elements.sourceView.textContent = documentPayload.source;
    elements.renderedView.scrollTop = 0;
    elements.sourceView.scrollTop = 0;

    renderFileTree();
    applyShellState();
    clearError();
  }

  function clearFolderState() {
    state.folderPath = null;
    state.files = [];
    elements.fileTree.innerHTML = "";
  }

  function toggleViewMode() {
    if (!state.document) {
      return;
    }

    state.viewMode = state.viewMode === "source" ? "rendered" : "source";
    applyShellState();
  }

  function renderFileTree() {
    if (!state.folderPath || state.files.length === 0) {
      elements.fileTree.innerHTML = "";
      return;
    }

    const activePath = state.document?.path ?? "";
    elements.fileTree.innerHTML = state.files
      .map((file) => {
        const buttonClass =
          file.path === activePath ? "file-tree__item is-active" : "file-tree__item";

        return `
        <button class="${buttonClass}" type="button" data-path="${escapeHtmlAttribute(file.path)}">
          <span>${escapeHtml(file.name)}</span>
          <span class="file-tree__path">${escapeHtml(file.relativePath)}</span>
        </button>
      `;
      })
      .join("");

    for (const button of elements.fileTree.querySelectorAll("[data-path]")) {
      button.addEventListener("click", () => {
        const path = button.getAttribute("data-path");
        if (!path || isCurrentPath(path)) {
          return;
        }

        void runAction(async () => {
          await openPath(path);
        });
      });
    }
  }

  function applyShellState() {
    const hasDocument = Boolean(state.document);
    const hasFolder = Boolean(state.folderPath);
    const showingSource = state.viewMode === "source";

    elements.emptyState.hidden = hasDocument;
    elements.documentShell.hidden = !hasDocument;
    elements.sidebar.hidden = !hasDocument || !hasFolder;
    elements.renderedView.hidden = !hasDocument || showingSource;
    elements.sourceView.hidden = !hasDocument || !showingSource;
    elements.toggleViewButton.hidden = !hasDocument;
    elements.toggleViewButton.disabled = !hasDocument;
    elements.toggleViewButton.textContent = showingSource ? "Show Rendered" : "Show Source";
  }

  function isCurrentPath(path) {
    return Boolean(path) && path === state.document?.path;
  }

  async function runAction(action) {
    try {
      clearError();
      await action();
    } catch (error) {
      showError(error);
    }
  }

  function showError(error) {
    const message = typeof error === "string" ? error : error?.message ?? String(error);
    elements.errorBanner.hidden = false;
    elements.errorBanner.textContent = message;
  }

  function clearError() {
    elements.errorBanner.hidden = true;
    elements.errorBanner.textContent = "";
  }

  return {
    state,
    boot,
    openFile,
    openFolder,
    openPath,
    showDocument,
    showError,
    showFolder
  };
}

export function createBrowserApp() {
  const tauri = window.__TAURI__;
  return createApp({
    elements: getBrowserElements(document),
    invoke: tauri.core.invoke,
    listen: tauri.event.listen,
    appWindow: tauri.window.getCurrentWindow(),
    documentObject: document,
    windowObject: window
  });
}

if (typeof window !== "undefined" && typeof document !== "undefined") {
  const app = createBrowserApp();
  app.boot().catch(app.showError);
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function escapeHtmlAttribute(value) {
  return escapeHtml(value).replaceAll("'", "&#39;");
}

function isPathInFolder(path, folderPath) {
  if (!path || !folderPath) {
    return false;
  }

  const normalizedPath = path.toLowerCase();
  const normalizedFolder = folderPath.toLowerCase().replace(/[\\\/]+$/, "");

  return (
    normalizedPath === normalizedFolder ||
    normalizedPath.startsWith(`${normalizedFolder}\\`) ||
    normalizedPath.startsWith(`${normalizedFolder}/`)
  );
}
