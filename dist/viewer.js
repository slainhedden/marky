const elements = {
  documentShell: document.querySelector("#document-shell"),
  emptyState: document.querySelector("#empty-state"),
  openFileButton: document.querySelector("#open-file-button"),
  renderedView: document.querySelector("#rendered-view"),
  sourceView: document.querySelector("#source-view"),
  toggleViewButton: document.querySelector("#toggle-view-button")
};

const state = {
  source: "",
  viewMode: "rendered"
};

function boot() {
  bindUi();
  applyShellState();
}

function bindUi() {
  elements.openFileButton.addEventListener("click", () => {
    window.alert("Open file support is coming next.");
  });

  elements.toggleViewButton.addEventListener("click", toggleViewMode);
}

function toggleViewMode() {
  if (!state.source) {
    return;
  }

  state.viewMode = state.viewMode === "source" ? "rendered" : "source";
  applyShellState();
}

function applyShellState() {
  const hasDocument = Boolean(state.source);
  const showingSource = state.viewMode === "source";

  elements.emptyState.hidden = hasDocument;
  elements.documentShell.hidden = !hasDocument;
  elements.renderedView.hidden = !hasDocument || showingSource;
  elements.sourceView.hidden = !hasDocument || !showingSource;
  elements.toggleViewButton.disabled = !hasDocument;
  elements.toggleViewButton.textContent = showingSource ? "Show Rendered" : "Show Source";
}

boot();
