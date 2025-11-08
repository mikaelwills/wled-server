// frontend/src/lib/models/Cue.ts

export type CueAction = 'preset' | 'on' | 'off';

export interface CueData {
  time: number;
  label: string;
  boards: string[];
  action: CueAction;
  preset: number;
  color: string;
  effect: number;
  brightness: number;
  transition: number;
}

export class Cue implements CueData {
  time: number;
  label: string;
  boards: string[];
  action: CueAction;
  preset: number;
  color: string;
  effect: number;
  brightness: number;
  transition: number;

  private constructor(data: CueData) {
    this.time = data.time;
    this.label = data.label;
    this.boards = data.boards;
    this.action = data.action;
    this.preset = data.preset;
    this.color = data.color;
    this.effect = data.effect;
    this.brightness = data.brightness;
    this.transition = data.transition;
  }

  /**
   * Factory method - validates and constructs Cue from JSON
   */
  static fromJson(data: Record<string, any>): Cue | null {
    if (!data || typeof data.time !== 'number') {
      console.error('Invalid cue data: missing or invalid time', data);
      return null;
    }

    return new Cue({
      time: data.time,
      label: data.label || `Cue ${Math.floor(data.time)}s`,
      boards: Array.isArray(data.boards) ? data.boards : [],
      action: data.action || 'preset', // Default to 'preset' for backward compatibility
      preset: data.preset ?? 0,
      color: data.color || '#ff0000',
      effect: data.effect ?? 0,
      brightness: data.brightness ?? 255,
      transition: data.transition ?? 0,
    });
  }

  /**
   * Convert to plain object for storage
   */
  toJson(): Record<string, any> {
    return {
      time: this.time,
      label: this.label,
      boards: this.boards,
      action: this.action,
      preset: this.preset,
      color: this.color,
      effect: this.effect,
      brightness: this.brightness,
      transition: this.transition,
    };
  }
}
