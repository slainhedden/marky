import assert from "node:assert/strict";
import test from "node:test";

import { createApp, readSingleDialogSelection } from "./viewer.js";

function createElement() {
  return {
    disabled: false,
    hidden: false,
    innerHTML: "",
    listeners: {},
    scrollTop: 99,
    textContent: "",
    value: "",
    addEventListener(type, handler) {
      this.listeners[type] = handler;
    },
    click() {
      this.listeners.click?.({ preventDefault() {}, target: this });
    },
    focus() {},
    querySelectorAll() {
      return [];
    }
  };
}

function createElements() {
  return {
    dirtyIndicator: { ...createElement(), hidden: true },
    documentShell: createElement(),
    emptyOpenFileButton: createElement(),
    emptyOpenFolderButton: createElement(),
    emptyState: createElement(),
    errorBanner: { ...createElement(), hidden: true },
    fileTree: createElement(),
    renderedView: createElement(),
    saveButton: createElement(),
    sidebar: createElement(),
    sourceView: createElement(),
    themeButton: createElement(),
    toggleViewButton: createElement(),
    toolbarOpenFileButton: createElement(),
    toolbarOpenFolderButton: createElement()
  };
}

function createDocumentObject() {
  return {
    documentElement: {
      dataset: {}
    },
    body: {
      classList: {
        add() {},
        remove() {}
      }
    }
  };
}

function createStorage(initialValue) {
  const values = new Map();
  if (initialValue) {
    values.set("barebones-markdown-viewer-theme", initialValue);
  }

  return {
    getItem(key) {
      return values.get(key) ?? null;
    },
    setItem(key, value) {
      values.set(key, value);
    }
  };
}

function createWindowObject() {
  return {
    listeners: {},
    addEventListener(type, handler) {
      this.listeners[type] = handler;
    },
    setTimeout(handler) {
      handler();
      return 1;
    }
  };
}

function createAppWindow() {
  return {
    closeCalls: 0,
    closeHandler: null,
    dragDropHandler: null,
    async close() {
      this.closeCalls += 1;
    },
    async destroy() {},
    async onCloseRequested(handler) {
      this.closeHandler = handler;
    },
    async onDragDropEvent(handler) {
      this.dragDropHandler = handler;
    }
  };
}

test("openFile shows the selected document", async () => {
  const elements = createElements();
  const calls = [];
  const documentObject = createDocumentObject();
  const app = createApp({
    appWindow: createAppWindow(),
    documentObject,
    elements,
    listen: async () => {},
    openDialog: async () => ["file:///C:/notes/example.md"],
    showMessage: async () => {
      throw new Error("showMessage should not be called");
    },
    storage: createStorage(),
    windowObject: createWindowObject(),
    invoke: async (command, args) => {
      calls.push({ command, args });

      if (command === "get_launch_document") {
        return null;
      }

      if (command === "open_markdown_path") {
        assert.deepEqual(args, { path: "C:/notes/example.md" });
        return {
          fileName: "example.md",
          html: "<h1>Example</h1>",
          path: "C:/notes/example.md",
          source: "# Example"
        };
      }

      throw new Error(`Unexpected command: ${command}`);
    }
  });

  await app.boot();
  await app.openFile();

  assert.deepEqual(calls, [
    { command: "get_launch_document", args: undefined },
    { command: "open_markdown_path", args: { path: "C:/notes/example.md" } }
  ]);
  assert.equal(app.state.document?.path, "C:/notes/example.md");
  assert.equal(elements.emptyState.hidden, true);
  assert.equal(elements.documentShell.hidden, false);
  assert.equal(elements.renderedView.hidden, false);
  assert.equal(elements.renderedView.innerHTML, "<h1>Example</h1>");
  assert.equal(elements.sourceView.value, "# Example");
  assert.equal(elements.renderedView.scrollTop, 0);
  assert.equal(elements.sourceView.scrollTop, 0);
  assert.equal(elements.errorBanner.hidden, true);
  assert.equal(documentObject.documentElement.dataset.theme, "midnight");
  assert.equal(elements.themeButton.textContent, "Theme: Midnight");
});

test("editing source marks the document dirty and save updates the document", async () => {
  const elements = createElements();
  const calls = [];
  const app = createApp({
    appWindow: createAppWindow(),
    documentObject: createDocumentObject(),
    elements,
    listen: async () => {},
    showMessage: async () => {
      throw new Error("showMessage should not be called");
    },
    storage: createStorage(),
    windowObject: createWindowObject(),
    invoke: async (command, args) => {
      calls.push({ command, args });

      if (command === "get_launch_document") {
        return null;
      }

      if (command === "save_markdown_path") {
        assert.deepEqual(args, {
          path: "C:/notes/example.md",
          source: "# Updated"
        });

        return {
          fileName: "example.md",
          html: "<h1>Updated</h1>",
          path: "C:/notes/example.md",
          source: "# Updated"
        };
      }

      throw new Error(`Unexpected command: ${command}`);
    }
  });

  await app.boot();
  app.showDocument({
    fileName: "example.md",
    html: "<h1>Example</h1>",
    path: "C:/notes/example.md",
    source: "# Example"
  });

  elements.toggleViewButton.click();
  elements.sourceView.value = "# Updated";
  elements.sourceView.listeners.input?.({ target: elements.sourceView });

  assert.equal(app.state.isDirty, true);
  assert.equal(elements.saveButton.disabled, false);
  assert.equal(elements.dirtyIndicator.hidden, false);

  await app.saveDocument();

  assert.deepEqual(calls, [
    { command: "get_launch_document", args: undefined },
    {
      command: "save_markdown_path",
      args: { path: "C:/notes/example.md", source: "# Updated" }
    }
  ]);
  assert.equal(app.state.isDirty, false);
  assert.equal(app.state.viewMode, "source");
  assert.equal(elements.renderedView.innerHTML, "<h1>Updated</h1>");
  assert.equal(elements.sourceView.value, "# Updated");
  assert.equal(elements.dirtyIndicator.hidden, true);
});

test("openFile cancels when unsaved changes are not discarded", async () => {
  const elements = createElements();
  const calls = [];
  let openDialogCalls = 0;
  const app = createApp({
    appWindow: createAppWindow(),
    documentObject: createDocumentObject(),
    elements,
    listen: async () => {},
    openDialog: async () => {
      openDialogCalls += 1;
      return ["C:/notes/other.md"];
    },
    showMessage: async () => "Cancel",
    storage: createStorage(),
    windowObject: createWindowObject(),
    invoke: async (command, args) => {
      calls.push({ command, args });

      if (command === "get_launch_document") {
        return null;
      }

      throw new Error(`Unexpected command: ${command}`);
    }
  });

  await app.boot();
  app.showDocument({
    fileName: "example.md",
    html: "<h1>Example</h1>",
    path: "C:/notes/example.md",
    source: "# Example"
  });

  elements.toggleViewButton.click();
  elements.sourceView.value = "# Changed";
  elements.sourceView.listeners.input?.({ target: elements.sourceView });

  await app.openFile();

  assert.equal(openDialogCalls, 1);
  assert.deepEqual(calls, [{ command: "get_launch_document", args: undefined }]);
  assert.equal(app.state.document?.path, "C:/notes/example.md");
  assert.equal(app.state.isDirty, true);
});

test("folder mode file switches use direct document opens", async () => {
  const elements = createElements();
  const calls = [];
  const app = createApp({
    appWindow: createAppWindow(),
    documentObject: createDocumentObject(),
    elements,
    listen: async () => {},
    openDialog: async () => ["C:/notes"],
    showMessage: async () => {
      throw new Error("showMessage should not be called");
    },
    storage: createStorage(),
    windowObject: createWindowObject(),
    invoke: async (command, args) => {
      calls.push({ command, args });

      if (command === "get_launch_document") {
        return null;
      }

      if (command === "open_markdown_folder") {
        assert.deepEqual(args, { folderPath: "C:/notes" });
        return {
          document: {
            fileName: "README.md",
            html: "<h1>Readme</h1>",
            path: "C:/notes/README.md",
            source: "# Readme"
          },
          files: [
            { name: "README.md", path: "C:/notes/README.md", relativePath: "README.md" },
            { name: "next.md", path: "C:/notes/next.md", relativePath: "next.md" }
          ],
          folderPath: "C:/notes"
        };
      }

      if (command === "open_markdown_path") {
        assert.deepEqual(args, { path: "C:/notes/next.md" });
        return {
          fileName: "next.md",
          html: "<h1>Next</h1>",
          path: "C:/notes/next.md",
          source: "# Next"
        };
      }

      throw new Error(`Unexpected command: ${command}`);
    }
  });

  await app.boot();
  await app.openFolder();
  await app.openPath("C:/notes/next.md");

  assert.deepEqual(calls, [
    { command: "get_launch_document", args: undefined },
    { command: "open_markdown_folder", args: { folderPath: "C:/notes" } },
    { command: "open_markdown_path", args: { path: "C:/notes/next.md" } }
  ]);
  assert.equal(app.state.folderPath, "C:/notes");
  assert.equal(app.state.document?.path, "C:/notes/next.md");
});

test("readSingleDialogSelection unwraps one-item arrays", () => {
  assert.equal(readSingleDialogSelection(["C:/notes/example.md"], "file"), "C:/notes/example.md");
});

test("readSingleDialogSelection rejects multi-select results", () => {
  assert.throws(
    () => readSingleDialogSelection(["C:/one.md", "C:/two.md"], "file"),
    /multiple paths/
  );
});

test("boot loads the saved theme and cycleTheme persists the next one", async () => {
  const elements = createElements();
  const documentObject = createDocumentObject();
  const storage = createStorage("paper");
  const app = createApp({
    appWindow: createAppWindow(),
    documentObject,
    elements,
    listen: async () => {},
    showMessage: async () => {
      throw new Error("showMessage should not be called");
    },
    storage,
    windowObject: createWindowObject(),
    invoke: async (command) => {
      if (command === "get_launch_document") {
        return null;
      }

      throw new Error(`Unexpected command: ${command}`);
    }
  });

  await app.boot();
  assert.equal(app.state.themeId, "paper");
  assert.equal(documentObject.documentElement.dataset.theme, "paper");
  assert.equal(elements.themeButton.textContent, "Theme: Paper");

  elements.themeButton.click();

  assert.equal(app.state.themeId, "forest");
  assert.equal(documentObject.documentElement.dataset.theme, "forest");
  assert.equal(elements.themeButton.textContent, "Theme: Forest");
  assert.equal(storage.getItem("barebones-markdown-viewer-theme"), "forest");
});

test("close requests with unsaved changes schedule a second close", async () => {
  const elements = createElements();
  const appWindow = createAppWindow();
  const app = createApp({
    appWindow,
    documentObject: createDocumentObject(),
    elements,
    listen: async () => {},
    showMessage: async () => "Discard",
    storage: createStorage(),
    windowObject: createWindowObject(),
    invoke: async (command) => {
      if (command === "get_launch_document") {
        return null;
      }

      throw new Error(`Unexpected command: ${command}`);
    }
  });

  await app.boot();
  app.showDocument({
    fileName: "example.md",
    html: "<h1>Example</h1>",
    path: "C:/notes/example.md",
    source: "# Example"
  });

  elements.toggleViewButton.click();
  elements.sourceView.value = "# Changed";
  elements.sourceView.listeners.input?.({ target: elements.sourceView });

  let prevented = false;
  await appWindow.closeHandler?.({
    preventDefault() {
      prevented = true;
    }
  });

  assert.equal(prevented, true);
  assert.equal(appWindow.closeCalls, 1);
  assert.equal(app.state.allowWindowClose, true);

  await appWindow.closeHandler?.({
    preventDefault() {
      throw new Error("second close should not be prevented");
    }
  });

  assert.equal(app.state.allowWindowClose, false);
});
