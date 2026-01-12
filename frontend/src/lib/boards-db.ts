// frontend/src/lib/boards-db.ts
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { boards, boardsLoading, boardsError, presets, performancePresets, patternPresets } from './store';
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

function getMajorityColor(members: BoardState[]): number[] | undefined {
  const connected = members.filter(m => m.connected);
  const source = connected.length > 0 ? connected : members;
  if (source.length === 0) return undefined;
  const counts = new Map<string, { color: number[]; count: number }>();
  for (const m of source) {
    if (m.color) {
      const key = JSON.stringify(m.color);
      const existing = counts.get(key);
      if (existing) {
        existing.count++;
      } else {
        counts.set(key, { color: m.color, count: 1 });
      }
    }
  }
  let majority: { color: number[]; count: number } | undefined;
  for (const entry of counts.values()) {
    if (!majority || entry.count > majority.count) {
      majority = entry;
    }
  }
  return majority?.color;
}

/**
 * Recalculate group states based on current member board states
 * CRITICAL: Never modify group identity (isGroup, memberIds) - only update operational state
 */
async function updateBoardFromResponse(response: Response, boardId: string): Promise<void> {
  const updatedState: BoardState = await response.json();
  boards.update((currentBoards) =>
    currentBoards.map((b) => (b.id === boardId && !b.isGroup ? updatedState : b))
  );
}

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
        color: getMajorityColor(members) ?? board.color,
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
 * IMPORTANT: Waits for initial fetch to complete before starting SSE to avoid race condition
 */
export async function initBoardsListener(): Promise<void> {
  if (!browser) return;

  boardsLoading.set(true);
  boardsError.set(null);

  try {
    // Wait for initial fetch to complete BEFORE starting SSE
    // This prevents race condition where SSE updates arrive before boards are loaded
    await fetchBoards();

    // Set up SSE for real-time updates (now boards are guaranteed to be populated)
    sseConnection = createSseConnection(
      (boardId: string, state: BoardState) => {
        // Optimized: Single-pass update of board state and affected groups
        boards.update((currentBoards) => {
          // Early return if board doesn't exist (defensive - avoids wasted work in edge cases)
          if (!currentBoards.some(b => b.id === boardId && !b.isGroup)) {
            return currentBoards;
          }

          // Build Map ONCE before iteration for O(1) board lookups
          const boardsMap = new Map<string, BoardState>();
          for (const board of currentBoards) {
            if (!board.isGroup) {
              boardsMap.set(board.id, board);
            }
          }
          // Update map with new state for the changed board
          boardsMap.set(boardId, state);

          return currentBoards.map((board) => {
            // Update the specific board that changed
            if (board.id === boardId) {
              // Groups should not receive individual state updates via SSE
              if (board.isGroup) {
                return board; // Skip silently - this shouldn't happen in normal operation
              }
              return state;
            }

            // Recalculate groups that contain the updated board
            if (board.isGroup && board.memberIds?.includes(boardId)) {
              // Use Map for O(1) member lookups
              const members: BoardState[] = [];
              if (board.memberIds) {
                for (const memberId of board.memberIds) {
                  const member = boardsMap.get(memberId);
                  if (member) members.push(member);
                }
              }

              return {
                ...board,
                color: getMajorityColor(members) ?? board.color,
                brightness: members.length > 0 ? members[0].brightness : board.brightness,
                effect: members.length > 0 ? members[0].effect : board.effect,
                on: newOn,
              };
            }

            return board;
          });
        });
      },
      (boardId: string, connected: boolean) => {
        // Optimized: Single-pass update of connection status and affected groups
        boards.update((currentBoards) => {
          // Early return if board doesn't exist (defensive - avoids wasted work in edge cases)
          if (!currentBoards.some(b => b.id === boardId && !b.isGroup)) {
            return currentBoards;
          }

          // Build Map ONCE before iteration for O(1) board lookups
          const boardsMap = new Map<string, BoardState>();
          let updatedBoard: BoardState | null = null;

          // First pass: build complete map and find/update target board
          for (const board of currentBoards) {
            if (!board.isGroup) {
              if (board.id === boardId) {
                // Update connection status for target board
                updatedBoard = { ...board, connected };
                boardsMap.set(board.id, updatedBoard);
              } else {
                boardsMap.set(board.id, board);
              }
            }
          }

          return currentBoards.map((board) => {
            // Return updated board if this is the target
            if (board.id === boardId) {
              // Groups don't have connection status changes via SSE
              if (board.isGroup) {
                return board; // Skip silently
              }
              return updatedBoard!;
            }

            // Recalculate groups that contain the updated board
            if (board.isGroup && board.memberIds?.includes(boardId)) {
              // Use Map for O(1) member lookups
              const members: BoardState[] = [];
              if (board.memberIds) {
                for (const memberId of board.memberIds) {
                  const member = boardsMap.get(memberId);
                  if (member) members.push(member);
                }
              }

              return {
                ...board,
                color: getMajorityColor(members) ?? board.color,
                brightness: members.length > 0 ? members[0].brightness : board.brightness,
                effect: members.length > 0 ? members[0].effect : board.effect,
                on: members.length > 0 ? members.some((m) => m.on) : board.on,
              };
            }

            return board;
          });
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

    // API returns { boards: [...], groups: [...] }
    const boardsList = Array.isArray(data.boards) ? data.boards : [];
    const groupsList = Array.isArray(data.groups) ? data.groups : [];

    // Convert groups to BoardState format with isGroup flag
    const groupBoards = groupsList.map((g: any) => ({
      id: g.id,
      ip: '',
      on: g.power ?? false,
      brightness: g.brightness ?? 128,
      color: g.color ?? [255, 255, 255],
      effect: 0,
      speed: 128,
      intensity: 128,
      connected: true,
      isGroup: true,
      memberIds: g.members ?? [],
      universe: g.universe
    }));

    // Combine boards and groups
    const loadedBoards = [...boardsList, ...groupBoards];

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

    // API returns { boards: [...], groups: [...] }
    const boardsList = Array.isArray(data.boards) ? data.boards : [];
    const groupsList = Array.isArray(data.groups) ? data.groups : [];

    // Convert groups to BoardState format with isGroup flag
    const freshGroups = groupsList.map((g: any) => ({
      id: g.id,
      ip: '',
      on: g.power ?? false,
      brightness: g.brightness ?? 128,
      color: g.color ?? [255, 255, 255],
      effect: 0,
      speed: 128,
      intensity: 128,
      connected: true,
      isGroup: true,
      memberIds: g.members ?? [],
      universe: g.universe
    }));

    const freshRegularBoards = boardsList;

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
      // Sort presets by effect type then name (Blackout is already in the server presets)
      const presetsList = presetsData
          .map((p: any) => ({ id: p.wled_slot, name: p.name, state: p.state }))
          .sort((a: any, b: any) => {
            // Extract first word (effect type) from preset name
            const typeA = a.name.split(' ')[0];
            const typeB = b.name.split(' ')[0];

            // Sort by type first, then by full name
            if (typeA === typeB) {
              return a.name.localeCompare(b.name);
            }
            return typeA.localeCompare(typeB);
          });
      presets.set(presetsList);
    } else {
      console.error('Failed to fetch presets:', response.statusText);
    }
  } catch (error) {
    console.error('Error fetching presets:', error);
  }
}

/**
 * Fetch performance presets from server (effects engine presets for E1.31)
 */
export async function fetchPerformancePresets(): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/effects/presets`);
    if (response.ok) {
      const data = await response.json();
      performancePresets.set(data);
    } else {
      console.error('Failed to fetch performance presets:', response.statusText);
    }
  } catch (error) {
    console.error('Error fetching performance presets:', error);
  }
}

/**
 * Fetch pattern presets from server (group patterns like wave, random, etc.)
 */
export async function fetchPatternPresets(): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/patterns/presets`);
    if (response.ok) {
      const data = await response.json();
      patternPresets.set(data);
    } else {
      console.error('Failed to fetch pattern presets:', response.statusText);
    }
  } catch (error) {
    console.error('Error fetching pattern presets:', error);
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
        message: `Replaced presets on board (${result.preset_count} presets)`
      };
    } else {
      const error = await response.text();
      return { success: false, message: error };
    }
  } catch (error) {
    console.error('Error replacing presets:', error);
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

      boards.update((currentBoards) =>
        currentBoards.map((b) => {
          if (b.id === boardId && b.isGroup) {
            return { ...b, on: power };
          }
          if (board?.memberIds?.includes(b.id) && !b.isGroup) {
            return { ...b, on: power };
          }
          return b;
        })
      );

      try {
        await setGroupPower(boardId, power);
      } catch (groupError) {
        // Log to console but don't show global error - boards already show as disconnected
        console.warn(`Group ${boardId} power command failed (boards may be unreachable):`, groupError);
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

      await updateBoardFromResponse(response, boardId);
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
        // Log to console but don't show global error - boards already show as disconnected
        console.warn(`Group ${boardId} color command failed (boards may be unreachable):`, groupError);
      }
    } else {
      // Regular board - update store from response
      const response = await fetch(`${API_URL}/board/${boardId}/color`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ r: red, g: green, b: blue }),
      });
      if (!response.ok) throw new Error('Failed to set color');

      await updateBoardFromResponse(response, boardId);
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
        // Log to console but don't show global error - boards already show as disconnected
        console.warn(`Group ${boardId} brightness command failed (boards may be unreachable):`, groupError);
      }
    } else {
      // Regular board - update store from response
      const response = await fetch(`${API_URL}/board/${boardId}/brightness`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ brightness }),
      });
      if (!response.ok) throw new Error('Failed to set brightness');

      await updateBoardFromResponse(response, boardId);
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
        // Log to console but don't show global error - boards already show as disconnected
        console.warn(`Group ${boardId} effect command failed (boards may be unreachable):`, groupError);
      }
    } else {
      // Regular board - update store from response
      const response = await fetch(`${API_URL}/board/${boardId}/effect`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ effect }),
      });
      if (!response.ok) throw new Error('Failed to set effect');

      await updateBoardFromResponse(response, boardId);
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
      const response = await fetch(`${API_URL}/group/${boardId}/speed`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ speed }),
      });
      if (!response.ok) throw new Error('Failed to set group speed');
    } else {
      // Regular board - update store from response
      const response = await fetch(`${API_URL}/board/${boardId}/speed`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ speed }),
      });
      if (!response.ok) throw new Error('Failed to set speed');

      await updateBoardFromResponse(response, boardId);
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
      // Regular board - update store from response
      const response = await fetch(`${API_URL}/board/${boardId}/intensity`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ intensity }),
      });
      if (!response.ok) throw new Error('Failed to set intensity');

      await updateBoardFromResponse(response, boardId);
    }
  } catch (error) {
    console.error('Error setting intensity:', error);
    boardsError.set('Failed to set intensity.');
  }
}

/**
 * Set board preset
 * When options.bpm is provided, uses DMX Mode 2 for atomic effect state with BPM-synced speed
 */
export async function setBoardPreset(
  boardId: string,
  preset: number,
  options?: { bpm?: number; presetName?: string }
): Promise<void> {
  if (!browser) return;

  const currentBoards = get(boards);
  const board = currentBoards.find((b) => b.id === boardId);

  try {
    if (board?.isGroup && board.memberIds) {
      try {
        await setGroupPreset(boardId, preset, 0, options);
      } catch (groupError) {
        console.warn(`Group ${boardId} preset command failed (boards may be unreachable):`, groupError);
      }
    } else {
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
export async function addBoard(id: string, ip: string, ledCount?: number, universe?: number): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, ip, led_count: ledCount, universe }),
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
export async function updateBoard(boardId: string, newId: string, newIp: string, ledCount?: number, universe?: number): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/boards/${boardId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        new_id: newId !== boardId ? newId : undefined,
        new_ip: newIp,
        led_count: ledCount,
        universe: universe,
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
 * Set board transition time
 */
export async function setBoardTransition(boardId: string, transition: number): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/board/${boardId}/transition`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ transition }),
    });

    if (!response.ok) {
      throw new Error('Failed to set transition');
    }

    // Update local state optimistically
    boards.update((currentBoards) =>
      currentBoards.map((b) =>
        b.id === boardId ? { ...b, transition } : b
      )
    );
  } catch (error) {
    console.error('Failed to set transition:', error);
    boardsError.set('Failed to set transition.');
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
