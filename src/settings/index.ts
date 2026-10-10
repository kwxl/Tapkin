import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { createAutosave } from "../shared/autosave";
import { field, updateRangeProgress } from "../shared/dom";
import type { Patch, View } from "../shared/types";

export function initializeSettings({ getView, error }: {
  getView: () => View | undefined;
  error: (message: unknown) => void;
}) {
  let timingDirty = false;
  async function action(task: () => Promise<unknown>) {
    field("error").hidden = true;
    try {
      await task();
    } catch (e) {
      error(e);
    }
  }
  function renderSettings() {
    const view = getView();
    if (!view) return;
    field("skin-name").textContent = view.skin.name;
    field("skin-path").textContent = view.skin.directory;
    const width = field<HTMLInputElement>("width");
    if (document.activeElement !== width)
      width.value = String(Math.round(view.settings.window_size.width));
    updateRangeProgress(width);
    field("size-value").textContent =
      `${Math.round(view.settings.window_size.width)} × ${Math.round(view.settings.window_size.height)} px`;
    for (const key of ["typing_timeout_ms", "frame_hold_ms"] as const) {
      const control = field<HTMLInputElement>(key);
      if (!timingDirty && !control.disabled && document.activeElement !== control)
        control.value = String(view.settings[key]);
    }
    field<HTMLInputElement>("frame_hold_ms").max = field<HTMLInputElement>("typing_timeout_ms").value;
    for (const key of [
      "always_on_top",
      "click_through",
      "lock_position",
      "launch_at_login",
      "repeat_held_keys",
    ] as const) {
      field<HTMLInputElement>(key).checked = view.settings[key];
    }
    field("input-status").textContent = view.input.retrying
      ? "Starting listener…"
      : view.input.active
        ? "Listening for typing"
        : "Keyboard listener inactive";
    field("input-status").classList.toggle("active", view.input.active);
    field("permission-message").textContent =
      view.input.message ??
      "Tapkin detects keyboard activity without recording or saving what you type.";
    field<HTMLButtonElement>("retry").disabled = view.input.retrying;
    field("mac-permissions").hidden = view.platform !== "macos";
    field("warning").textContent = view.warning ?? "";
    field("warning").hidden = !view.warning;
  }

  field("choose-skin").addEventListener(
    "click",
    () =>
      void action(async () => {
        const directory = await open({
          directory: true,
          multiple: false,
          title: "Choose a Tapkin skin folder",
        });
        if (directory) await invoke("load_skin", { directory });
      }),
  );
  for (const [id, command] of [
    ["reload", "reload_skin"],
    ["open-folder", "open_skin_folder"],
    ["retry", "retry_listener"],
    ["quit", "quit_app"],
  ]) {
    field(id).addEventListener(
      "click",
      () => void action(() => invoke(command)),
    );
  }
  field("input-permission").addEventListener(
    "click",
    () =>
      void action(() => invoke("open_privacy_settings", { section: "input" })),
  );
  field("accessibility-permission").addEventListener(
    "click",
    () =>
      void action(() =>
        invoke("open_privacy_settings", { section: "accessibility" }),
      ),
  );
  for (const key of [
    "always_on_top",
    "click_through",
    "lock_position",
    "launch_at_login",
    "repeat_held_keys",
  ] as const) {
    const control = field<HTMLInputElement>(key);
    control.addEventListener(
      "change",
      () =>
        void action(async () => {
          const patch: Patch = { [key]: control.checked };
          control.disabled = true;
          try {
            await invoke("update_settings", { patch });
          } finally {
            control.disabled = false;
            renderSettings();
          }
        }),
    );
  }
  // Animation timing: delayed autosave.
  const timingForm = field<HTMLFormElement>("animation-timing");
  const timeout = field<HTMLInputElement>("typing_timeout_ms");
  const hold = field<HTMLInputElement>("frame_hold_ms");
  const timingStatus = field("timing-status");
  const timingSave = createAutosave<Patch>({
    delay: 400,
    save: (patch) => invoke("update_settings", { patch }),
    saved: () => {
      timingDirty = false;
      timingStatus.textContent = "Saved";
      renderSettings();
    },
    failed: (e) => {
      timingStatus.textContent = "Could not save. Edit a value to retry.";
      error(e);
    },
  });
  function queueTiming() {
    timingDirty = true;
    hold.max = timeout.value;
    if (!timingForm.checkValidity()) {
      timingSave.cancel();
      timingStatus.textContent = "Enter a timeout of 50–10000 ms and a frame hold of 0–timeout ms.";
      return;
    }
    timingStatus.textContent = "Changes pending…";
    timingSave.update({
      typing_timeout_ms: timeout.valueAsNumber,
      frame_hold_ms: hold.valueAsNumber,
    });
  }
  for (const control of [timeout, hold]) {
    control.addEventListener("input", queueTiming);
    control.addEventListener("change", () => {
      queueTiming();
      void timingSave.flush();
    });
  }
  timingForm.addEventListener("submit", (event) => {
    event.preventDefault();
    queueTiming();
    void timingSave.flush();
  });
  // Window size: throttled live preview and persistent final commit.
  const width = field<HTMLInputElement>("width");
  let pendingWidth: number | undefined;
  let commitWidth: number | undefined;
  let resizing = false;
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;
  let lastPreviewAt = 0;

  async function flushResize() {
    resizeTimer = undefined;
    if (resizing) return;
    const committing = commitWidth !== undefined;
    const value = committing ? commitWidth : pendingWidth;
    if (value === undefined) return;
    pendingWidth = undefined;
    commitWidth = undefined;
    resizing = true;
    lastPreviewAt = performance.now();
    try {
      if (committing) {
        await invoke("update_settings", { patch: { width: value } });
      } else {
        await invoke("preview_size", { width: value });
      }
    } catch (e) {
      error(e);
      // Restore the persisted size if a transient preview fails.
      if (
        !committing &&
        pendingWidth === undefined &&
        commitWidth === undefined &&
        getView()
      ) {
        commitWidth = getView()!.settings.window_size.width;
      }
    } finally {
      resizing = false;
      if (pendingWidth !== undefined || commitWidth !== undefined)
        scheduleResize();
      else if (committing) renderSettings();
    }
  }

  function scheduleResize() {
    if (resizing || resizeTimer !== undefined) return;
    const delay =
      commitWidth !== undefined
        ? 0
        : Math.max(0, 1000 / 30 - (performance.now() - lastPreviewAt));
    resizeTimer = setTimeout(() => void flushResize(), delay);
  }

  width.addEventListener("input", () => {
    updateRangeProgress(width);
    pendingWidth = Number(width.value);
    const view = getView();
    if (view) {
      const height = Math.min(
        800,
        (pendingWidth * view.skin.height) / view.skin.width,
      );
      const actualWidth = (height * view.skin.width) / view.skin.height;
      field("size-value").textContent =
        `${Math.round(actualWidth)} × ${Math.round(height)} px`;
    }
    scheduleResize();
  });
  width.addEventListener("change", () => {
    commitWidth = Number(width.value);
    pendingWidth = undefined;
    if (resizeTimer !== undefined) {
      clearTimeout(resizeTimer);
      resizeTimer = undefined;
    }
    scheduleResize();
  });
  // Development-only local signal; never inspect the value of KeyboardEvent.key.
  if (import.meta.env.DEV) {
    field("local-test").hidden = false;
    field("test-input").addEventListener("keydown", (event) => {
      event.preventDefault();
      if (!getView()?.input.active) void invoke("local_test_input").catch(error);
    });
  }

  return { render: renderSettings };
}
