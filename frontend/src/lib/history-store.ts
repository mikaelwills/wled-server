import { writable, type Writable } from 'svelte/store';
import { browser } from '$app/environment';
import { API_URL } from './api';

export interface PlaybackSession {
	id: string;
	program_id: string;
	program_name: string;
	started_at: number;
	ended_at: number | null;
	duration_ms: number;
	cue_count: number;
	cues_drifted: number;
	cue_drift_avg_ms: number;
	cue_drift_max_ms: number;
	packets_ok: number;
	packets_wouldblock: number;
	packets_err: number;
	frame_count: number;
	frame_avg_ms: number;
	completed: boolean;
}

export interface HistoryResponse {
	sessions: PlaybackSession[];
	current: PlaybackSession | null;
}

export const historyStore: Writable<HistoryResponse> = writable({
	sessions: [],
	current: null
});
export const historyLoading: Writable<boolean> = writable(false);
export const historyError: Writable<string | null> = writable(null);

export async function fetchHistory(): Promise<void> {
	if (!browser) return;

	historyLoading.set(true);
	historyError.set(null);

	try {
		const response = await fetch(`${API_URL}/history`);
		if (response.ok) {
			const data: HistoryResponse = await response.json();
			historyStore.set(data);
		} else {
			historyError.set('Failed to fetch history');
		}
	} catch (error) {
		historyError.set(String(error));
	} finally {
		historyLoading.set(false);
	}
}

export async function deleteSession(id: string): Promise<boolean> {
	if (!browser) return false;

	try {
		const response = await fetch(`${API_URL}/history/${id}`, {
			method: 'DELETE'
		});
		if (response.ok) {
			await fetchHistory();
			return true;
		}
	} catch (error) {
		console.error('Failed to delete session:', error);
	}
	return false;
}

export async function clearHistory(): Promise<number> {
	if (!browser) return 0;

	try {
		const response = await fetch(`${API_URL}/history`, {
			method: 'DELETE'
		});
		if (response.ok) {
			const data = await response.json();
			await fetchHistory();
			return data.deleted_count || 0;
		}
	} catch (error) {
		console.error('Failed to clear history:', error);
	}
	return 0;
}

export function formatDuration(ms: number): string {
	const seconds = Math.floor(ms / 1000);
	const minutes = Math.floor(seconds / 60);
	const remainingSeconds = seconds % 60;
	return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

export function formatTimestamp(timestamp: number): string {
	return new Date(timestamp).toLocaleString();
}

export function getDriftStatus(avgMs: number): 'good' | 'warning' | 'bad' {
	if (avgMs < 5) return 'good';
	if (avgMs < 15) return 'warning';
	return 'bad';
}
