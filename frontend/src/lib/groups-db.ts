// frontend/src/lib/groups-db.ts
import { browser } from '$app/environment';
import { API_URL } from './api';

/**
 * Add a new group
 */
export async function addGroup(id: string, memberIds: string[], universe?: number): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/groups`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, members: memberIds, universe }),
    });

    if (!res.ok) {
      if (res.status === 409) {
        throw new Error('A board or group with this ID already exists');
      } else if (res.status === 400) {
        throw new Error('One or more member boards not found');
      }
      throw new Error(`Failed to create group: ${res.statusText}`);
    }
  } catch (error) {
    console.error('Failed to add group:', error);
    throw error;
  }
}

/**
 * Update an existing group
 */
export async function updateGroup(groupId: string, newId: string, memberIds: string[], universe?: number): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/groups/${groupId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: newId, members: memberIds, universe }),
    });

    if (!res.ok) {
      if (res.status === 404) {
        throw new Error('Group not found');
      } else if (res.status === 409) {
        throw new Error('A board or group with this ID already exists');
      } else if (res.status === 400) {
        throw new Error('One or more member boards not found');
      }
      throw new Error(`Failed to update group: ${res.statusText}`);
    }
  } catch (error) {
    console.error('Failed to update group:', error);
    throw error;
  }
}

/**
 * Delete a group
 */
export async function deleteGroup(groupId: string): Promise<void> {
  if (!browser) return;

  try {
    const res = await fetch(`${API_URL}/groups/${groupId}`, {
      method: 'DELETE',
    });

    if (!res.ok) {
      if (res.status === 404) {
        throw new Error('Group not found');
      }
      throw new Error(`Failed to delete group: ${res.statusText}`);
    }
  } catch (error) {
    console.error('Failed to delete group:', error);
    throw error;
  }
}
