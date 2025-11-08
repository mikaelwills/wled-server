// frontend/src/lib/models/Program.ts
import { Cue, type CueData } from './Cue';

export interface ProgramData {
  id: string;
  songName: string;
  loopyProTrack: string;
  fileName: string;
  audioData: string;
  cues: Cue[];
  createdAt: string;
  defaultTargetBoard?: string;
}

export class Program implements ProgramData {
  id: string;
  songName: string;
  loopyProTrack: string;
  fileName: string;
  audioData: string;
  cues: Cue[];
  createdAt: string;
  defaultTargetBoard?: string;

  private constructor(data: ProgramData) {
    this.id = data.id;
    this.songName = data.songName;
    this.loopyProTrack = data.loopyProTrack;
    this.fileName = data.fileName;
    this.audioData = data.audioData;
    this.cues = data.cues;
    this.createdAt = data.createdAt;
    this.defaultTargetBoard = data.defaultTargetBoard;
  }

  /**
   * Factory method - validates and constructs Program from JSON
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

    const audioData = data.audioData || data.audio_data || '';
    console.log(`[Program.fromJson] id=${data.id}, audioData length=${audioData.length}`);

    return new Program({
      id: data.id,
      songName: songName,
      loopyProTrack: data.loopyProTrack || data.loopy_pro_track || '',
      fileName: data.fileName || data.file_name || 'audio.wav',
      audioData,
      cues,
      createdAt: data.createdAt || data.created_at || new Date().toISOString(),
      defaultTargetBoard: data.defaultTargetBoard || data.default_target_board,
    });
  }

  /**
   * Convert to plain object for storage (snake_case for Rust API)
   */
  toJson(): Record<string, any> {
    return {
      id: this.id,
      song_name: this.songName,
      loopy_pro_track: this.loopyProTrack,
      file_name: this.fileName,
      audio_data: this.audioData,
      cues: this.cues.map(c => c.toJson()),
      created_at: this.createdAt,
      default_target_board: this.defaultTargetBoard,
    };
  }
}
