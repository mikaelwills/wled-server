import { API_URL } from './api';
import type { BoardState } from './types';

export interface ResamplingProgress {
  current: number;
  total: number;
  active: boolean;
  trackName: string;
  fromRate: number;
  toRate: number;
}

export type SseEvent =
  | { type: 'state_update'; board_id: string; state: BoardState }
  | { type: 'connection_status'; board_id: string; connected: boolean }
  | { type: 'resampling_progress'; current: number; total: number; active: boolean; track_name: string; from_rate: number; to_rate: number }
  | { type: 'connected'; message: string };

type ResamplingCallback = (progress: ResamplingProgress) => void;
const resamplingCallbacks: ResamplingCallback[] = [];

export function onResamplingProgress(callback: ResamplingCallback): () => void {
  resamplingCallbacks.push(callback);
  return () => {
    const idx = resamplingCallbacks.indexOf(callback);
    if (idx >= 0) resamplingCallbacks.splice(idx, 1);
  };
}

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
          current: data.current,
          total: data.total,
          active: data.active,
          trackName: data.track_name,
          fromRate: data.from_rate,
          toRate: data.to_rate,
        };
        for (const cb of resamplingCallbacks) {
          cb(progress);
        }
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
