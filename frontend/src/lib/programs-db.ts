// frontend/src/lib/programs-db.ts
import { browser } from '$app/environment';
import { programs, programsLoading, programsError } from './store';
import { Program } from './models/Program';
import { API_URL } from '$lib/api';

/**
 * Initialize programs from API
 */
export async function initPrograms(): Promise<void> {
  if (!browser) return;

  programsLoading.set(true);
  programsError.set(null);

  try {
    const response = await fetch(`${API_URL}/programs`);

    if (!response.ok) {
      throw new Error('Failed to load programs from server');
    }

    const data = await response.json();
    console.log('[programs-db] API returned programs:', data.map((p: any) => ({
      id: p.id,
      audioDataLength: p.audio_data?.length || 0
    })));

    const loadedPrograms = Array.isArray(data)
      ? data.map((p: any) => Program.fromJson(p)).filter((p): p is Program => p !== null)
      : [];

    // Sort by createdAt descending (newest first)
    loadedPrograms.sort((a, b) => {
      const dateA = new Date(a.createdAt || 0).getTime();
      const dateB = new Date(b.createdAt || 0).getTime();
      return dateB - dateA;
    });

    console.log('[programs-db] Loaded programs:', loadedPrograms.map(p => ({
      id: p.id,
      audioDataLength: p.audioData?.length || 0
    })));

    programs.set(loadedPrograms);
    programsLoading.set(false);
  } catch (error) {
    console.error('Failed to load programs:', error);
    programsError.set('Failed to load programs from server.');
    programsLoading.set(false);
    programs.set([]);
  }
}

/**
 * Save a program (create or update)
 */
export async function saveProgram(program: Program): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/programs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(program.toJson())
    });

    if (!response.ok) {
      throw new Error('Failed to save program to server');
    }

    // Update local store
    programs.update(currentPrograms => {
      const existingIndex = currentPrograms.findIndex(p => p.id === program.id);

      if (existingIndex >= 0) {
        // Update existing
        currentPrograms[existingIndex] = program;
        return [...currentPrograms];
      } else {
        // Add new at beginning (newest first)
        return [program, ...currentPrograms];
      }
    });
  } catch (error) {
    console.error('Failed to save program:', error);
    programsError.set('Failed to save program to server.');
    throw error;
  }
}

/**
 * Delete a program by ID
 */
export async function deleteProgram(programId: string): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/programs/${programId}`, {
      method: 'DELETE'
    });

    if (!response.ok) {
      throw new Error('Failed to delete program from server');
    }

    // Update local store
    programs.update(currentPrograms =>
      currentPrograms.filter(p => p.id !== programId)
    );
  } catch (error) {
    console.error('Failed to delete program:', error);
    programsError.set('Failed to delete program from server.');
    throw error;
  }
}

/**
 * Cleanup (for future use when we add real-time listeners)
 */
export function cleanupPrograms(): void {
  programs.set([]);
  programsLoading.set(true);
  programsError.set(null);
}
