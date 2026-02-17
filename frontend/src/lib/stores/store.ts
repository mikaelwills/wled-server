import { writable, type Writable } from 'svelte/store';
import type { Program } from '$lib/models/Program';
import type { BoardState } from '$lib/types';
import type { Setlist } from '$lib/models/Setlist';

// Programs store - state triplet pattern
export const programs: Writable<Program[]> = writable([]);
export const programsLoading: Writable<boolean> = writable(true);
export const programsError: Writable<string | null> = writable(null);

// Setlists store
export const setlists: Writable<Setlist[]> = writable([]);
export const activeSetlistId: Writable<string> = writable('default');

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
	id?: number;
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

// SSE-driven playback position (backend broadcasts at 10Hz)
export interface PlaybackPositionEvent {
	programId: string;
	positionSecs: number;
	durationSecs: number;
}
export const playbackPosition: Writable<PlaybackPositionEvent | null> = writable(null);

// Loopy Pro settings store
export type AudioSource = 'audio_engine' | 'loopy_pro';

export interface LoopyProSettings {
	ip: string;
	port: number;
	audio_source: AudioSource;
	audio_sync_delay_ms: number;
}

export const loopyProSettings: Writable<LoopyProSettings> = writable({
	ip: '192.168.1.100',
	port: 7000,
	audio_source: 'audio_engine',
	audio_sync_delay_ms: 0
});
export const loopyProSettingsLoading: Writable<boolean> = writable(false);
export const loopyProSettingsError: Writable<string | null> = writable(null);

// Audio store - maps program.id to loaded Audio elements
export const audioElements: Writable<Record<string, HTMLAudioElement>> = writable({});
export const audioLoading: Writable<boolean> = writable(true); // Start true - initAudio sets false when done
export const audioError: Writable<string | null> = writable(null);

// Audio blob URLs cache - maps program.id to blob URL for WaveSurfer
export const audioBlobUrls: Writable<Record<string, string>> = writable({});

// Cached waveform peaks - maps program.id to pre-computed peaks data
// This allows WaveSurfer to render instantly without decoding audio
export const cachedPeaks: Writable<Record<string, {
	peaks: Array<number[]>;
	duration: number;
}>> = writable({});

// Guide track stores - maps program.id to guide audio data
export const guideBlobUrls: Writable<Record<string, string>> = writable({});
export const guideCachedPeaks: Writable<Record<string, {
	peaks: Array<number[]>;
	duration: number;
}>> = writable({});

// Grid multiplier for beat grid display (4 = "1" button default)
export const gridMultiplier: Writable<number> = writable(4);

// Resampling quality store
export type ResamplingQuality = 'fast' | 'balanced' | 'high';
export const resamplingQuality: Writable<ResamplingQuality> = writable('balanced');
export const resamplingQualityLoading: Writable<boolean> = writable(true);

// Slot mute state - tracks which audio slots are muted
export const slotMuted: Writable<Record<string, boolean>> = writable({
	backing: false,
	guide: false,
	click: false,
	aux: false,
});

// Slot volume state - per-slot volume levels (0.0 to 2.0, default 1.0)
export const slotVolume: Writable<Record<string, number>> = writable({
	backing: 1.0,
	guide: 1.0,
	click: 1.0,
	aux: 1.0,
});

// Resampling progress store - centralized SSE progress state
export interface SlotResamplingProgress {
	slot: string;
	programId: string;
	trackName: string;
	current: number;
	total: number;
	active: boolean;
	fromRate: number;
	toRate: number;
}

export const resamplingProgress: Writable<Record<string, SlotResamplingProgress | null>> = writable({
	backing: null,
	guide: null,
	click: null,
	aux: null,
});

export interface ResamplingCompleteEvent {
	slot: string;
	programId: string;
	targetRate: number;
	quality: string;
	timestamp: number;
}

export const resamplingComplete: Writable<ResamplingCompleteEvent | null> = writable(null);

export interface ResamplingBatchState {
	totalTracks: number;
	completedTracks: number;
	targetRate: number;
}

export const resamplingBatch: Writable<ResamplingBatchState | null> = writable(null);
