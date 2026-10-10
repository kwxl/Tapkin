import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { field } from "./dom";
import type { Skin, View, FrameEvent, SkinEvent, SettingsEvent, InputStatusEvent, WarningEvent } from "./types";
import { initializeSettings } from "../settings";

const isSettings =
  new URLSearchParams(location.search).get("view") === "settings";
document.body.classList.toggle("settings-view", isSettings);
const image = document.querySelector<HTMLImageElement>("#pet")!;
let view: View | undefined;
let cache: HTMLImageElement[] = [];
let cacheRevision = -1;
const pending: (() => void)[] = [];
const unlisten: UnlistenFn[] = [];
function error(message: unknown) {
  const text = String(message);
  if (isSettings) {
    field("error").textContent = text;
    field("error").hidden = false;
  } else {
    image.title = `Tapkin: ${text}. Open Settings from the tray.`;
    void invoke("show_settings_window").catch(() => undefined);
  }
}
function renderFrame() {
  if (!view) return;
  image.src =
    (cacheRevision === view.revision ? cache[view.frame]?.src : undefined) ??
    view.skin.images[view.frame] ??
    view.skin.images[0];
  image.alt = `${view.skin.name} ${view.frame === 0 ? "idle" : "typing"}`;
  if (isSettings) field<HTMLImageElement>("preview").src = image.src;
}
async function preload(skin: Skin) {
  const images = skin.images.map((src) => {
    const img = new Image();
    img.src = src;
    return img;
  });
  await Promise.all(images.map((img) => img.decode()));
  return images;
}
const settings = isSettings ? initializeSettings({ getView: () => view, error }) : undefined;
function renderSettings() { settings?.render(); }

function afterBootstrap(fn: () => void) {
  if (view) fn();
  else pending.push(fn);
}
function acceptSequence(sequence: number) {
  if (!view || sequence <= view.sequence) return false;
  view.sequence = sequence;
  return true;
}

async function bootstrap() {
  // Listen before requesting the snapshot, then ignore stale frame sequence numbers.
  unlisten.push(
    await listen<FrameEvent>("pet-frame", ({ payload }) =>
      afterBootstrap(() => {
        if (
          !view ||
          payload.revision < view.revision ||
          (isSettings && payload.revision !== view.revision)
        )
          return;
        if (!isSettings && payload.revision > view.revision && !payload.image)
          return;
        if (!acceptSequence(payload.sequence)) return;
        if (!isSettings) {
          // PNG transport/caching belongs to the Tauri overlay. Settings still uses SkinView.
          if (payload.revision > view.revision) {
            view.revision = payload.revision;
            view.skin.images = [];
            cache = [];
            cacheRevision = payload.revision;
          }
          if (payload.name) view.skin.name = payload.name;
          if (payload.image) {
            view.skin.images[payload.frame] = payload.image;
            const decoded = new Image();
            decoded.src = payload.image;
            cache[payload.frame] = decoded;
          }
        }
        view.frame = payload.frame;
        renderFrame();
      }),
    ),
  );
  unlisten.push(
    await listen<SkinEvent>("skin-changed", ({ payload }) =>
      afterBootstrap(() => {
        if (
          !view ||
          payload.revision <= view.revision ||
          !acceptSequence(payload.sequence)
        )
          return;
        view.skin = payload.skin;
        view.revision = payload.revision;
        view.frame = 0;
        view.warning = null;
        renderFrame();
        renderSettings();
        void preload(payload.skin)
          .then((images) => {
            if (view?.revision === payload.revision) {
              cache = images;
              cacheRevision = payload.revision;
              renderFrame();
            }
          })
          .catch(error);
      }),
    ),
  );
  unlisten.push(
    await listen<SettingsEvent>("settings-changed", ({ payload }) =>
      afterBootstrap(() => {
        if (view && acceptSequence(payload.sequence)) {
          view.settings = payload.settings;
          renderSettings();
        }
      }),
    ),
  );
  unlisten.push(
    await listen<InputStatusEvent>("input-status", ({ payload }) =>
      afterBootstrap(() => {
        if (view && acceptSequence(payload.sequence)) {
          view.input = payload.input;
          renderSettings();
        }
      }),
    ),
  );
  unlisten.push(
    await listen<WarningEvent>("app-warning", ({ payload }) =>
      afterBootstrap(() => {
        if (view && acceptSequence(payload.sequence)) {
          view.warning = payload.message;
          renderSettings();
        }
      }),
    ),
  );
  const snapshot = await invoke<View>("get_view");
  cache = await preload(snapshot.skin);
  cacheRevision = snapshot.revision;
  view = snapshot;
  renderFrame();
  renderSettings();
  pending.splice(0).forEach((fn) => fn());
}

// Pet-only drag and context-menu controls.
if (!isSettings) {
  image.addEventListener("pointerdown", (e) => {
    if (e.button === 0) void invoke("drag_pet").catch(error);
  });
  image.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    void invoke("show_settings_window").catch(error);
  });
}
// Keep cached decoded images alive; swaps never trigger a network request or a render loop.
window.addEventListener("beforeunload", () => {
  unlisten.forEach((fn) => fn());
  cache = [];
});

void bootstrap().catch(error);
