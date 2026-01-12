import { writable, type Writable } from 'svelte/store';
import { browser } from '$app/environment';
import { API_URL } from './api';

export interface DriftEvent {
	timestamp: number;
	source: string;
	drift_ms: number;
	label: string;
}

export interface TimingSnapshot {
	cue_count: number;
	cues_drifted: number;
	cue_drift_total_ms: number;
	cue_drift_max_ms: number;
	frame_count: number;
	frame_avg_ms: number;
	frame_max_ms: number;
	packets_ok: number;
	packets_wouldblock: number;
	packets_err: number;
	recent_events: DriftEvent[];
	drift_threshold_ms: number;
}

export const timingSnapshot: Writable<TimingSnapshot | null> = writable(null);
export const timingLoading: Writable<boolean> = writable(false);
export const timingError: Writable<string | null> = writable(null);
export const timingMonitorVisible: Writable<boolean> = writable(false);

let pollInterval: ReturnType<typeof setInterval> | null = null;

export async function fetchTimingSnapshot(): Promise<void> {
	if (!browser) return;

	try {
		const response = await fetch(`${API_URL}/timing/snapshot`);
		if (response.ok) {
			const data = await response.json();
			timingSnapshot.set(data);
		} else {
			timingError.set('Failed to fetch timing snapshot');
		}
	} catch (error) {
		timingError.set(String(error));
	}
}

export async function fetchTimingEvents(): Promise<DriftEvent[]> {
	if (!browser) return [];

	try {
		const response = await fetch(`${API_URL}/timing/events`);
		if (response.ok) {
			const data = await response.json();
			return data.events || [];
		}
	} catch (error) {
		console.error('Failed to fetch timing events:', error);
	}
	return [];
}

export async function clearTimingEvents(): Promise<void> {
	if (!browser) return;

	try {
		await fetch(`${API_URL}/timing/events`, { method: 'DELETE' });
		await fetchTimingSnapshot();
	} catch (error) {
		console.error('Failed to clear timing events:', error);
	}
}

export async function resetTimingMetrics(): Promise<void> {
	if (!browser) return;

	try {
		await fetch(`${API_URL}/timing/reset`, { method: 'POST' });
		await fetchTimingSnapshot();
	} catch (error) {
		console.error('Failed to reset timing metrics:', error);
	}
}

export async function updateDriftThreshold(threshold_ms: number): Promise<void> {
	if (!browser) return;

	try {
		await fetch(`${API_URL}/timing/threshold`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ drift_threshold_ms: threshold_ms })
		});
		await fetchTimingSnapshot();
	} catch (error) {
		console.error('Failed to update drift threshold:', error);
	}
}

export function startTimingPolling(intervalMs: number = 1000): void {
	if (pollInterval) return;

	fetchTimingSnapshot();
	pollInterval = setInterval(fetchTimingSnapshot, intervalMs);
}

export function stopTimingPolling(): void {
	if (pollInterval) {
		clearInterval(pollInterval);
		pollInterval = null;
	}
}

export function toggleTimingMonitor(): void {
	timingMonitorVisible.update((v) => {
		const newValue = !v;
		if (newValue) {
			startTimingPolling();
		} else {
			stopTimingPolling();
		}
		return newValue;
	});
}
