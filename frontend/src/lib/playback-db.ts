// frontend/src/lib/playback-db.ts
import { browser } from '$app/environment';
import { currentlyPlayingProgramId, activeTimeouts } from './store';
import { API_URL } from './api';
import type { Program } from './models/Program';

/**
 * Play a program - stops any currently playing program and schedules cues
 * @param program - The program to play
 * @param startTime - Optional start time in seconds (default: 0)
 */
export async function playProgram(program: Program, startTime: number = 0): Promise<void> {
  if (!browser) return;

  console.log('playProgram called with:', {
    id: program.id,
    cueCount: program.cues?.length,
    cues: program.cues,
    startTime
  });

  // Stop any currently playing program first
  stopPlayback();

  // Set as currently playing
  currentlyPlayingProgramId.set(program.id);

  // Sort cues by time and filter out cues that have already passed
  const sortedCues = [...program.cues]
    .sort((a, b) => a.time - b.time)
    .filter(cue => cue.time >= startTime);

  console.log(`Sorted cues (${sortedCues.length} remaining from ${startTime}s):`, sortedCues);

  // Send OSC to start Loopy Pro track
  if (program.loopyProTrack) {
    try {
      await fetch(`${API_URL}/osc`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: `/PlayStop/${program.loopyProTrack.padStart(2, '0')}`
        })
      });
    } catch (err) {
      console.error('Failed to send OSC:', err);
    }
  }

  // Schedule all cues (adjusted for start time)
  const newTimeouts: number[] = [];

  sortedCues.forEach((cue) => {
    // Calculate delay from current position (cue.time - startTime)
    const delay = (cue.time - startTime) * 1000; // Convert to milliseconds

    const timeoutId = window.setTimeout(async () => {
      console.log(`Triggering cue at ${cue.time}s: ${cue.label}`);

      // Send commands to all boards in this cue
      for (const boardId of cue.boards) {
        try {
          await sendCueCommands(cue, boardId);
        } catch (err) {
          console.error(`Failed to send commands to board ${boardId}:`, err);
        }
      }
    }, delay);

    newTimeouts.push(timeoutId);
  });

  // Auto-stop after last cue + 1 second (adjusted for start time)
  if (sortedCues.length > 0) {
    const lastCueTime = sortedCues[sortedCues.length - 1].time;
    const stopDelay = (lastCueTime - startTime + 1) * 1000;
    const stopTimeoutId = window.setTimeout(() => {
      currentlyPlayingProgramId.set(null);
      activeTimeouts.set([]);
    }, stopDelay);
    newTimeouts.push(stopTimeoutId);
  }

  // Store all timeout IDs
  activeTimeouts.set(newTimeouts);
}

/**
 * Stop playback - clears all scheduled timeouts
 */
export function stopPlayback(): void {
  if (!browser) return;

  // Clear all scheduled timeouts
  let currentTimeouts: number[] = [];
  const unsubscribe = activeTimeouts.subscribe(t => {
    currentTimeouts = t;
  });
  unsubscribe();

  currentTimeouts.forEach(timeoutId => clearTimeout(timeoutId));

  // Reset stores
  activeTimeouts.set([]);
  currentlyPlayingProgramId.set(null);
}

/**
 * Send LED commands for a specific cue to a board
 */
async function sendCueCommands(cue: any, boardId: string): Promise<void> {
  if (!browser) return;

  // Set preset if specified
  if (cue.preset > 0) {
    await fetch(`${API_URL}/board/${boardId}/preset`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ preset: cue.preset, transition: cue.transition })
    });
  } else {
    // Set color
    const rgb = hexToRgb(cue.color);
    await fetch(`${API_URL}/board/${boardId}/color`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ r: rgb.r, g: rgb.g, b: rgb.b, transition: cue.transition })
    });

    // Set effect
    await fetch(`${API_URL}/board/${boardId}/effect`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ effect: cue.effect, transition: cue.transition })
    });
  }

  // Set brightness
  await fetch(`${API_URL}/board/${boardId}/brightness`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ brightness: cue.brightness, transition: cue.transition })
  });
}

/**
 * Convert hex color to RGB
 */
function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return result
    ? {
        r: parseInt(result[1], 16),
        g: parseInt(result[2], 16),
        b: parseInt(result[3], 16)
      }
    : { r: 0, g: 0, b: 0 };
}
