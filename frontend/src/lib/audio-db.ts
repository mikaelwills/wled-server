// frontend/src/lib/audio-db.ts
// Centralized audio loading and management
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { audioElements, audioLoading, audioError, audioBlobUrls, cachedPeaks, programs } from './store';
import { API_URL } from '$lib/api';

// Track programs currently being loaded to prevent duplicate fetches
const loadingPrograms = new Set<string>();

// Shared AudioContext for decoding
let audioContext: AudioContext | null = null;

function getAudioContext(): AudioContext {
	if (!audioContext) {
		audioContext = new AudioContext();
	}
	return audioContext;
}

/**
 * Compute waveform peaks from an AudioBuffer
 * Returns normalized min/max pairs suitable for WaveSurfer
 */
function computePeaksFromBuffer(audioBuffer: AudioBuffer, targetLength: number = 8000): Array<number[]> {
	const channelData = audioBuffer.getChannelData(0);
	const samples = channelData.length;
	const samplesPerPeak = Math.floor(samples / targetLength);
	const peaks: number[] = [];

	for (let i = 0; i < targetLength; i++) {
		const start = i * samplesPerPeak;
		const end = Math.min(start + samplesPerPeak, samples);

		let min = 0;
		let max = 0;
		for (let j = start; j < end; j++) {
			const value = channelData[j];
			if (value < min) min = value;
			if (value > max) max = value;
		}
		// Store both min and max for proper waveform rendering
		peaks.push(min, max);
	}

	return [peaks];
}

/**
 * Initialize audio for all programs
 * Fetches pre-computed peaks from backend first for instant loading
 * Only computes peaks locally if not cached on backend
 */
export async function initAudio(): Promise<void> {
	if (!browser) return;

	audioLoading.set(true);
	audioError.set(null);

	try {
		const currentPrograms = get(programs);
		const totalStart = performance.now();
		console.log(`[initAudio] Starting - ${currentPrograms.length} programs (parallel)`);

		const loadedAudio: Record<string, HTMLAudioElement> = {};
		const loadedBlobUrls: Record<string, string> = {};
		const loadedPeaks: Record<string, { peaks: Array<number[]>; duration: number }> = {};

		const ctx = getAudioContext();

		await Promise.all(
			currentPrograms
				.filter(program => program.audioId)
				.map(async (program) => {
					try {
						const programStart = performance.now();

						const [peaksResponse, audioResponse] = await Promise.all([
							fetch(`${API_URL}/audio/${program.audioId}/peaks`),
							fetch(`${API_URL}/audio/${program.audioId}`)
						]);

						if (!audioResponse.ok) return;

						const blob = await audioResponse.blob();
						const blobUrl = URL.createObjectURL(blob);

						const audio = new Audio();
						audio.src = blobUrl;
						loadedAudio[program.id] = audio;
						loadedBlobUrls[program.id] = blobUrl;

						if (peaksResponse.ok) {
							const peaksData = await peaksResponse.json();
							loadedPeaks[program.id] = peaksData;
							console.log(`[initAudio] ✓ Cached peaks: ${program.songName} (${(performance.now() - programStart).toFixed(0)}ms)`);
						} else {
							const arrayBuffer = await blob.arrayBuffer();
							const audioBuffer = await ctx.decodeAudioData(arrayBuffer);
							const peaks = computePeaksFromBuffer(audioBuffer);
							loadedPeaks[program.id] = { peaks, duration: audioBuffer.duration };

							fetch(`${API_URL}/audio/${program.audioId}/peaks`, {
								method: 'POST',
								headers: { 'Content-Type': 'application/json' },
								body: JSON.stringify({ peaks, duration: audioBuffer.duration })
							}).catch(err => console.warn(`Failed to save peaks for ${program.songName}:`, err));

							console.log(`[initAudio] ✓ Computed & saved: ${program.songName} (${(performance.now() - programStart).toFixed(0)}ms)`);
						}
					} catch (err) {
						console.warn(`Error loading audio for ${program.songName}:`, err);
					}
				})
		);

		audioElements.set(loadedAudio);
		audioBlobUrls.set(loadedBlobUrls);
		cachedPeaks.set(loadedPeaks);
		audioLoading.set(false);
		console.log(`[initAudio] Complete - ${Object.keys(loadedPeaks).length} programs ready (${(performance.now() - totalStart).toFixed(0)}ms total)`);
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
 * Cleanup all audio elements and revoke blob URLs
 */
export function cleanupAudio(): void {
	const elements = get(audioElements);
	const blobUrls = get(audioBlobUrls);

	Object.values(elements).forEach(audio => {
		if (audio) {
			audio.pause();
			audio.src = '';
		}
	});

	Object.values(blobUrls).forEach(url => {
		if (url) URL.revokeObjectURL(url);
	});

	audioElements.set({});
	audioBlobUrls.set({});
}

/**
 * Get blob URL for a specific program (for WaveSurfer)
 */
export function getAudioBlobUrl(programId: string): string | undefined {
	const urls = get(audioBlobUrls);
	return urls[programId];
}

/**
 * Load audio for a single program (for newly created programs)
 * Fetches peaks from backend first, computes and saves if missing
 * Guards against duplicate concurrent fetches
 */
export async function loadAudioForProgram(programId: string, audioId: string): Promise<string | undefined> {
	if (!browser) return undefined;

	const existingUrl = get(audioBlobUrls)[programId];
	if (existingUrl) return existingUrl;

	if (loadingPrograms.has(programId)) return undefined;
	loadingPrograms.add(programId);

	try {
		const peaksResponse = await fetch(`${API_URL}/audio/${audioId}/peaks`);

		if (peaksResponse.ok) {
			const peaksData = await peaksResponse.json();
			cachedPeaks.update(cache => ({ ...cache, [programId]: peaksData }));

			const audioResponse = await fetch(`${API_URL}/audio/${audioId}`);
			if (!audioResponse.ok) return undefined;

			const blob = await audioResponse.blob();
			const blobUrl = URL.createObjectURL(blob);

			const audio = new Audio();
			audio.src = blobUrl;
			audioElements.update(elements => ({ ...elements, [programId]: audio }));
			audioBlobUrls.update(urls => ({ ...urls, [programId]: blobUrl }));

			return blobUrl;
		}

		const audioResponse = await fetch(`${API_URL}/audio/${audioId}`);
		if (!audioResponse.ok) return undefined;

		const blob = await audioResponse.blob();
		const blobUrl = URL.createObjectURL(blob);

		const audio = new Audio();
		audio.src = blobUrl;
		audioElements.update(elements => ({ ...elements, [programId]: audio }));
		audioBlobUrls.update(urls => ({ ...urls, [programId]: blobUrl }));

		const ctx = getAudioContext();
		const arrayBuffer = await blob.arrayBuffer();
		const audioBuffer = await ctx.decodeAudioData(arrayBuffer);
		const peaks = computePeaksFromBuffer(audioBuffer);
		cachedPeaks.update(cache => ({
			...cache,
			[programId]: { peaks, duration: audioBuffer.duration }
		}));

		fetch(`${API_URL}/audio/${audioId}/peaks`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ peaks, duration: audioBuffer.duration })
		}).catch(err => console.warn(`Failed to save peaks for program ${programId}:`, err));

		return blobUrl;
	} catch (err) {
		console.warn(`Error loading audio for program ${programId}:`, err);
	} finally {
		loadingPrograms.delete(programId);
	}
	return undefined;
}

/**
 * Remove audio for a deleted program
 */
export function removeAudioForProgram(programId: string): void {
	const elements = get(audioElements);
	const urls = get(audioBlobUrls);

	if (elements[programId]) {
		elements[programId].pause();
		elements[programId].src = '';
	}

	if (urls[programId]) {
		URL.revokeObjectURL(urls[programId]);
	}

	audioElements.update(el => {
		const { [programId]: _, ...rest } = el;
		return rest;
	});

	audioBlobUrls.update(u => {
		const { [programId]: _, ...rest } = u;
		return rest;
	});

	cachedPeaks.update(p => {
		const { [programId]: _, ...rest } = p;
		return rest;
	});
}

/**
 * Cache waveform peaks for instant rendering on subsequent loads
 */
export function cacheAudioPeaks(programId: string, peaks: Array<number[]>, duration: number): void {
	cachedPeaks.update(cache => ({
		...cache,
		[programId]: { peaks, duration }
	}));
}

/**
 * Get cached peaks for a program (for instant WaveSurfer rendering)
 */
export function getCachedPeaks(programId: string): { peaks: Array<number[]>; duration: number } | null {
	const cache = get(cachedPeaks);
	return cache[programId] || null;
}

/**
 * Reload audio (e.g., when mute_audio setting changes)
 */
export async function reloadAudio(): Promise<void> {
	cleanupAudio();
	await initAudio();
}
