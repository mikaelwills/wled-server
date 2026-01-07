// frontend/src/lib/models/Cue.ts

export interface CueData {
  time: number;
  label: string;
  boards: string[];
  presetName?: string;
  preset?: number;
  color: string;
  effect: number;
  brightness: number;
  syncRate?: number;
}

export class Cue implements CueData {
  time: number;
  label: string;
  boards: string[];
  presetName?: string;
  preset?: number;
  color: string;
  effect: number;
  brightness: number;
  syncRate?: number;

  private constructor(data: CueData) {
    this.time = data.time;
    this.label = data.label;
    this.boards = data.boards;
    this.presetName = data.presetName;
    this.preset = data.preset;
    this.color = data.color;
    this.effect = data.effect;
    this.brightness = data.brightness;
    this.syncRate = data.syncRate;
  }

  static fromJson(data: Record<string, any>): Cue | null {
    if (!data || typeof data.time !== 'number') {
      console.error('Invalid cue data: missing or invalid time', data);
      return null;
    }

    return new Cue({
      time: data.time,
      label: data.label || `Cue ${Math.floor(data.time)}s`,
      boards: Array.isArray(data.targets) ? data.targets : (Array.isArray(data.boards) ? data.boards : []),
      presetName: data.presetName || data.preset_name,
      preset: data.preset,
      color: data.color || '#ff0000',
      effect: data.effect ?? 0,
      brightness: data.brightness ?? 255,
      syncRate: data.syncRate ?? data.sync_rate ?? 1,
    });
  }

  toJson(): Record<string, any> {
    return {
      time: this.time,
      label: this.label,
      targets: this.boards,
      preset_name: this.presetName,
      color: this.color,
      effect: this.effect,
      brightness: this.brightness,
      sync_rate: this.syncRate,
    };
  }

  hasValidPreset(availablePresets: Array<{name: string}>): boolean {
    if (!this.presetName) return false;
    return availablePresets.some(p => p.name === this.presetName);
  }
}
