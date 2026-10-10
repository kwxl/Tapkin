export type Skin = {
  name: string;
  directory: string;
  width: number;
  height: number;
  images: string[];
};
export type Settings = {
  selected_skin: string;
  window_position: { x: number; y: number } | null;
  window_size: { width: number; height: number };
  always_on_top: boolean;
  click_through: boolean;
  lock_position: boolean;
  launch_at_login: boolean;
  typing_timeout_ms: number;
  frame_hold_ms: number;
  repeat_held_keys: boolean;
};
export type InputStatus = {
  active: boolean;
  retrying: boolean;
  message: string | null;
};
export type View = {
  skin: Skin;
  settings: Settings;
  frame: number;
  revision: number;
  sequence: number;
  input: InputStatus;
  warning: string | null;
  platform: string;
};
export type FrameEvent = {
  frame: number;
  revision: number;
  sequence: number;
  image?: string;
  name?: string;
};
export type SkinEvent = { skin: Skin; revision: number; sequence: number };
export type SettingsEvent = { settings: Settings; sequence: number };
export type InputStatusEvent = { input: InputStatus; sequence: number };
export type WarningEvent = { message: string; sequence: number };
export type Patch = Partial<
  Pick<
    Settings,
    "always_on_top" | "click_through" | "lock_position" | "launch_at_login" | "repeat_held_keys"
  >
> & { width?: number; typing_timeout_ms?: number; frame_hold_ms?: number };

