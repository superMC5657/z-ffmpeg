import { create } from "zustand";
import type { Preset } from "@/types";
import {
  loadPresets,
  deletePreset,
  importPreset,
  getBuiltinPresets,
} from "@/lib/tauri";

interface PresetState {
  presets: Preset[];
  selectedPresetId: string | null;
  isLoading: boolean;

  fetchPresets: () => Promise<void>;
  removePreset: (id: string) => Promise<void>;
  importPreset: (json: string, name: string) => Promise<Preset>;
  selectPreset: (id: string | null) => void;
  applyPreset: (preset: Preset) => Preset["config"];
}

export const usePresetStore = create<PresetState>((set, get) => ({
  presets: [],
  selectedPresetId: null,
  isLoading: false,

  fetchPresets: async () => {
    set({ isLoading: true });
    try {
      const [custom, builtins] = await Promise.all([
        loadPresets(),
        getBuiltinPresets(),
      ]);
      set({ presets: [...builtins, ...custom], isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  removePreset: async (id) => {
    await deletePreset(id);
    set((s) => ({
      presets: s.presets.filter((p) => p.id !== id),
      selectedPresetId:
        s.selectedPresetId === id ? null : s.selectedPresetId,
    }));
  },

  importPreset: async (json, name) => {
    const preset = await importPreset(json, name);
    await get().fetchPresets();
    return preset;
  },

  selectPreset: (id) => set({ selectedPresetId: id }),

  applyPreset: (preset) => {
    set({ selectedPresetId: preset.id });
    return preset.config;
  },
}));
