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
