// frontend/src/lib/boards-db.ts
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { boards, boardsLoading, boardsError, presets } from './store';
import { API_URL } from './api';
import { createSseConnection } from './sse';
import { 
  setGroupPower, 
  setGroupColor, 
  setGroupBrightness, 
  setGroupEffect, 
  setGroupPreset,
  type GroupOperationResult 
} from './groups-api';
import type { BoardState } from './types';

let sseConnection: EventSource | null = null;

/**
 * Recalculate group states based on current member board states
 * CRITICAL: Never modify group identity (isGroup, memberIds) - only update operational state
 */
function recalculateGroupStates(currentBoards: BoardState[]): BoardState[] {
  return currentBoards.map((board) => {
    if (board.isGroup && board.memberIds) {
      const members = currentBoards.filter(
        (b) => !b.isGroup && board.memberIds?.includes(b.id)
      );

      // CRITICAL: Preserve immutable group identity, only update derived operational state
      return {
        // Preserve all existing group properties
        ...board,
        // NEVER modify these immutable group properties:
        // - isGroup: always true for groups
        // - memberIds: only changed via explicit group management
        // Only update derived operational state from member boards:
        color: members.length > 0 ? members[0].color : board.color,
        brightness: members.length > 0 ? members[0].brightness : board.brightness,
        effect: members.length > 0 ? members[0].effect : board.effect,
        on: members.length > 0 ? members.some((m) => m.on) : board.on,
      };
    }
    return board;
  });
}

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
        // Update specific board state and recalculate groups
        boards.update((currentBoards) => {
          const updatedBoards = currentBoards.map((b) => {
            if (b.id === boardId) {
              // CRITICAL: Preserve group identity - never overwrite group properties with individual board state
              if (b.isGroup) {
                // This is a group, don't overwrite with individual board state
                console.warn(`SSE: Ignoring individual board update for group ${boardId}`);
                return b;
              }
              // This is an individual board, update its state
              return state;
            }
            return b;
          });
          return recalculateGroupStates(updatedBoards);
        });
      },
      (boardId: string, connected: boolean) => {
        // Update connection status and recalculate groups
        boards.update((currentBoards) => {
          const updatedBoards = currentBoards.map((b) => {
            if (b.id === boardId) {
              // CRITICAL: Preserve group identity - never overwrite group properties with individual board state
              if (b.isGroup) {
                // This is a group, don't overwrite with individual board state
                console.warn(`SSE: Ignoring connection update for group ${boardId}`);
                return b;
              }
              // This is an individual board, update its connection status
              return { ...b, connected };
            }
            return b;
          });
          return recalculateGroupStates(updatedBoards);
        });
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
export async function fetchBoards(): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards`);

    if (!res.ok) {
      throw new Error(`Failed to fetch boards: ${res.statusText}`);
    }

    const data = await res.json();
    const loadedBoards = Array.isArray(data) ? data : [];

    // Derive group state from member boards using helper function
    const processedBoards = recalculateGroupStates(loadedBoards);

    // Sort: groups first, then regular boards
    processedBoards.sort((a: BoardState, b: BoardState) => {
      if (a.isGroup && !b.isGroup) return -1;
      if (!a.isGroup && b.isGroup) return 1;
      return 0;
    });

    boards.set(processedBoards);
    boardsLoading.set(false);
  } catch (error) {
    console.error('Failed to fetch boards:', error);
    boards.set([]);
    boardsLoading.set(false);
  }
}

/**
 * Refresh only groups from the API, keeping individual board states
 */
export async function refreshGroups(): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards`);

    if (!res.ok) {
      throw new Error(`Failed to fetch boards for group refresh: ${res.statusText}`);
    }

    const data = await res.json();
    const loadedBoards = Array.isArray(data) ? data : [];

    // Separate groups and regular boards from fresh data
    const freshGroups = loadedBoards.filter(board => board.isGroup);
    const freshRegularBoards = loadedBoards.filter(board => !board.isGroup);

    // Get current boards to preserve individual board states
    const currentBoards = get(boards);
    const currentRegularBoards = currentBoards.filter(board => !board.isGroup);

    // Use fresh regular boards if they exist, otherwise keep current ones
    // This ensures we have the latest board data while preserving states
    const regularBoards = freshRegularBoards.length > 0 ? freshRegularBoards : currentRegularBoards;

    // Derive group state from member boards using helper function
    const processedGroups = recalculateGroupStates(freshGroups);

    // Combine updated groups with regular boards
    const allBoards = [...processedGroups, ...regularBoards];

    // Sort: groups first, then regular boards
    allBoards.sort((a: BoardState, b: BoardState) => {
      if (a.isGroup && !b.isGroup) return -1;
      if (!a.isGroup && b.isGroup) return 1;
      return a.id.localeCompare(b.id); // Sort by ID as tiebreaker
    });

    boards.set(allBoards);
  } catch (error) {
    console.error('Failed to refresh groups:', error);
  }
}

/**
 * Fetch all presets from server
 */
export async function fetchPresets(): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/presets`);
    if (response.ok) {
      const presetsData = await response.json();
      // Add "No Preset" option at the front, sorted by effect type then name
      const presetsList = [
        { id: 0, name: 'No Preset' },
        ...presetsData
          .map((p: any) => ({ id: p.wled_slot, name: p.name }))
          .sort((a: any, b: any) => {
            // Extract first word (effect type) from preset name
            const typeA = a.name.split(' ')[0];
            const typeB = b.name.split(' ')[0];

            // Sort by type first, then by full name
            if (typeA === typeB) {
              return a.name.localeCompare(b.name);
            }
            return typeA.localeCompare(typeB);
          })
      ];
      presets.set(presetsList);
    } else {
      console.error('Failed to fetch presets:', response.statusText);
    }
  } catch (error) {
    console.error('Error fetching presets:', error);
  }
}

/**
 * Sync all presets to a board
 */
export async function syncPresetsToBoard(boardId: string): Promise<{ success: boolean; message: string }> {
  if (!browser) return { success: false, message: 'Not in browser context' };

  try {
    const response = await fetch(`${API_URL}/board/${boardId}/presets/sync`, {
      method: 'POST'
    });

    if (response.ok) {
      const result = await response.json();
      return {
        success: true,
        message: `Synced ${result.synced} of ${result.total} presets`
      };
    } else {
      const error = await response.text();
      return { success: false, message: error };
    }
  } catch (error) {
    console.error('Error syncing presets:', error);
    return { success: false, message: String(error) };
  }
}

/**
 * Set board power
 */
export async function setBoardPower(boardId: string, power: boolean): Promise<void> {
  if (!browser) return;

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // This is a GROUP - use group endpoint for atomic group operation
      try {
        const result: GroupOperationResult = await setGroupPower(boardId, power);
        
        // Log any failures for debugging
        // if (result.failed_members.length > 0) {
        //   console.warn(`Group ${boardId} - Failed members:`, result.failed_members);
        // }
        
        // Update group's state optimistically, preserving group identity
        boards.update((currentBoards) =>
          currentBoards.map((b) => {
            if (b.id === boardId) {
              // CRITICAL: Preserve immutable group identity
              if (!b.isGroup) {
                console.error(`Group operation attempted on non-group ${boardId}`);
                return b;
              }
              return {
                // Preserve ALL existing group properties
                ...b,
                // Only update derived operational state
                on: power,
                // CRITICAL: Never modify these immutable group properties:
                isGroup: true, // Preserve group identity
                memberIds: b.memberIds, // Preserve group membership
                ip: b.ip, // Preserve group IP (empty string)
                connected: b.connected, // Preserve group connection status
              };
            }
            return b;
          })
        );
      } catch (groupError) {
        console.error(`Failed to set power for group ${boardId}:`, groupError);
        boardsError.set(`Failed to set group power: ${groupError instanceof Error ? groupError.message : 'Unknown error'}`);
      }
    } else {
      // This is an INDIVIDUAL BOARD - use individual board endpoint
      // Even if this board is a member of a group, we still control it individually
      const response = await fetch(`${API_URL}/board/${boardId}/power`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ on: power }),
      });
      if (!response.ok) throw new Error('Failed to set power');
    }
  } catch (error) {
    console.error('Error setting power:', error);
    boardsError.set('Failed to set power.');
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

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Use new group endpoint for atomic group operation
      try {
        const result: GroupOperationResult = await setGroupColor(boardId, red, green, blue);
        
        // Log any failures for debugging
        // if (result.failed_members.length > 0) {
        //   console.warn(`Group ${boardId} - Failed members:`, result.failed_members);
        // }
        
        // Update group's state optimistically, preserving group identity
        boards.update((currentBoards) =>
          currentBoards.map((b) => {
            if (b.id === boardId) {
              // CRITICAL: Preserve immutable group identity
              if (!b.isGroup) {
                console.error(`Group operation attempted on non-group ${boardId}`);
                return b;
              }
              return {
                // Preserve ALL existing group properties
                ...b,
                // Only update derived operational state
                color: [red, green, blue] as [number, number, number],
                // CRITICAL: Never modify these immutable group properties:
                isGroup: true, // Preserve group identity
                memberIds: b.memberIds, // Preserve group membership
                ip: b.ip, // Preserve group IP (empty string)
                connected: b.connected, // Preserve group connection status
              };
            }
            return b;
          })
        );
      } catch (groupError) {
        console.error(`Failed to set color for group ${boardId}:`, groupError);
        boardsError.set(`Failed to set group color: ${groupError instanceof Error ? groupError.message : 'Unknown error'}`);
      }
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

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Use new group endpoint for atomic group operation
      try {
        const result: GroupOperationResult = await setGroupBrightness(boardId, brightness);
        
        // Log any failures for debugging
        // if (result.failed_members.length > 0) {
        //   console.warn(`Group ${boardId} - Failed members:`, result.failed_members);
        // }
        
        // Update group's state optimistically, preserving group identity
        boards.update((currentBoards) =>
          currentBoards.map((b) => {
            if (b.id === boardId) {
              // CRITICAL: Preserve immutable group identity
              if (!b.isGroup) {
                console.error(`Group operation attempted on non-group ${boardId}`);
                return b;
              }
              return {
                // Preserve ALL existing group properties
                ...b,
                // Only update derived operational state
                brightness,
                // CRITICAL: Never modify these immutable group properties:
                isGroup: true, // Preserve group identity
                memberIds: b.memberIds, // Preserve group membership
                ip: b.ip, // Preserve group IP (empty string)
                connected: b.connected, // Preserve group connection status
              };
            }
            return b;
          })
        );
      } catch (groupError) {
        console.error(`Failed to set brightness for group ${boardId}:`, groupError);
        boardsError.set(`Failed to set group brightness: ${groupError instanceof Error ? groupError.message : 'Unknown error'}`);
      }
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

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Use new group endpoint for atomic group operation
      try {
        const result: GroupOperationResult = await setGroupEffect(boardId, effect);
        
        // Log any failures for debugging
        // if (result.failed_members.length > 0) {
        //   console.warn(`Group ${boardId} - Failed members:`, result.failed_members);
        // }
        
        // Update group's state optimistically, preserving group identity
        boards.update((currentBoards) =>
          currentBoards.map((b) => {
            if (b.id === boardId) {
              // CRITICAL: Preserve immutable group identity
              if (!b.isGroup) {
                console.error(`Group operation attempted on non-group ${boardId}`);
                return b;
              }
              return {
                // Preserve ALL existing group properties
                ...b,
                // Only update derived operational state
                effect,
                // CRITICAL: Never modify these immutable group properties:
                isGroup: true, // Preserve group identity
                memberIds: b.memberIds, // Preserve group membership
                ip: b.ip, // Preserve group IP (empty string)
                connected: b.connected, // Preserve group connection status
              };
            }
            return b;
          })
        );
      } catch (groupError) {
        console.error(`Failed to set effect for group ${boardId}:`, groupError);
        boardsError.set(`Failed to set group effect: ${groupError instanceof Error ? groupError.message : 'Unknown error'}`);
      }
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
 * Set board speed
 */
export async function setBoardSpeed(boardId: string, speed: number): Promise<void> {
  if (!browser) return;

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // For now, skip group handling - could be added later
      console.warn('Group speed control not yet implemented');
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/speed`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ speed }),
      });
      if (!response.ok) throw new Error('Failed to set speed');
    }
  } catch (error) {
    console.error('Error setting speed:', error);
    boardsError.set('Failed to set speed.');
  }
}

/**
 * Set board intensity
 */
export async function setBoardIntensity(boardId: string, intensity: number): Promise<void> {
  if (!browser) return;

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // For now, skip group handling - could be added later
      console.warn('Group intensity control not yet implemented');
    } else {
      // Regular board - SSE will update the state
      const response = await fetch(`${API_URL}/board/${boardId}/intensity`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ intensity }),
      });
      if (!response.ok) throw new Error('Failed to set intensity');
    }
  } catch (error) {
    console.error('Error setting intensity:', error);
    boardsError.set('Failed to set intensity.');
  }
}

/**
 * Set board preset
 */
export async function setBoardPreset(boardId: string, preset: number): Promise<void> {
  if (!browser) return;

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      // Use new group endpoint for atomic group operation
      try {
        const result: GroupOperationResult = await setGroupPreset(boardId, preset);
        
        // Log any failures for debugging
        // if (result.failed_members.length > 0) {
        //   console.warn(`Group ${boardId} - Failed members:`, result.failed_members);
        // }
        
        // Note: We don't update group state optimistically for presets since they can have complex effects
        // SSE will update individual member states, which will then trigger group recalculation
      } catch (groupError) {
        console.error(`Failed to set preset for group ${boardId}:`, groupError);
        boardsError.set(`Failed to set group preset: ${groupError instanceof Error ? groupError.message : 'Unknown error'}`);
      }
    } else {
      // Regular board - SSE will update the state
      const fetchStartTime = performance.now();
      console.log(`🌐 [${fetchStartTime.toFixed(3)}ms] Firing HTTP request: board='${boardId}' preset=${preset}`);

      const response = await fetch(`${API_URL}/board/${boardId}/preset`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ preset, transition: 0 }),
      });

      const fetchEndTime = performance.now();
      console.log(`✅ [${fetchEndTime.toFixed(3)}ms] HTTP response received: board='${boardId}' (took ${(fetchEndTime - fetchStartTime).toFixed(1)}ms)`);

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
 * Set board LED count
 */
export async function setBoardLedCount(boardId: string, ledCount: number): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/board/${boardId}/led-count`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ led_count: ledCount }),
    });

    if (!response.ok) {
      throw new Error('Failed to set LED count');
    }

    // Update local state optimistically
    boards.update((currentBoards) =>
      currentBoards.map((b) =>
        b.id === boardId ? { ...b, ledCount: ledCount } : b
      )
    );
  } catch (error) {
    console.error('Failed to set LED count:', error);
    boardsError.set('Failed to set LED count.');
    throw error;
  }
}

/**
 * Reset board segment to defaults (grp=1, spc=0, of=0)
 */
export async function resetBoardSegment(boardId: string): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/board/${boardId}/reset-segment`, {
      method: 'POST',
    });

    if (!response.ok) {
      throw new Error('Failed to reset segment');
    }
  } catch (error) {
    console.error('Failed to reset segment:', error);
    boardsError.set('Failed to reset segment.');
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
