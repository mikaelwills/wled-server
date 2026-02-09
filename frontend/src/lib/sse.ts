import { API_URL } from './api';
import type { BoardState } from './types';
import { resamplingProgress, resamplingComplete, type SlotResamplingProgress } from '$lib/stores/store';

export type { SlotResamplingProgress as ResamplingProgress };

export type SseEvent =
  | { type: 'state_update'; board_id: string; state: BoardState }
  | { type: 'connection_status'; board_id: string; connected: boolean }
  | { type: 'resampling_progress'; slot: string; program_id: string; track_name: string; current: number; total: number; active: boolean; from_rate: number; to_rate: number }
  | { type: 'resampling_complete'; slot: string; program_id: string; target_rate: number; quality: string }
  | { type: 'connected'; message: string };

export function createSseConnection(
  onStateUpdate: (boardId: string, state: BoardState) => void,
  onConnectionStatus: (boardId: string, connected: boolean) => void
): EventSource {
  const eventSource = new EventSource(`${API_URL}/events`);
  let isPageUnloading = false;

  if (typeof window !== 'undefined') {
    window.addEventListener('beforeunload', () => {
      isPageUnloading = true;
    });
  }

  eventSource.onopen = () => {
    console.log('SSE connected');
  };

  eventSource.onmessage = (event) => {
    try {
      const data: SseEvent = JSON.parse(event.data);

      if (data.type === 'state_update') {
        onStateUpdate(data.board_id, data.state);
      } else if (data.type === 'connection_status') {
        onConnectionStatus(data.board_id, data.connected);
      } else if (data.type === 'resampling_progress') {
        const progress: ResamplingProgress = {
          slot: data.slot,
          programId: data.program_id,
          trackName: data.track_name,
          current: data.current,
          total: data.total,
          active: data.active,
          fromRate: data.from_rate,
          toRate: data.to_rate,
        };
        if (progress.active) {
          resamplingProgress.update(state => ({ ...state, [progress.slot]: progress }));
        } else {
          resamplingProgress.update(state => ({ ...state, [progress.slot]: null }));
        }
      } else if (data.type === 'resampling_complete') {
        resamplingComplete.set({
          slot: data.slot,
          programId: data.program_id,
          targetRate: data.target_rate,
          quality: data.quality,
          timestamp: Date.now(),
        });
      }
    } catch (err) {
      console.error('Failed to parse SSE event:', err);
    }
  };

  eventSource.onerror = (event) => {
    if (isPageUnloading) {
      return;
    }

    if (eventSource.readyState === EventSource.CONNECTING) {
      console.log('SSE reconnecting...');
    } else if (eventSource.readyState === EventSource.CLOSED) {
      console.warn('SSE connection closed');
    }
  };

  return eventSource;
}
