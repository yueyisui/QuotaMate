import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, CodexUsage, PetImageHistoryEntry, RuntimeStatus } from "../types";

export const backend = {
  getUsage: () => invoke<CodexUsage>("get_usage"),
  getConfig: () => invoke<AppConfig>("get_config"),
  getRuntimeStatus: () => invoke<RuntimeStatus>("get_runtime_status"),
  getAppVersion: () => invoke<string>("app_version"),
  saveConfig: (next: AppConfig) => invoke<AppConfig>("save_config", { next }),
  refreshUsage: () => invoke<void>("refresh_usage"),
  showWindow: (window: string) => invoke<void>("show_window", { window }),
  hideMenuBarPanel: () => invoke<void>("hide_menu_bar_panel"),
  showFloatingContextMenu: (window: "compact" | "pet") =>
    invoke<void>("show_floating_context_menu", { window }),
  setCompactExpanded: (expanded: boolean) =>
    invoke<void>("set_compact_expanded", { expanded }),
  setPetExpanded: (expanded: boolean) =>
    invoke<void>("set_pet_expanded", { expanded }),
  disableDisplay: (window: "compact" | "pet") =>
    invoke<void>("disable_display", { window }),
  importPetImage: (source: string) =>
    invoke<AppConfig>("import_pet_image", { source }),
  getPetImageData: () => invoke<string | null>("get_pet_image_data"),
  getPetImageHistory: () => invoke<PetImageHistoryEntry[]>("get_pet_image_history"),
  selectPetImage: (id: string) => invoke<AppConfig>("select_pet_image", { id }),
  deletePetImage: (id: string) => invoke<AppConfig>("delete_pet_image", { id }),
  openLogFolder: () => invoke<void>("open_log_folder"),
};
