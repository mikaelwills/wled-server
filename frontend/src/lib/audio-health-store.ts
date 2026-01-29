import { writable, type Writable } from 'svelte/store';
import { browser } from '$app/environment';
import { API_URL } from './api';

export interface AudioHealthSnapshot {
	playing: boolean;
	sample_rate: number;
	position_samples: number;
	position_secs: number;
	callback_count: number;
	underrun_count: number;
	buffer_size: number;
	samples_delivered: number;
	silence_frames: number;
	max_callback_interval_ms: number;
	late_callbacks: number;
	expected_interval_ms: number;
}

export const audioHealthSnapshot: Writable<AudioHealthSnapshot | null> = writable(null);
export const audioHealthMonitorVisible: Writable<boolean> = writable(false);

let pollInterval: ReturnType<typeof setInterval> | null = null;
let lastUnderrunCount = 0;
let underrunDelta = 0;

export async function fetchAudioHealth(): Promise<void> {
	if (!browser) return;

	try {
		const response = await fetch(`${API_URL}/audio/engine/health`);
		if (response.ok) {
			const data: AudioHealthSnapshot = await response.json();

			if (lastUnderrunCount > 0) {
				underrunDelta = data.underrun_count - lastUnderrunCount;
			}
			lastUnderrunCount = data.underrun_count;

			audioHealthSnapshot.set(data);
		}
	} catch (error) {
		console.error('Failed to fetch audio health:', error);
	}
}

export function getUnderrunDelta(): number {
	return underrunDelta;
}

export function startAudioHealthPolling(intervalMs: number = 500): void {
	if (pollInterval) return;

	fetchAudioHealth();
	pollInterval = setInterval(fetchAudioHealth, intervalMs);
}

export function stopAudioHealthPolling(): void {
	if (pollInterval) {
		clearInterval(pollInterval);
		pollInterval = null;
	}
}

export async function resetAudioHealth(): Promise<void> {
	if (!browser) return;

	try {
		await fetch(`${API_URL}/audio/engine/health`, { method: 'DELETE' });
		await fetchAudioHealth();
	} catch (error) {
		console.error('Failed to reset audio health:', error);
	}
}

export function toggleAudioHealthMonitor(): void {
	audioHealthMonitorVisible.update((v) => {
		const newValue = !v;
		if (newValue) {
			startAudioHealthPolling(250);
		} else {
			stopAudioHealthPolling();
		}
		return newValue;
	});
}
