// frontend/src/lib/audio-db.ts
// Centralized audio loading and management
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { audioElements, audioLoading, audioError, programs } from './store';
import { API_URL } from '$lib/api';

/**
 * Initialize audio for all programs
 * Always loads audio - mute setting only affects playback in performance page
 * Sequencer always needs audio for waveform display and editing
 */
export async function initAudio(): Promise<void> {
	if (!browser) return;

	audioLoading.set(true);
	audioError.set(null);

	try {
		const currentPrograms = get(programs);
		const loadedAudio: Record<string, HTMLAudioElement> = {};

		console.log(`🎵 Loading audio for ${currentPrograms.length} programs...`);

		// Load all audio files
		for (const program of currentPrograms) {
			if (program.audioId) {
				try {
					const audio = new Audio();
					const response = await fetch(`${API_URL}/audio/${program.audioId}`);

					if (response.ok) {
						const blob = await response.blob();
						audio.src = URL.createObjectURL(blob);
						loadedAudio[program.id] = audio;
						console.log(`✅ Loaded audio for: ${program.songName}`);
					} else {
						console.warn(`Failed to fetch audio for ${program.songName}: ${response.statusText}`);
					}
				} catch (err) {
					console.warn(`Error loading audio for ${program.songName}:`, err);
					// Continue loading other files on error
				}
			}
		}

		// Update store with all loaded audio
		audioElements.set(loadedAudio);
		audioLoading.set(false);
		console.log(`✅ Audio loading complete: ${Object.keys(loadedAudio).length} files loaded`);
	} catch (error) {
		console.error('Failed to load audio:', error);
		audioError.set('Failed to load audio files.');
		audioLoading.set(false);
	}
}

/**
 * Get audio element for a specific program
 */
export function getAudioElement(programId: string): HTMLAudioElement | undefined {
	const elements = get(audioElements);
	return elements[programId];
}

/**
 * Cleanup all audio elements
 */
export function cleanupAudio(): void {
	const elements = get(audioElements);
	Object.values(elements).forEach(audio => {
		if (audio) {
			audio.pause();
			audio.src = '';
		}
	});
	audioElements.set({});
}

/**
 * Reload audio (e.g., when mute_audio setting changes)
 */
export async function reloadAudio(): Promise<void> {
	cleanupAudio();
	await initAudio();
}
