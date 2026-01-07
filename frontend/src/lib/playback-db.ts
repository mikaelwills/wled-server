import { browser } from '$app/environment';
import { currentlyPlayingProgram } from './store';
import { API_URL } from './api';
import type { Program } from './models/Program';

export async function playProgram(program: Program, startTime: number = 0): Promise<void> {
  if (!browser) return;

  console.log('▶️ Playing program:', program.id, '@ start:', startTime);

  currentlyPlayingProgram.set(program);

  try {
    const response = await fetch(`${API_URL}/programs/${program.id}/play?start=${startTime}`, {
      method: 'POST'
    });
    if (!response.ok) {
      console.error('Failed to start program:', response.statusText);
    }
  } catch (err) {
    console.error('Failed to call play API:', err);
  }
}

export async function stopPlayback(): Promise<void> {
  if (!browser) return;

  console.log('⏹️ Stopping playback');

  currentlyPlayingProgram.set(null);

  try {
    const response = await fetch(`${API_URL}/programs/stop`, {
      method: 'POST'
    });
    if (!response.ok) {
      console.error('Failed to stop program:', response.statusText);
    }
  } catch (err) {
    console.error('Failed to call stop API:', err);
  }
}

export async function pausePlayback(): Promise<void> {
  if (!browser) return;
  console.log('⏸️ Pausing playback');
  currentlyPlayingProgram.set(null);

  try {
    const response = await fetch(`${API_URL}/programs/stop`, {
      method: 'POST'
    });
    if (!response.ok) {
      console.error('Failed to pause program:', response.statusText);
    }
  } catch (err) {
    console.error('Failed to call pause API:', err);
  }
}

export function clearPlayback(): void {
  if (!browser) return;
  currentlyPlayingProgram.set(null);
}
