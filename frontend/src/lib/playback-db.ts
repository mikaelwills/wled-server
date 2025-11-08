// frontend/src/lib/playback-db.ts
import { browser } from '$app/environment';
import { currentlyPlayingProgram, activeTimeouts } from './store';
import { API_URL } from './api';
import { setBoardBrightness, setBoardColor, setBoardEffect, setBoardPreset, setBoardPower } from './boards-db';
import type { Program } from './models/Program';

/**
 * Extract all unique board IDs from a program's cues
 */
function getUniqueBoardIds(program: Program): string[] {
  const boardIds = new Set<string>();
  program.cues.forEach(cue => {
    cue.boards.forEach(boardId => boardIds.add(boardId));
  });
  return Array.from(boardIds);
}

/**
 * Play a program - FAST, non-blocking, schedules cues with minimal latency
 * @param program - The program to play
 * @param startTime - Optional start time in seconds (default: 0)
 * @param audioStartTime - Performance.now() timestamp when audio actually started
 */
export function playProgram(program: Program, startTime: number = 0, audioStartTime?: number): void {
  if (!browser) return;

  const playbackStartTime = audioStartTime || performance.now();

  console.log('⚡ playProgram FAST path:', {
    id: program.id,
    cueCount: program.cues?.length,
    startTime,
    audioStartTime: playbackStartTime
  });

  // FAST: Clear old playback synchronously (no await)
  clearPlayback();

  // FAST: Set as currently playing immediately
  currentlyPlayingProgram.set(program);

  // FIRE-AND-FORGET: Send OSC command in background (non-blocking)
  if (program.loopyProTrack) {
    fetch(`${API_URL}/osc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        address: `/PlayStop/${program.loopyProTrack.padStart(2, '0')}`
      })
    }).catch(err => {
      console.error('Failed to send OSC:', err);
    });
  }

  // FAST: Sort and filter cues
  const sortedCues = [...program.cues]
    .sort((a, b) => a.time - b.time)
    .filter(cue => cue.time >= startTime);

  console.log(`📍 Scheduling ${sortedCues.length} cues from ${startTime}s`);

  // Schedule all cues relative to AUDIO START TIME
  const newTimeouts: number[] = [];

  sortedCues.forEach((cue) => {
    // Calculate delay from when audio ACTUALLY started
    const cueTimeMs = (cue.time - startTime) * 1000;
    const elapsedSinceAudioStart = performance.now() - playbackStartTime;
    const delay = Math.max(0, cueTimeMs - elapsedSinceAudioStart);

    const timeoutId = window.setTimeout(() => {
      const actualTime = (performance.now() - playbackStartTime) / 1000;
      const timestamp = performance.now().toFixed(3);
      console.log(`🎯 [${timestamp}ms] Cue ${cue.label} @ ${cue.time}s (actual: ${actualTime.toFixed(3)}s, drift: ${((actualTime - cue.time) * 1000).toFixed(0)}ms)`);

      // Send commands to all boards (fire-and-forget for minimal latency)
      cue.boards.forEach(boardId => {
        sendCueCommands(cue, boardId).catch(err => {
          console.error(`Failed to send commands to board ${boardId}:`, err);
        });
      });
    }, delay);

    newTimeouts.push(timeoutId);
  });

  // Store all timeout IDs
  activeTimeouts.set(newTimeouts);
}

/**
 * Clear playback state synchronously (FAST - no network calls)
 */
export function clearPlayback(): void {
  if (!browser) return;

  // Clear all scheduled timeouts
  let currentTimeouts: number[] = [];
  const unsubTimeouts = activeTimeouts.subscribe(t => {
    currentTimeouts = t;
  });
  unsubTimeouts();

  currentTimeouts.forEach(timeoutId => clearTimeout(timeoutId));

  // Reset stores immediately
  activeTimeouts.set([]);
  currentlyPlayingProgram.set(null);
}

/**
 * Dim all boards in a program to 0 (fade to black)
 */
export async function dimProgramBoards(program: Program): Promise<void> {
  if (!browser) return;

  const boardIds = getUniqueBoardIds(program);
  console.log(`Dimming ${boardIds.length} boards to 0:`, boardIds);

  // Dim all boards in parallel for faster response
  await Promise.all(
    boardIds.map(async (boardId) => {
      try {
        await setBoardBrightness(boardId, 0);
        console.log(`✓ Dimmed board ${boardId} to 0`);
      } catch (err) {
        console.error(`Failed to dim board ${boardId}:`, err);
      }
    })
  );
}

/**
 * Stop playback - clears all scheduled timeouts and dims all boards
 */
export async function stopPlayback(): Promise<void> {
  if (!browser) return;

  // Get currently playing program BEFORE clearing
  let program: Program | null = null;
  const unsubProgram = currentlyPlayingProgram.subscribe(p => {
    program = p;
  });
  unsubProgram();

  // FAST: Clear playback state synchronously
  clearPlayback();

  // Dim all boards to 0 (fade to black)
  if (program) {
    await dimProgramBoards(program);
  }
}

/**
 * Send LED commands for a specific cue to a board
 * Uses boards-db functions which handle both groups and individual boards
 */
async function sendCueCommands(cue: any, boardId: string): Promise<void> {
  if (!browser) return;

  // Check action type (default to 'preset' for backward compatibility)
  const action = cue.action || 'preset';

  switch (action) {
    case 'on':
      // Lightweight: Send power on command with instant transition
      await setBoardPower(boardId, true);
      break;

    case 'off':
      // Lightweight: Send power off command with instant transition
      await setBoardPower(boardId, false);
      break;

    case 'preset':
    default:
      // Heavy: Load preset (sets the "look")
      if (cue.preset > 0) {
        await setBoardPreset(boardId, cue.preset);
      } else {
        // Set color
        const rgb = hexToRgb(cue.color);
        await setBoardColor(boardId, rgb.r, rgb.g, rgb.b);

        // Set effect
        await setBoardEffect(boardId, cue.effect);
      }

      // Set brightness
      await setBoardBrightness(boardId, cue.brightness);
      break;
  }
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
