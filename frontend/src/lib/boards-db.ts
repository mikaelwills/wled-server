// frontend/src/lib/boards-db.ts
import { browser } from '$app/environment';
import { boards, boardsLoading, boardsError, presets } from './store';
import { API_URL } from './api';
import { createSseConnection } from './sse';
import type { BoardState } from './types';

let sseConnection: EventSource | null = null;

/**
 * Initialize SSE listener for real-time board updates
 */
export function initBoardsListener(): void {
  if (!browser) return;

  boardsLoading.set(true);
  boardsError.set(null);

  try {
    // Initial fetch to populate boards
    fetchBoards();

    // Set up SSE for real-time updates
    sseConnection = createSseConnection(
      (boardId: string, state: BoardState) => {
        // Update specific board state
        boards.update((currentBoards) =>
          currentBoards.map((b) => (b.id === boardId ? state : b))
        );
      },
      (boardId: string, connected: boolean) => {
        // Update connection status
        boards.update((currentBoards) =>
          currentBoards.map((b) => (b.id === boardId ? { ...b, connected } : b))
        );
      }
    );
  } catch (error) {
    console.error('Failed to initialize boards listener:', error);
    boardsError.set('Failed to initialize boards listener.');
    boardsLoading.set(false);
  }
}

/**
 * Fetch boards once from API
 */
async function fetchBoards(): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards`);

    if (!res.ok) {
      throw new Error(`Failed to fetch boards: ${res.statusText}`);
    }

    const data = await res.json();
    const loadedBoards = Array.isArray(data) ? data : [];

    // Derive group state from member boards
    const processedBoards = loadedBoards.map((board: BoardState) => {
      if (board.isGroup && board.memberIds) {
        const members = loadedBoards.filter(
          (b: BoardState) => !b.isGroup && board.memberIds?.includes(b.id)
        );

        if (members.length > 0) {
          const firstMember = members[0];
          return {
            ...board,
            color: firstMember.color,
            brightness: firstMember.brightness,
            effect: firstMember.effect,
            on: members.every((m: BoardState) => m.on),
          };
        }
      }
      return board;
    });

    // Sort: groups first, then regular boards
    processedBoards.sort((a: BoardState, b: BoardState) => {
      if (a.isGroup && !b.isGroup) return -1;
      if (!a.isGroup && b.isGroup) return 1;
      return 0;
    });

    boards.set(processedBoards);
    boardsLoading.set(false);
  } catch (error) {
    console.error('Failed to load boards:', error);
    boardsError.set('Failed to load boards from server.');
    boardsLoading.set(false);
    boards.set([]);
  }
}

/**
 * Initialize presets (static list from presets.json)
 */
export function initPresets(): void {
  const presetsList = [
    { id: 0, name: 'None (Manual Control)' },
    { id: 1, name: 'Lightning Cyan' },
    { id: 2, name: 'Lightning Cyan' },
    { id: 3, name: 'Lightning Red' },
    { id: 4, name: 'Lightning Green' },
    { id: 5, name: 'Puddles Green' },
    { id: 7, name: 'Puddles Cyan' },
    { id: 8, name: 'Puddles Red' },
    { id: 9, name: 'Candles' },
    { id: 11, name: 'Puddles Pink' },
    { id: 12, name: 'Wipe Cyan' },
    { id: 13, name: 'Wipe White' },
    { id: 14, name: 'Wipe Red' },
    { id: 15, name: 'Wipe Green' },
  ];

  presets.set(presetsList);
}

/**
 * Toggle board power
 */
export async function toggleBoardPower(boardId: string): Promise<void> {
  if (!browser) return;

  let currentBoards: BoardState[] = [];
  const unsubscribe = boards.subscribe((b) => {
    currentBoards = b;
  });
  unsubscribe();

  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Toggle all members in parallel
      const memberStatesPromises = board.memberIds.map(async (memberId) => {
        try {
          const response = await fetch(`${API_URL}/board/${memberId}/toggle`, {
            method: 'POST',
          });
          if (!response.ok) throw new Error(`Failed to toggle ${memberId}`);
          return await response.json();
        } catch (e) {
          console.error(`Error toggling ${memberId}:`, e);
          return null;
        }
      });

      const memberStates = (await Promise.all(memberStatesPromises)).filter(
        (state): state is BoardState => state !== null
      );

      // Update group's state based on members
      const allOn = memberStates.length > 0 && memberStates.every((m) => m.on);
      boards.update((currentBoards) =>
        currentBoards.map((b) => (b.id === boardId ? { ...b, on: allOn } : b))
      );
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/toggle`, {
        method: 'POST',
      });
      if (!response.ok) throw new Error('Failed to toggle power');
    }
  } catch (error) {
    console.error('Error toggling power:', error);
    boardsError.set('Failed to toggle power.');
  }
}

/**
 * Set board color
 */
export async function setBoardColor(
  boardId: string,
  r: number,
  g: number,
  b: number
): Promise<void> {
  if (!browser) return;

  const red = Number(r);
  const green = Number(g);
  const blue = Number(b);

  let currentBoards: BoardState[] = [];
  const unsubscribe = boards.subscribe((b) => {
    currentBoards = b;
  });
  unsubscribe();

  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Set color for all members in parallel
      await Promise.all(
        board.memberIds.map(async (memberId) => {
          try {
            const response = await fetch(`${API_URL}/board/${memberId}/color`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ r: red, g: green, b: blue }),
            });
            if (!response.ok) throw new Error(`Failed to set color for ${memberId}`);
          } catch (e) {
            console.error(`Error setting color for ${memberId}:`, e);
          }
        })
      );

      // Update group's state optimistically
      boards.update((currentBoards) =>
        currentBoards.map((b) =>
          b.id === boardId ? { ...b, color: [red, green, blue] as [number, number, number] } : b
        )
      );
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/color`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ r: red, g: green, b: blue }),
      });
      if (!response.ok) throw new Error('Failed to set color');
    }
  } catch (error) {
    console.error('Error setting color:', error);
    boardsError.set('Failed to set color.');
  }
}

/**
 * Set board brightness
 */
export async function setBoardBrightness(boardId: string, brightness: number): Promise<void> {
  if (!browser) return;

  let currentBoards: BoardState[] = [];
  const unsubscribe = boards.subscribe((b) => {
    currentBoards = b;
  });
  unsubscribe();

  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Set brightness for all members in parallel
      await Promise.all(
        board.memberIds.map(async (memberId) => {
          try {
            const response = await fetch(`${API_URL}/board/${memberId}/brightness`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ brightness }),
            });
            if (!response.ok) throw new Error(`Failed to set brightness for ${memberId}`);
          } catch (e) {
            console.error(`Error setting brightness for ${memberId}:`, e);
          }
        })
      );

      // Update group's state optimistically
      boards.update((currentBoards) =>
        currentBoards.map((b) => (b.id === boardId ? { ...b, brightness } : b))
      );
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/brightness`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ brightness }),
      });
      if (!response.ok) throw new Error('Failed to set brightness');
    }
  } catch (error) {
    console.error('Error setting brightness:', error);
    boardsError.set('Failed to set brightness.');
  }
}

/**
 * Set board effect
 */
export async function setBoardEffect(boardId: string, effect: number): Promise<void> {
  if (!browser) return;

  let currentBoards: BoardState[] = [];
  const unsubscribe = boards.subscribe((b) => {
    currentBoards = b;
  });
  unsubscribe();

  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Set effect for all members in parallel
      await Promise.all(
        board.memberIds.map(async (memberId) => {
          try {
            const response = await fetch(`${API_URL}/board/${memberId}/effect`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ effect }),
            });
            if (!response.ok) throw new Error(`Failed to set effect for ${memberId}`);
          } catch (e) {
            console.error(`Error setting effect for ${memberId}:`, e);
          }
        })
      );

      // Update group's state optimistically
      boards.update((currentBoards) =>
        currentBoards.map((b) => (b.id === boardId ? { ...b, effect } : b))
      );
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/effect`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ effect }),
      });
      if (!response.ok) throw new Error('Failed to set effect');
    }
  } catch (error) {
    console.error('Error setting effect:', error);
    boardsError.set('Failed to set effect.');
  }
}

/**
 * Set board preset
 */
export async function setBoardPreset(boardId: string, preset: number): Promise<void> {
  if (!browser) return;

  let currentBoards: BoardState[] = [];
  const unsubscribe = boards.subscribe((b) => {
    currentBoards = b;
  });
  unsubscribe();

  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Set preset for all members in parallel
      await Promise.all(
        board.memberIds.map(async (memberId) => {
          try {
            const response = await fetch(`${API_URL}/board/${memberId}/preset`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ preset, transition: 0 }),
            });
            if (!response.ok) throw new Error(`Failed to set preset for ${memberId}`);
          } catch (e) {
            console.error(`Error setting preset for ${memberId}:`, e);
          }
        })
      );
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/preset`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ preset, transition: 0 }),
      });
      if (!response.ok) throw new Error('Failed to set preset');
    }
  } catch (error) {
    console.error('Error setting preset:', error);
    boardsError.set('Failed to set preset.');
  }
}

/**
 * Add a new board
 */
export async function addBoard(id: string, ip: string): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, ip }),
    });

    if (!res.ok) {
      if (res.status === 409) {
        throw new Error('A board with this ID already exists');
      }
      throw new Error(`Failed to add board: ${res.statusText}`);
    }

    // Reload boards after adding
    await fetchBoards();
  } catch (error) {
    console.error('Failed to add board:', error);
    boardsError.set('Failed to add board.');
    throw error;
  }
}

/**
 * Update a board
 */
export async function updateBoard(boardId: string, newId: string, newIp: string): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards/${boardId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        new_id: newId !== boardId ? newId : undefined,
        new_ip: newIp,
      }),
    });

    if (!res.ok) {
      throw new Error(`Failed to update board: ${res.statusText}`);
    }

    // Reload boards after updating
    await fetchBoards();
  } catch (error) {
    console.error('Failed to update board:', error);
    boardsError.set('Failed to update board.');
    throw error;
  }
}

/**
 * Delete a board
 */
export async function deleteBoard(boardId: string): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards/${boardId}`, {
      method: 'DELETE',
    });

    if (!res.ok) {
      if (res.status === 404) {
        throw new Error('Board not found');
      }
      throw new Error(`Failed to delete board: ${res.statusText}`);
    }

    // Reload boards after deleting
    await fetchBoards();
  } catch (error) {
    console.error('Failed to delete board:', error);
    boardsError.set('Failed to delete board.');
    throw error;
  }
}

/**
 * Cleanup (reset stores and close SSE)
 */
export function cleanupBoardsListener(): void {
  if (sseConnection) {
    sseConnection.close();
    sseConnection = null;
  }

  boards.set([]);
  boardsLoading.set(true);
  boardsError.set(null);
}
