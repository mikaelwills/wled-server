export interface BoardState {
  id: string;
  ip: string;
  on: boolean;
  brightness: number;
  color: [number, number, number];
  effect: number;
  connected: boolean;
  isGroup?: boolean;
  memberIds?: string[];
}
