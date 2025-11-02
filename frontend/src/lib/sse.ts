import { API_URL } from './api';
import type { BoardState } from './types';

export type SseEvent =
  | { type: 'state_update'; board_id: string; state: BoardState }
  | { type: 'connection_status'; board_id: string; connected: boolean }
  | { type: 'connected'; message: string };

export function createSseConnection(
  onStateUpdate: (boardId: string, state: BoardState) => void,
  onConnectionStatus: (boardId: string, connected: boolean) => void
): EventSource {
  const eventSource = new EventSource(`${API_URL}/events`);

  eventSource.onmessage = (event) => {
    console.log('SSE received:', event.data);
    try {
      const data: SseEvent = JSON.parse(event.data);
      console.log('SSE parsed:', data);

      if (data.type === 'state_update') {
        console.log('State update for:', data.board_id);
        onStateUpdate(data.board_id, data.state);
      } else if (data.type === 'connection_status') {
        console.log('Connection status for:', data.board_id, 'connected:', data.connected);
        onConnectionStatus(data.board_id, data.connected);
      } else if (data.type === 'connected') {
        console.log('SSE connected:', data.message);
      }
    } catch (err) {
      console.error('Failed to parse SSE event:', err);
    }
  };

  eventSource.onerror = () => {
    console.error('SSE connection error');
  };

  return eventSource;
}
