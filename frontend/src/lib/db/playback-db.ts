import { browser } from '$app/environment';
import { currentlyPlayingProgram } from '$lib/stores/store';
import { API_URL } from '$lib/api';
import type { Program } from '$lib/models/Program';

export async function playProgram(program: Program, startTime: number = 0): Promise<void> {
  if (!browser) return;

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

  try {
    const response = await fetch(`${API_URL}/audio/engine/pause`, {
      method: 'POST'
    });
    if (!response.ok) {
      console.error('Failed to pause program:', response.statusText);
    }
  } catch (err) {
    console.error('Failed to call pause API:', err);
  }
}

export async function resumePlayback(): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/audio/engine/resume`, {
      method: 'POST'
    });
    if (!response.ok) {
      console.error('Failed to resume program:', response.statusText);
    }
  } catch (err) {
    console.error('Failed to call resume API:', err);
  }
}

export async function seekPlayback(positionSecs: number): Promise<void> {
  if (!browser) return;

  try {
    const response = await fetch(`${API_URL}/audio/engine/seek`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ position_secs: positionSecs })
    });
    if (!response.ok) {
      console.error('Failed to seek:', response.statusText);
    }
  } catch (err) {
    console.error('Failed to call seek API:', err);
  }
}

export function clearPlayback(): void {
  if (!browser) return;
  currentlyPlayingProgram.set(null);
}
