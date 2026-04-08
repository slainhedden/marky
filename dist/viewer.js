const MARKDOWN_EXTENSIONS = ["md", "markdown", "mdown", "mkd"];

function createState() {
  return {
    document: null,
    viewMode: "rendered"
  };
}

function getBrowserElements(documentObject) {
  return {
    documentShell: documentObject.querySelector("#document-shell"),
    emptyOpenFileButton: documentObject.querySelector("#empty-open-file-button"),
    emptyState: documentObject.querySelector("#empty-state"),
    errorBanner: documentObject.querySelector("#error-banner"),
    renderedView: documentObject.querySelector("#rendered-view"),
    sourceView: documentObject.querySelector("#source-view"),
    toggleViewButton: documentObject.querySelector("#toggle-view-button"),
    toolbarOpenFileButton: documentObject.querySelector("#toolbar-open-file-button")
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
  windowObject,
  openDialog = async (options) => await invoke("plugin:dialog|open", { options })
}) {
  const state = createState();

  async function boot() {
    bindUi();
    applyShellState();

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

    elements.toggleViewButton.addEventListener("click", toggleViewMode);

    windowObject.addEventListener("keydown", (event) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void runAction(openFile);
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

    if (!selectedPath || selectedPath === state.document?.path) {
      return;
    }

    const documentPayload = await invoke("open_markdown_path", {
      path: selectedPath
    });

    showDocument(documentPayload);
  }

  function showDocument(documentPayload) {
    state.document = documentPayload;
    state.viewMode = "rendered";

    elements.renderedView.innerHTML = documentPayload.html;
    elements.sourceView.textContent = documentPayload.source;
    elements.renderedView.scrollTop = 0;
    elements.sourceView.scrollTop = 0;

    applyShellState();
    clearError();
  }

  function toggleViewMode() {
    if (!state.document) {
      return;
    }

    state.viewMode = state.viewMode === "source" ? "rendered" : "source";
    applyShellState();
  }

  function applyShellState() {
    const hasDocument = Boolean(state.document);
    const showingSource = state.viewMode === "source";

    elements.emptyState.hidden = hasDocument;
    elements.documentShell.hidden = !hasDocument;
    elements.renderedView.hidden = !hasDocument || showingSource;
    elements.sourceView.hidden = !hasDocument || !showingSource;
    elements.toggleViewButton.hidden = !hasDocument;
    elements.toggleViewButton.disabled = !hasDocument;
    elements.toggleViewButton.textContent = showingSource ? "Show Rendered" : "Show Source";
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
    showDocument,
    showError
  };
}

export function createBrowserApp() {
  const tauri = window.__TAURI__;
  return createApp({
    elements: getBrowserElements(document),
    invoke: tauri.core.invoke,
    windowObject: window
  });
}

if (typeof window !== "undefined" && typeof document !== "undefined") {
  const app = createBrowserApp();
  app.boot().catch(app.showError);
}
