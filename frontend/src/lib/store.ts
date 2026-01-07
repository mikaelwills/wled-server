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

// Home-use presets store (WLED board presets)
export interface Preset {
	id: number;
	name: string;
}
export const presets: Writable<Preset[]> = writable([]);

// Performance presets store (server-side effects engine, E1.31)
export interface PerformancePreset {
	name: string;
	effect_type: string;
	color: [number, number, number];
}
export const performancePresets: Writable<PerformancePreset[]> = writable([]);

// Pattern presets store (group patterns like wave, random, etc.)
export interface PatternPreset {
	name: string;
	pattern: string;
	color: [number, number, number];
}
export const patternPresets: Writable<PatternPreset[]> = writable([]);

// Playback store - manages currently playing program
export const currentlyPlayingProgram: Writable<Program | null> = writable(null);

// Track which program ID should respond to spacebar (last played/paused)
export const lastActiveProgramId: Writable<string | null> = writable(null);

// Loopy Pro settings store
export interface LoopyProSettings {
	ip: string;
	port: number;
	mute_audio: boolean;
	audio_sync_delay_ms: number;
}

export const loopyProSettings: Writable<LoopyProSettings> = writable({
	ip: '192.168.1.100',
	port: 7000,
	mute_audio: false,
	audio_sync_delay_ms: 0
});
export const loopyProSettingsLoading: Writable<boolean> = writable(false);
export const loopyProSettingsError: Writable<string | null> = writable(null);

// Audio store - maps program.id to loaded Audio elements
export const audioElements: Writable<Record<string, HTMLAudioElement>> = writable({});
export const audioLoading: Writable<boolean> = writable(false);
export const audioError: Writable<string | null> = writable(null);

// Grid multiplier for beat grid display (4 = "1" button default)
export const gridMultiplier: Writable<number> = writable(4);
