// frontend/src/lib/programs-db.ts
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { programs, programsLoading, programsError } from '$lib/stores/store';
import { Program } from '$lib/models/Program';
import { API_URL } from '$lib/api';
import { removeAudioForProgram, removeGuideAudioForProgram } from '$lib/db/audio-db';

/**
 * Initialize programs from API
 */
export async function initPrograms(): Promise<void> {
  if (!browser) return;

  programsLoading.set(true);
  programsError.set(null);

  const url = `${API_URL}/programs`;
  console.log(`[programs-db] Fetching from: ${url}`);

  try {
    const response = await fetch(url);
    console.log(`[programs-db] Response status: ${response.status} ${response.statusText}`);

    if (!response.ok) {
      throw new Error(`Failed to load programs from server: ${response.status}`);
    }

    const data = await response.json();
    console.log('[programs-db] RAW API response:', JSON.stringify(data));
    console.log('[programs-db] API returned programs:', data.map((p: any) => ({
      id: p.id,
      audioDataLength: p.audio_data?.length || 0
    })));

    const loadedPrograms = Array.isArray(data)
      ? data.map((p: any) => Program.fromJson(p)).filter((p): p is Program => p !== null)
      : [];

    // Sort by displayOrder ascending (for drag-and-drop reordering)
    loadedPrograms.sort((a, b) => a.displayOrder - b.displayOrder);

    console.log('[programs-db] Loaded programs:', loadedPrograms.map(p => ({
      id: p.id,
      audioId: p.audioId
    })));

    programs.set(loadedPrograms);
    programsLoading.set(false);
  } catch (error) {
    console.error('[programs-db] FETCH ERROR:', error);
    const errorMsg = `Failed to load programs: ${error}`;
    console.error('[programs-db]', errorMsg);
    programsError.set(errorMsg);
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

        if (result.audio_file) {
          program.audioId = result.audio_file;
        } else {
          throw new Error('Audio upload response did not include audio_file.');
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
 * Update an existing program (without audio upload)
 */
export async function updateProgram(program: Program): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/programs/${program.id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(program.toJson())
    });

    if (!response.ok) {
      throw new Error(`Failed to update program: ${response.statusText}`);
    }

    // Update local store
    programs.update(currentPrograms => {
      const existingIndex = currentPrograms.findIndex(p => p.id === program.id);
      if (existingIndex >= 0) {
        currentPrograms[existingIndex] = program;
        return [...currentPrograms];
      }
      return currentPrograms;
    });
  } catch (error) {
    console.error('Failed to update program:', error);
    programsError.set('Failed to update program on server.');
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
      }
    }

    // Delete guide audio from backend if it exists
    if (program?.guideAudioId) {
      try {
        await fetch(`${API_URL}/audio/${program.guideAudioId}`, { method: 'DELETE' });
        console.log(`Deleted guide audio file: ${program.guideAudioId}`);
      } catch (err) {
        console.warn('Failed to delete guide audio file:', err);
      }
    }

    // Delete program from backend (may not exist if never saved)
    console.log(`[programs-db] Deleting program JSON: ${programId}`);
    const response = await fetch(`${API_URL}/programs/${programId}`, {
      method: 'DELETE'
    });
    console.log(`[programs-db] Delete response: ${response.status}`);

    if (!response.ok && response.status !== 404) {
      throw new Error(`Failed to delete program from server: ${response.status}`);
    }

    // Update local store (always, even if backend had no file)
    programs.update(currentPrograms => {
      console.log(`[programs-db] Removing from store, before: ${currentPrograms.length} programs`);
      const filtered = currentPrograms.filter(p => p.id !== programId);
      console.log(`[programs-db] After filter: ${filtered.length} programs`);
      return filtered;
    });

    // Clean up cached audio blob URLs
    removeAudioForProgram(programId);
    removeGuideAudioForProgram(programId);
    console.log(`[programs-db] Program deleted: ${programId}`);
  } catch (error) {
    console.error('Failed to delete program:', error);
    programsError.set('Failed to delete program from server.');
    throw error;
  }
}

/**
 * Reorder programs and persist to backend
 */
export async function reorderPrograms(reorderedPrograms: Program[]): Promise<void> {
  if (!browser) return;

  try {
    const updatedPrograms = reorderedPrograms.map((program, index) => {
      const instance = Program.fromJson(program as any) || program;
      instance.displayOrder = index;
      return instance;
    });

    programs.set(updatedPrograms);

    await Promise.all(
      updatedPrograms.map(program => {
        const jsonBody = typeof program.toJson === 'function'
          ? program.toJson()
          : {
              id: program.id,
              song_name: program.songName,
              loopy_pro_track: program.loopyProTrack,
              file_name: program.fileName,
              audio_file: program.audioId,
              cues: program.cues.map((c: any) => typeof c.toJson === 'function' ? c.toJson() : c),
              created_at: program.createdAt,
              default_target_board: program.defaultTargetBoard,
              next_program_id: program.nextProgramId,
              transition_type: program.transitionType,
              transition_duration: program.transitionDuration,
              audio_duration: program.audioDuration,
              display_order: program.displayOrder,
              bpm: program.bpm,
              grid_offset: program.gridOffset,
            };

        return fetch(`${API_URL}/programs/${program.id}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(jsonBody)
        });
      })
    );

    console.log('[programs-db] Programs reordered successfully');
  } catch (error) {
    console.error('Failed to reorder programs:', error);
    programsError.set('Failed to save new program order.');
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