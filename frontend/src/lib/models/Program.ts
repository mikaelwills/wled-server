// frontend/src/lib/models/Program.ts
import { Cue, type CueData } from './Cue';

export type TransitionType = 'immediate' | 'blackout' | 'hold';

export interface ProgramData {
  id: string;
  songName: string;
  loopyProTrack: string;
  fileName: string;
  audioId: string;
  audioData?: string; // Legacy field - for old programs with embedded audio
  cues: Cue[];
  createdAt: string;
  defaultTargetBoard?: string;
  // Auto-play chain fields
  nextProgramId?: string;
  transitionType: TransitionType;
  transitionDuration: number; // milliseconds
}

export class Program implements ProgramData {
  id: string;
  songName: string;
  loopyProTrack: string;
  fileName: string;
  audioId: string;
  audioData?: string; // Legacy field - kept for backward compatibility
  cues: Cue[];
  createdAt: string;
  defaultTargetBoard?: string;
  // Auto-play chain fields
  nextProgramId?: string;
  transitionType: TransitionType;
  transitionDuration: number;

  private constructor(data: ProgramData) {
    this.id = data.id;
    this.songName = data.songName;
    this.loopyProTrack = data.loopyProTrack;
    this.fileName = data.fileName;
    this.audioId = data.audioId;
    this.audioData = data.audioData; // Preserve if present
    this.cues = data.cues;
    this.createdAt = data.createdAt;
    this.defaultTargetBoard = data.defaultTargetBoard;
    this.nextProgramId = data.nextProgramId;
    this.transitionType = data.transitionType || 'immediate';
    this.transitionDuration = data.transitionDuration || 0;
  }

  /**
   * Factory method - validates and constructs Program from JSON
   * Supports both new (audioId) and legacy (audioData) formats
   */
  static fromJson(data: Record<string, any>): Program | null {
    const songName = data.songName || data.song_name;
    if (!data || !data.id || !songName) {
      console.error('Invalid program data: missing id or songName', data);
      return null;
    }

    // Parse cues using Cue factory
    const cues = Array.isArray(data.cues)
      ? data.cues.map((c: any) => Cue.fromJson(c)).filter((c): c is Cue => c !== null)
      : [];

    // Check for audioId (new format)
    let audioId = data.audioId || data.audio_id || data.audio_file || '';

    // Check for legacy audioData field (keep it if present, don't migrate yet)
    const audioData = data.audioData || data.audio_data;

    if (audioData && audioData.length > 0) {
      console.log(`[Program.fromJson] Legacy audio detected for ${data.id}, size: ${audioData.length}`);
      // Don't migrate during load - will migrate on next save
    } else if (audioId) {
      console.log(`[Program.fromJson] Using audioId: ${audioId}`);
    }

    return new Program({
      id: data.id,
      songName: songName,
      loopyProTrack: data.loopyProTrack || data.loopy_pro_track || '',
      fileName: data.fileName || data.file_name || 'audio.wav',
      audioId,
      audioData, // Preserve legacy audio if present
      cues,
      createdAt: data.createdAt || data.created_at || new Date().toISOString(),
      defaultTargetBoard: data.defaultTargetBoard || data.default_target_board,
      nextProgramId: data.nextProgramId || data.next_program_id,
      transitionType: (data.transitionType || data.transition_type || 'immediate') as TransitionType,
      transitionDuration: data.transitionDuration || data.transition_duration || 0,
    });
  }

  /**
   * Convert to plain object for storage (snake_case for Rust API)
   * Note: Does NOT include audioData - audio stored separately in localStorage
   */
  toJson(): Record<string, any> {
    return {
      id: this.id,
      song_name: this.songName,
      loopy_pro_track: this.loopyProTrack,
      file_name: this.fileName,
      audio_file: this.audioId,
      cues: this.cues.map(c => c.toJson()),
      created_at: this.createdAt,
      default_target_board: this.defaultTargetBoard,
      next_program_id: this.nextProgramId,
      transition_type: this.transitionType,
      transition_duration: this.transitionDuration,
    };
  }
}
