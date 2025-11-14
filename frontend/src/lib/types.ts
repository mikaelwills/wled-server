export interface BoardState {
  id: string;
  ip: string;
  on: boolean;
  brightness: number;
  color: [number, number, number];
  effect: number;
  speed: number;
  intensity: number;
  transition: number;
  connected: boolean;
  ledCount?: number;
  maxLeds?: number;
  isGroup?: boolean;
  memberIds?: string[];
  universe?: number; // E1.31 universe number (for groups)
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
