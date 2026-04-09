const MARKDOWN_EXTENSIONS = ["md", "markdown", "mdown", "mkd"];
const THEMES = [
  { id: "midnight", label: "Midnight" },
  { id: "paper", label: "Paper" },
  { id: "forest", label: "Forest" }
];
const THEME_STORAGE_KEY = "barebones-markdown-viewer-theme";

function createState() {
  return {
    allowWindowClose: false,
    document: null,
    draftSource: "",
    files: [],
    folderPath: null,
    isDirty: false,
    isSaving: false,
    themeId: THEMES[0].id,
    viewMode: "rendered"
  };
}

function getBrowserElements(documentObject) {
  return {
    dirtyIndicator: documentObject.querySelector("#dirty-indicator"),
    documentShell: documentObject.querySelector("#document-shell"),
    emptyOpenFileButton: documentObject.querySelector("#empty-open-file-button"),
    emptyOpenFolderButton: documentObject.querySelector("#empty-open-folder-button"),
    emptyState: documentObject.querySelector("#empty-state"),
    errorBanner: documentObject.querySelector("#error-banner"),
    fileTree: documentObject.querySelector("#file-tree"),
    renderedView: documentObject.querySelector("#rendered-view"),
    saveButton: documentObject.querySelector("#save-button"),
    sidebar: documentObject.querySelector("#sidebar"),
    sourceView: documentObject.querySelector("#source-view"),
    themeButton: documentObject.querySelector("#theme-button"),
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
  storage,
  openDialog = async (options) => await invoke("plugin:dialog|open", { options }),
  showMessage = async (message, options) =>
    await invoke("plugin:dialog|message", {
      buttons: normalizeDialogButtons(options?.buttons),
      kind: options?.kind,
      message,
      title: options?.title
    })
}) {
  const state = createState();

  async function boot() {
    loadTheme();
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

    elements.saveButton.addEventListener("click", () => {
      void runAction(saveDocument);
    });

    elements.sourceView.addEventListener("input", () => {
      updateDraftSource(elements.sourceView.value);
    });

    elements.themeButton.addEventListener("click", cycleTheme);
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

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void runAction(saveDocument);
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

    if (!(await confirmBeforeNavigation("Save changes before opening a different file?"))) {
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

    if (!(await confirmBeforeNavigation("Save changes before opening a folder?"))) {
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
        if (!(await confirmBeforeNavigation("Save changes before opening a different file?"))) {
          return;
        }

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

            if (!(await confirmBeforeNavigation("Save changes before opening a different file?"))) {
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

    await appWindow.onCloseRequested(async (event) => {
      if (state.allowWindowClose) {
        state.allowWindowClose = false;
        return;
      }

      if (!state.isDirty) {
        return;
      }

      event.preventDefault();

      try {
        if (!(await confirmBeforeNavigation("Save changes before closing the window?"))) {
          return;
        }

        state.allowWindowClose = true;
        windowObject.setTimeout(async () => {
          try {
            await appWindow.close();
          } catch (error) {
            state.allowWindowClose = false;
            showError(error);
          }
        }, 0);
      } catch (error) {
        state.allowWindowClose = false;
        showError(error);
      }
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

  async function saveDocument() {
    if (!state.document || !state.isDirty || state.isSaving) {
      return false;
    }

    state.isSaving = true;
    applyShellState();

    try {
      const savedDocument = await invoke("save_markdown_path", {
        path: state.document.path,
        source: state.draftSource
      });

      showDocument(savedDocument, {
        preserveViewMode: true,
        resetScroll: false
      });

      return true;
    } catch (error) {
      state.isSaving = false;
      applyShellState();
      throw error;
    }
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

    if (!isExternalHref(href)) {
      if (!(await confirmBeforeNavigation("Save changes before following a different markdown link?"))) {
        return;
      }
    }

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

    if (state.folderPath) {
      clearFolderState();
    }

    showDocument(nextDocument);
  }

  async function confirmBeforeNavigation(actionDescription) {
    if (!state.document || !state.isDirty) {
      return true;
    }

    const fileName = state.document.fileName ?? "This file";
    const result = await showMessage(
      `${fileName} has unsaved changes.\n\n${actionDescription}`,
      {
        buttons: { yes: "Save", no: "Discard", cancel: "Cancel" },
        kind: "warning",
        title: "Unsaved Changes"
      }
    );

    if (result === "Save") {
      await saveDocument();
      return true;
    }

    return result === "Discard";
  }

  function showFolder(folderPayload) {
    state.folderPath = folderPayload.folderPath;
    state.files = folderPayload.files;
    showDocument(folderPayload.document);
  }

  function showDocument(documentPayload, options = {}) {
    const { preserveViewMode = false, resetScroll = true } = options;

    state.document = documentPayload;
    state.draftSource = documentPayload.source;
    state.isDirty = false;
    state.isSaving = false;

    if (!preserveViewMode) {
      state.viewMode = "rendered";
    }

    elements.emptyState.hidden = true;
    elements.documentShell.hidden = false;

    elements.renderedView.innerHTML = documentPayload.html;
    elements.sourceView.value = documentPayload.source;

    if (resetScroll) {
      resetViewerScroll();
    }

    renderFileTree();
    applyShellState();
    clearError();
  }

  function updateDraftSource(nextSource) {
    state.draftSource = nextSource;
    state.isDirty = Boolean(state.document) && nextSource !== state.document.source;
    applyShellState();
  }

  function clearFolderState() {
    state.folderPath = null;
    state.files = [];
    elements.fileTree.innerHTML = "";
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

    elements.saveButton.disabled = !hasDocument || !state.isDirty || state.isSaving;
    elements.saveButton.textContent = state.isSaving ? "Saving..." : "Save";

    elements.dirtyIndicator.hidden = !hasDocument || !state.isDirty;
  }

  function loadTheme() {
    const themeId = storage?.getItem(THEME_STORAGE_KEY);
    applyTheme(themeId);
  }

  function applyTheme(themeId) {
    const theme = getTheme(themeId);
    state.themeId = theme.id;
    documentObject.documentElement.dataset.theme = theme.id;
    elements.themeButton.textContent = `Theme: ${theme.label}`;
    storage?.setItem(THEME_STORAGE_KEY, theme.id);
  }

  function cycleTheme() {
    const currentThemeIndex = THEMES.findIndex((theme) => theme.id === state.themeId);
    const nextTheme = THEMES[(currentThemeIndex + 1) % THEMES.length];
    applyTheme(nextTheme.id);
  }

  function toggleViewMode() {
    if (!state.document) {
      return;
    }

    state.viewMode = state.viewMode === "source" ? "rendered" : "source";
    resetViewerScroll();
    applyShellState();

    if (state.viewMode === "source") {
      elements.sourceView.focus();
    }
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
          if (!(await confirmBeforeNavigation("Save changes before opening a different file?"))) {
            return;
          }

          await openPath(path);
        });
      });
    }
  }

  function resetViewerScroll() {
    elements.renderedView.scrollTop = 0;
    elements.sourceView.scrollTop = 0;
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
    applyShellState,
    boot,
    clearFolderState,
    openFile,
    openFolder,
    openPath,
    runAction,
    saveDocument,
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
    windowObject: window,
    storage: window.localStorage
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

function getTheme(themeId) {
  return THEMES.find((theme) => theme.id === themeId) ?? THEMES[0];
}

function normalizeDialogButtons(buttons) {
  if (!buttons || typeof buttons === "string") {
    return buttons;
  }

  if ("ok" in buttons && "cancel" in buttons) {
    return { OkCancelCustom: [buttons.ok, buttons.cancel] };
  }

  if ("yes" in buttons && "no" in buttons && "cancel" in buttons) {
    return { YesNoCancelCustom: [buttons.yes, buttons.no, buttons.cancel] };
  }

  if ("ok" in buttons) {
    return { OkCustom: buttons.ok };
  }

  return undefined;
}

function isExternalHref(href) {
  return matchesProtocol(href, "http", "https", "mailto", "tel");
}

function matchesProtocol(href, ...protocols) {
  return protocols.includes(href.split(":").shift());
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
