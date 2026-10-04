import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./style.css";

type Skin = { name: string; directory: string; width: number; height: number; images: string[] };
type Settings = {
  selected_skin: string; window_position: { x: number; y: number } | null;
  window_size: { width: number; height: number }; always_on_top: boolean;
  click_through: boolean; lock_position: boolean; launch_at_login: boolean;
};
type InputStatus = { active: boolean; retrying: boolean; message: string | null };
type View = { skin: Skin; settings: Settings; frame: number; revision: number; sequence: number;
  input: InputStatus; warning: string | null; platform: string };
type FrameEvent = { frame: number; revision: number; sequence: number; image?: string; name?: string };
type SkinEvent = { skin: Skin; revision: number; sequence: number };
type SettingsEvent = { settings: Settings; sequence: number };
type InputStatusEvent = { input: InputStatus; sequence: number };
type WarningEvent = { message: string; sequence: number };
type Patch = Partial<Pick<Settings, "always_on_top" | "click_through" | "lock_position" | "launch_at_login">> & { width?: number };

const isSettings = new URLSearchParams(location.search).get("view") === "settings";
document.body.classList.toggle("settings-view", isSettings);
const image = document.querySelector<HTMLImageElement>("#pet")!;
let view: View | undefined;
let cache: HTMLImageElement[] = [];
let cacheRevision = -1;
const pending: (() => void)[] = [];
const unlisten: UnlistenFn[] = [];
const field = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

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
async function action(task: () => Promise<unknown>) {
  field("error").hidden = true;
  try { await task(); } catch (e) { error(e); }
}
function renderFrame() {
  if (!view) return;
  image.src = (cacheRevision === view.revision ? cache[view.frame]?.src : undefined)
    ?? view.skin.images[view.frame] ?? view.skin.images[0];
  image.alt = `${view.skin.name} ${view.frame === 0 ? "idle" : "typing"}`;
  if (isSettings) field<HTMLImageElement>("preview").src = image.src;
}
async function preload(skin: Skin) {
  const images = skin.images.map((src) => { const img = new Image(); img.src = src; return img; });
  await Promise.all(images.map((img) => img.decode()));
  return images;
}
function renderSettings() {
  if (!view || !isSettings) return;
  field("skin-name").textContent = view.skin.name;
  field("skin-path").textContent = view.skin.directory;
  const width = field<HTMLInputElement>("width");
  if (document.activeElement !== width) width.value = String(Math.round(view.settings.window_size.width));
  field("size-value").textContent = `${Math.round(view.settings.window_size.width)} × ${Math.round(view.settings.window_size.height)} px`;
  for (const key of ["always_on_top", "click_through", "lock_position", "launch_at_login"] as const) {
    field<HTMLInputElement>(key).checked = view.settings[key];
  }
  field("input-status").textContent = view.input.retrying ? "Starting listener…" : view.input.active ? "Listening for typing" : "Keyboard listener inactive";
  field("input-status").classList.toggle("active", view.input.active);
  field("permission-message").textContent = view.input.message ?? "Tapkin detects keyboard activity without recording or saving what you type.";
  field<HTMLButtonElement>("retry").disabled = view.input.retrying;
  field("mac-permissions").hidden = view.platform !== "macos";
  field("warning").textContent = view.warning ?? "";
  field("warning").hidden = !view.warning;
}
function afterBootstrap(fn: () => void) { if (view) fn(); else pending.push(fn); }
function acceptSequence(sequence: number) {
  if (!view || sequence <= view.sequence) return false;
  view.sequence = sequence;
  return true;
}

async function bootstrap() {
  // Listen before requesting the snapshot, then ignore stale frame sequence numbers.
  unlisten.push(await listen<FrameEvent>("pet-frame", ({ payload }) => afterBootstrap(() => {
    if (!view || payload.revision < view.revision || (isSettings && payload.revision !== view.revision)) return;
    if (!isSettings && payload.revision > view.revision && !payload.image) return;
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
  })));
  unlisten.push(await listen<SkinEvent>("skin-changed", ({ payload }) => afterBootstrap(() => {
    if (!view || payload.revision <= view.revision || !acceptSequence(payload.sequence)) return;
    view.skin = payload.skin;
    view.revision = payload.revision;
    view.frame = 0;
    view.warning = null;
    renderFrame();
    renderSettings();
    void preload(payload.skin).then((images) => {
      if (view?.revision === payload.revision) { cache = images; cacheRevision = payload.revision; renderFrame(); }
    }).catch(error);
  })));
  unlisten.push(await listen<SettingsEvent>("settings-changed", ({ payload }) => afterBootstrap(() => {
    if (view && acceptSequence(payload.sequence)) { view.settings = payload.settings; renderSettings(); }
  })));
  unlisten.push(await listen<InputStatusEvent>("input-status", ({ payload }) => afterBootstrap(() => {
    if (view && acceptSequence(payload.sequence)) { view.input = payload.input; renderSettings(); }
  })));
  unlisten.push(await listen<WarningEvent>("app-warning", ({ payload }) => afterBootstrap(() => {
    if (view && acceptSequence(payload.sequence)) { view.warning = payload.message; renderSettings(); }
  })));
  const snapshot = await invoke<View>("get_view");
  cache = await preload(snapshot.skin);
  cacheRevision = snapshot.revision;
  view = snapshot;
  renderFrame();
  renderSettings();
  pending.splice(0).forEach((fn) => fn());
}

image.addEventListener("pointerdown", (e) => {
  if (e.button === 0) void invoke("drag_pet").catch(error);
});
image.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  void invoke("show_settings_window").catch(error);
});
// Keep cached decoded images alive; swaps never trigger a network request or a render loop.
window.addEventListener("beforeunload", () => { unlisten.forEach((fn) => fn()); cache = []; });

if (isSettings) {
  field("choose-skin").addEventListener("click", () => void action(async () => {
    const directory = await open({ directory: true, multiple: false, title: "Choose a Tapkin skin folder" });
    if (directory) await invoke("load_skin", { directory });
  }));
  for (const [id, command] of [["reload", "reload_skin"], ["open-folder", "open_skin_folder"],
    ["retry", "retry_listener"], ["quit", "quit_app"]]) {
    field(id).addEventListener("click", () => void action(() => invoke(command)));
  }
  field("input-permission").addEventListener("click", () => void action(() => invoke("open_privacy_settings", { section: "input" })));
  field("accessibility-permission").addEventListener("click", () => void action(() => invoke("open_privacy_settings", { section: "accessibility" })));
  for (const key of ["always_on_top", "click_through", "lock_position", "launch_at_login"] as const) {
    const control = field<HTMLInputElement>(key);
    control.addEventListener("change", () => void action(async () => {
      const patch: Patch = { [key]: control.checked };
      control.disabled = true;
      try { await invoke("update_settings", { patch }); }
      finally { control.disabled = false; renderSettings(); }
    }));
  }
  field<HTMLInputElement>("width").addEventListener("change", () => void action(async () => {
    const patch: Patch = { width: Number(field<HTMLInputElement>("width").value) };
    try { await invoke("update_settings", { patch }); } finally { renderSettings(); }
  }));
  // Development-only local signal; never inspect the value of KeyboardEvent.key.
  if (import.meta.env.DEV) {
    field("local-test").hidden = false;
    field("test-input").addEventListener("keydown", (event) => {
      event.preventDefault();
      if (!view?.input.active) void invoke("local_test_input").catch(error);
    });
  }
}
void bootstrap().catch(error);
