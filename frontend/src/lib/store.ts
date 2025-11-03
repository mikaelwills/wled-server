// frontend/src/lib/store.ts
import { writable, type Writable } from 'svelte/store';
import type { Program } from './models/Program';
import type { BoardState } from './types';

// Programs store - state triplet pattern
export const programs: Writable<Program[]> = writable([]);
export const programsLoading: Writable<boolean> = writable(true);
export const programsError: Writable<string | null> = writable(null);

// Boards store - state triplet pattern
// Note: Groups are boards with isGroup: true, stored in same array
export const boards: Writable<BoardState[]> = writable([]);
export const boardsLoading: Writable<boolean> = writable(true);
export const boardsError: Writable<string | null> = writable(null);

// Presets store (static list, 0-16)
export interface Preset {
	id: number;
	name: string;
}
export const presets: Writable<Preset[]> = writable([]);

// Playback store - manages currently playing program
export const currentlyPlayingProgramId: Writable<string | null> = writable(null);
export const activeTimeouts: Writable<number[]> = writable([]);
