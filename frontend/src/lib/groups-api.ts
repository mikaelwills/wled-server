// frontend/src/lib/groups-api.ts
import { browser } from '$app/environment';
import { API_URL } from './api';
import type { BoardState } from './types';

export interface GroupOperationResult {
  group_id: string;
  successful_members: string[];
  failed_members: [string, string][]; // (board_id, error_message)
  member_states: BoardState[];
}

/**
 * Set power for all boards in a group
 */
export async function setGroupPower(groupId: string, power: boolean): Promise<GroupOperationResult> {
  if (!browser) throw new Error('Not in browser environment');

  const response = await fetch(`${API_URL}/group/${groupId}/power`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ on: power }),
  });

  if (!response.ok) {
    throw new Error(`Failed to set group power: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Set color for all boards in a group
 */
export async function setGroupColor(
  groupId: string,
  r: number,
  g: number,
  b: number,
  transition: number = 0
): Promise<GroupOperationResult> {
  if (!browser) throw new Error('Not in browser environment');

  const response = await fetch(`${API_URL}/group/${groupId}/color`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ r, g, b, transition }),
  });

  if (!response.ok) {
    throw new Error(`Failed to set group color: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Set brightness for all boards in a group
 */
export async function setGroupBrightness(
  groupId: string,
  brightness: number,
  transition: number = 0
): Promise<GroupOperationResult> {
  if (!browser) throw new Error('Not in browser environment');

  const response = await fetch(`${API_URL}/group/${groupId}/brightness`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ brightness, transition }),
  });

  if (!response.ok) {
    throw new Error(`Failed to set group brightness: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Set effect for all boards in a group
 */
export async function setGroupEffect(
  groupId: string,
  effect: number,
  transition: number = 0
): Promise<GroupOperationResult> {
  if (!browser) throw new Error('Not in browser environment');

  const response = await fetch(`${API_URL}/group/${groupId}/effect`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ effect, transition }),
  });

  if (!response.ok) {
    throw new Error(`Failed to set group effect: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Set preset for all boards in a group
 * When bpm is provided, uses DMX Mode 2 for atomic effect state with BPM-synced speed
 */
export async function setGroupPreset(
  groupId: string,
  preset: number,
  transition: number = 0,
  options?: { bpm?: number; presetName?: string }
): Promise<GroupOperationResult> {
  if (!browser) throw new Error('Not in browser environment');

  const fetchStartTime = performance.now();
  const bpmInfo = options?.bpm ? ` bpm=${options.bpm}` : '';
  console.log(`🌐 [${fetchStartTime.toFixed(3)}ms] Firing HTTP request: group='${groupId}' preset=${preset}${bpmInfo}`);

  const body: Record<string, any> = { preset, transition };
  if (options?.bpm) body.bpm = options.bpm;
  if (options?.presetName) body.preset_name = options.presetName;

  const response = await fetch(`${API_URL}/group/${groupId}/preset`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  const fetchEndTime = performance.now();
  console.log(`✅ [${fetchEndTime.toFixed(3)}ms] HTTP response received: group='${groupId}' (took ${(fetchEndTime - fetchStartTime).toFixed(1)}ms)`);

  if (!response.ok) {
    throw new Error(`Failed to set group preset: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Delete a group
 */
export async function deleteGroup(groupId: string): Promise<void> {
  if (!browser) throw new Error('Not in browser environment');

  const response = await fetch(`${API_URL}/groups/${groupId}`, {
    method: 'DELETE',
  });

  if (!response.ok) {
    throw new Error(`Failed to delete group: ${response.statusText}`);
  }
}