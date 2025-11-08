export interface BoardState {
  id: string;
  ip: string;
  on: boolean;
  brightness: number;
  color: [number, number, number];
  effect: number;
  speed: number;
  intensity: number;
  connected: boolean;
  ledCount?: number;
  maxLeds?: number;
  isGroup?: boolean;
  memberIds?: string[];
}

export interface PresetState {
  on: boolean;
  brightness: number;
  color: [number, number, number];
  effect: number;
  speed: number;
  intensity: number;
}

export interface WledPreset {
  id: string;
  name: string;
  wled_slot: number;
  description?: string | null;
  state: PresetState;
  created_at: string;
}
