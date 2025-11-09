// frontend/src/lib/programs-db.ts
import { browser } from '$app/environment';
import { get } from 'svelte/store';
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
      audioId: p.audioId
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
 * Save a program (create or update) and optionally upload audio
 */
export async function saveProgram(program: Program, audioDataUrl: string | null = null): Promise<void> {
  if (!browser) return;

  try {
    // If there's new audio, upload it first and update the program's audioId
    if (audioDataUrl) {
      try {
        console.log(`[programs-db] Uploading audio for program: ${program.id}`);
        const audioResponse = await fetch(`${API_URL}/audio/${program.id}`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ data_url: audioDataUrl })
        });

        if (!audioResponse.ok) {
          throw new Error(`Failed to upload audio: ${audioResponse.statusText}`);
        }
        const result = await audioResponse.json();
        console.log('[programs-db] Audio uploaded:', result);

        // The backend returns a JSON object with the filename, e.g., { "filename": "..." }
        if (result.filename) {
          program.audioId = result.filename;
        } else {
          throw new Error('Audio upload response did not include a filename.');
        }
      } catch (error) {
        console.error('[programs-db] Error uploading audio:', error);
        programsError.set('Audio upload failed. Program was not saved.');
        // Re-throw to prevent the program from being saved in a bad state
        throw error;
      }
    }

    const isUpdate = get(programs).some(p => p.id === program.id);
    const url = isUpdate ? `${API_URL}/programs/${program.id}` : `${API_URL}/programs`;
    const method = isUpdate ? 'PUT' : 'POST';

    const response = await fetch(url, {
      method,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(program.toJson())
    });

    if (!response.ok) {
      throw new Error(`Failed to save program to server: ${response.statusText}`);
    }

    // Update local store
    programs.update(currentPrograms => {
      const existingIndex = currentPrograms.findIndex(p => p.id === program.id);
      if (existingIndex >= 0) {
        currentPrograms[existingIndex] = program;
        return [...currentPrograms];
      } else {
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
    // Get program to find audio filename
    const program = get(programs).find(p => p.id === programId);

    // Delete audio from backend if it exists
    if (program?.audioId) {
      try {
        await fetch(`${API_URL}/audio/${program.audioId}`, { method: 'DELETE' });
        console.log(`Deleted audio file: ${program.audioId}`);
      } catch (err) {
        console.warn('Failed to delete audio file:', err);
        // Continue with program deletion even if audio deletion fails
      }
    }

    // Delete program from backend
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