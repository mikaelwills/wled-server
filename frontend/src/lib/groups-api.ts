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
 */
export async function setGroupPreset(
  groupId: string,
  preset: number,
  transition: number = 0
): Promise<GroupOperationResult> {
  if (!browser) throw new Error('Not in browser environment');

  const response = await fetch(`${API_URL}/group/${groupId}/preset`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ preset, transition }),
  });

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