import { browser } from '$app/environment';
import { setlists, activeSetlistId, programs } from '$lib/stores/store';
import { Program } from '$lib/models/Program';
import type { Setlist } from '$lib/models/Setlist';
import { API_URL } from '$lib/api';

export async function initSetlists(): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/setlists`);
    if (!response.ok) {
      throw new Error(`Failed to load setlists: ${response.status}`);
    }

    const data = await response.json();
    const loaded: Setlist[] = (data.setlists || []).map((s: any) => ({
      id: s.id,
      name: s.name,
      displayOrder: s.display_order ?? 0,
    }));

    setlists.set(loaded);
    activeSetlistId.set(data.active_setlist_id || 'default');
  } catch (error) {
    console.error('[setlists-db] Failed to load setlists:', error);
  }
}

export async function createSetlist(name: string): Promise<Setlist | null> {
  if (!browser) return null;

  try {
    const response = await fetch(`${API_URL}/setlists`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    });

    if (!response.ok) {
      throw new Error(`Failed to create setlist: ${response.statusText}`);
    }

    const data = await response.json();
    const newSetlist: Setlist = {
      id: data.id,
      name: data.name,
      displayOrder: data.display_order ?? 0,
    };

    setlists.update(current => [...current, newSetlist]);
    return newSetlist;
  } catch (error) {
    console.error('[setlists-db] Failed to create setlist:', error);
    return null;
  }
}

export async function renameSetlist(id: string, name: string): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/setlists/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    });

    if (!response.ok) {
      throw new Error(`Failed to rename setlist: ${response.statusText}`);
    }

    setlists.update(current =>
      current.map(s => s.id === id ? { ...s, name } : s)
    );
  } catch (error) {
    console.error('[setlists-db] Failed to rename setlist:', error);
  }
}

export async function deleteSetlist(id: string): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/setlists/${id}`, {
      method: 'DELETE',
    });

    if (!response.ok) {
      throw new Error(`Failed to delete setlist: ${response.statusText}`);
    }

    setlists.update(current => current.filter(s => s.id !== id));

    programs.update(current =>
      current.map(p => p.setlistId === id ? Object.assign(p, { setlistId: 'default' }) : p)
    );

    activeSetlistId.update(current => current === id ? 'default' : current);
  } catch (error) {
    console.error('[setlists-db] Failed to delete setlist:', error);
  }
}

export async function activateSetlist(id: string): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/setlists/${id}/activate`, {
      method: 'POST',
    });

    if (!response.ok) {
      throw new Error(`Failed to activate setlist: ${response.statusText}`);
    }

    const data = await response.json();
    activeSetlistId.set(data.active_setlist_id);

    const loadedPrograms = (data.programs || [])
      .map((p: any) => Program.fromJson(p))
      .filter((p: Program | null): p is Program => p !== null);

    programs.update(current => {
      const otherPrograms = current.filter(p => p.setlistId !== id);
      return [...otherPrograms, ...loadedPrograms];
    });
  } catch (error) {
    console.error('[setlists-db] Failed to activate setlist:', error);
  }
}

export async function moveProgram(programId: string, targetSetlistId: string): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/programs/${programId}/move`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ setlist_id: targetSetlistId }),
    });

    if (!response.ok) {
      throw new Error(`Failed to move program: ${response.statusText}`);
    }

    programs.update(current =>
      current.map(p => p.id === programId ? Object.assign(p, { setlistId: targetSetlistId }) : p)
    );
  } catch (error) {
    console.error('[setlists-db] Failed to move program:', error);
  }
}

export async function cloneToSetlist(programId: string, targetSetlistId: string): Promise<Program | null> {
  if (!browser) return null;

  try {
    const response = await fetch(`${API_URL}/programs/${programId}/clone-to-setlist`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ setlist_id: targetSetlistId }),
    });

    if (!response.ok) {
      throw new Error(`Failed to clone program: ${response.statusText}`);
    }

    const data = await response.json();
    const newProgram = Program.fromJson(data);

    console.log(`[CUE-DEBUG] cloneToSetlist: cloned "${newProgram?.songName}" with ${newProgram?.cues?.length ?? 0} cues`);

    if (newProgram) {
      programs.update(current => [...current, newProgram]);
    }

    return newProgram;
  } catch (error) {
    console.error('[setlists-db] Failed to clone program:', error);
    return null;
  }
}
