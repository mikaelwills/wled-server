// frontend/src/lib/audio-db.ts
// Centralized audio loading and management
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { audioElements, audioLoading, audioError, audioBlobUrls, cachedPeaks, guideBlobUrls, guideCachedPeaks, programs, resamplingQuality, resamplingQualityLoading, slotMuted, slotVolume } from '$lib/stores/store';
import type { ResamplingQuality } from '$lib/stores/store';
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
export function computePeaksFromBuffer(audioBuffer: AudioBuffer, targetLength: number = 8000): Array<number[]> {
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
		console.log(`[initAudio] Starting - ${currentPrograms.length} programs (peaks only)`);

		const loadedPeaks: Record<string, { peaks: Array<number[]>; duration: number }> = {};
		const loadedGuidePeaks: Record<string, { peaks: Array<number[]>; duration: number }> = {};

		await Promise.all(
			currentPrograms
				.filter(program => program.audioId)
				.map(async (program) => {
					try {
						const programStart = performance.now();

						const peaksResponse = await fetch(`${API_URL}/audio/${program.audioId}/peaks`);
						if (peaksResponse.ok) {
							loadedPeaks[program.id] = await peaksResponse.json();
							console.log(`[initAudio] ✓ Peaks: ${program.songName} (${(performance.now() - programStart).toFixed(0)}ms)`);
						}

						if (program.guideAudioId) {
							try {
								const guidePeaksRes = await fetch(`${API_URL}/audio/${program.guideAudioId}/peaks`);
								if (guidePeaksRes.ok) {
									loadedGuidePeaks[program.id] = await guidePeaksRes.json();
									console.log(`[initAudio] ✓ Guide peaks: ${program.songName}`);
								}
							} catch (err) {
								console.warn(`Error loading guide peaks for ${program.songName}:`, err);
							}
						}
					} catch (err) {
						console.warn(`Error loading peaks for ${program.songName}:`, err);
					}
				})
		);

		cachedPeaks.set(loadedPeaks);
		guideCachedPeaks.set(loadedGuidePeaks);
		audioLoading.set(false);
		const guideCount = Object.keys(loadedGuidePeaks).length;
		console.log(`[initAudio] Complete - ${Object.keys(loadedPeaks).length} programs ready, ${guideCount} with guide (${(performance.now() - totalStart).toFixed(0)}ms total)`);
	} catch (error) {
		console.error('Failed to load peaks:', error);
		audioError.set('Failed to load audio peaks.');
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
	console.log('[cleanupAudio] CALLED - revoking all blob URLs');
	const elements = get(audioElements);
	const blobUrls = get(audioBlobUrls);
	const guideUrls = get(guideBlobUrls);

	Object.values(elements).forEach(audio => {
		if (audio) {
			audio.pause();
			audio.src = '';
		}
	});

	Object.values(blobUrls).forEach(url => {
		if (url) URL.revokeObjectURL(url);
	});

	Object.values(guideUrls).forEach(url => {
		if (url) URL.revokeObjectURL(url);
	});

	audioElements.set({});
	audioBlobUrls.set({});
	guideBlobUrls.set({});
	guideCachedPeaks.set({});
}

/**
 * Get blob URL for a specific program (for WaveSurfer)
 */
export function getAudioBlobUrl(programId: string): string | undefined {
	const urls = get(audioBlobUrls);
	return urls[programId];
}

/**
 * Check if a blob URL is still valid
 */
async function isBlobUrlValid(url: string): Promise<boolean> {
	try {
		const response = await fetch(url, { method: 'HEAD' });
		console.log(`[isBlobUrlValid] ${url.substring(0, 50)}... -> ${response.ok}`);
		return response.ok;
	} catch (err) {
		console.log(`[isBlobUrlValid] ${url.substring(0, 50)}... -> error:`, err);
		return false;
	}
}

/**
 * Load audio for a single program (for newly created programs)
 * Fetches peaks from backend first, computes and saves if missing
 * Guards against duplicate concurrent fetches
 */
export async function loadAudioForProgram(programId: string, audioId: string): Promise<void> {
	if (!browser) return;

	const existingPeaks = get(cachedPeaks)[programId];
	if (existingPeaks) return;

	if (loadingPrograms.has(programId)) return;
	loadingPrograms.add(programId);

	try {
		const peaksResponse = await fetch(`${API_URL}/audio/${audioId}/peaks`);
		if (peaksResponse.ok) {
			const peaksData = await peaksResponse.json();
			cachedPeaks.update(cache => ({ ...cache, [programId]: peaksData }));
			console.log(`[loadAudio] ${programId} - peaks loaded`);
		}
	} catch (err) {
		console.warn(`Error loading peaks for program ${programId}:`, err);
	} finally {
		loadingPrograms.delete(programId);
	}
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

export function clearAudioCacheForProgram(programId: string): void {
	const urls = get(audioBlobUrls);

	if (urls[programId]) {
		URL.revokeObjectURL(urls[programId]);
	}

	audioBlobUrls.update(u => {
		const { [programId]: _, ...rest } = u;
		return rest;
	});

	cachedPeaks.update(p => {
		const { [programId]: _, ...rest } = p;
		return rest;
	});

	audioElements.update(el => {
		const elem = el[programId];
		if (elem) {
			elem.pause();
			elem.src = '';
		}
		const { [programId]: _, ...rest } = el;
		return rest;
	});
}

const loadingGuides = new Set<string>();

/**
 * Load guide audio for a program
 */
export async function loadGuideAudioForProgram(programId: string, guideAudioId: string): Promise<void> {
	if (!browser) return;

	const existingPeaks = get(guideCachedPeaks)[programId];
	if (existingPeaks) return;

	if (loadingGuides.has(programId)) return;
	loadingGuides.add(programId);

	try {
		const peaksResponse = await fetch(`${API_URL}/audio/${guideAudioId}/peaks`);
		if (peaksResponse.ok) {
			const peaksData = await peaksResponse.json();
			guideCachedPeaks.update(cache => ({ ...cache, [programId]: peaksData }));
			console.log(`[loadGuide] ${programId} - guide peaks loaded`);
		}
	} catch (err) {
		console.warn(`Error loading guide peaks for program ${programId}:`, err);
	} finally {
		loadingGuides.delete(programId);
	}
}

/**
 * Remove guide audio for a program
 */
export function removeGuideAudioForProgram(programId: string): void {
	const urls = get(guideBlobUrls);

	if (urls[programId]) {
		URL.revokeObjectURL(urls[programId]);
	}

	guideBlobUrls.update(u => {
		const { [programId]: _, ...rest } = u;
		return rest;
	});

	guideCachedPeaks.update(p => {
		const { [programId]: _, ...rest } = p;
		return rest;
	});
}

/**
 * Get guide blob URL for a program
 */
export function getGuideBlobUrl(programId: string): string | undefined {
	return get(guideBlobUrls)[programId];
}

/**
 * Get cached guide peaks for a program
 */
export function getGuideCachedPeaks(programId: string): { peaks: Array<number[]>; duration: number } | null {
	return get(guideCachedPeaks)[programId] || null;
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

/**
 * Initialize resampling quality from API
 */
export async function initResamplingQuality(): Promise<void> {
	if (!browser) return;

	resamplingQualityLoading.set(true);
	try {
		const res = await fetch(`${API_URL}/audio/resampling-quality`);
		if (res.ok) {
			const data = await res.json();
			resamplingQuality.set(data.quality);
		}
	} catch (e) {
		console.error('Failed to fetch resampling quality:', e);
	}
	resamplingQualityLoading.set(false);
}

/**
 * Update resampling quality
 */
export async function updateResamplingQuality(quality: ResamplingQuality): Promise<void> {
	if (!browser) return;

	resamplingQuality.set(quality);
	try {
		await fetch(`${API_URL}/audio/resampling-quality`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ quality })
		});
	} catch (e) {
		console.error('Failed to set resampling quality:', e);
	}
}

export type SlotId = 'backing' | 'guide' | 'click' | 'aux';

export async function setSlotVolume(track: SlotId, volume: number): Promise<void> {
	if (!browser) return;

	const clamped = Math.max(0, Math.min(2, volume));
	slotVolume.update(state => ({ ...state, [track]: clamped }));

	try {
		await fetch(`${API_URL}/audio/volume`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ track, volume: clamped })
		});
	} catch (e) {
		console.error(`Failed to set ${track} volume:`, e);
	}
}

export async function toggleSlotMute(track: SlotId): Promise<void> {
	if (!browser) return;

	const currentMuted = get(slotMuted);
	const newMuted = !currentMuted[track];

	slotMuted.update(state => ({ ...state, [track]: newMuted }));

	try {
		await fetch(`${API_URL}/audio/mute`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ track, muted: newMuted })
		});
	} catch (e) {
		console.error(`Failed to set ${track} mute:`, e);
		slotMuted.update(state => ({ ...state, [track]: !newMuted }));
	}
}
