<script lang="ts">
	let {
		songName = $bindable(''),
		loopyProTrack = $bindable(''),
		bpm = $bindable<number | null>(null),
		fileName,
		programId,
		isPlaying,
		onPlay,
		onPause,
		onStop,
		onSave,
		onDownload,
		onDelete,
		onBpmChange
	}: {
		songName: string;
		loopyProTrack: string;
		bpm: number | null;
		fileName: string;
		programId: string | null;
		isPlaying: boolean;
		onPlay: () => void;
		onPause: () => void;
		onStop: () => void;
		onSave: () => void;
		onDownload: () => void;
		onDelete: () => void;
		onBpmChange: () => void;
	} = $props();
</script>

<div class="waveform-header">
	{#if isPlaying}
		<button class="btn-program-pause" onclick={onPause}>
			<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="2" width="4" height="12"/><rect x="9" y="2" width="4" height="12"/></svg>
		</button>
	{:else}
		<button class="btn-program-play" onclick={onPlay}>
			<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,1 14,8 3,15"/></svg>
		</button>
	{/if}
	<button class="btn-program-stop" onclick={onStop} title="Stop and reset to start">
		<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><rect x="2" y="2" width="12" height="12" rx="1"/></svg>
	</button>
	<input
		type="text"
		bind:value={songName}
		placeholder="Song name"
		class="song-name-input"
	/>
	<input
		type="text"
		bind:value={loopyProTrack}
		placeholder="Track"
		class="track-input"
		maxlength="2"
	/>
	<input
		type="number"
		bind:value={bpm}
		placeholder="BPM"
		class="bpm-input"
		min="20"
		max="300"
		oninput={onBpmChange}
	/>
	<span class="file-name">{fileName}</span>
	<div class="spacer"></div>
	<button class="btn-save" onclick={onSave} title="Save program">
		<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
			<path d="M12.5 14.5h-9c-.55 0-1-.45-1-1v-11c0-.55.45-1 1-1h6.88l3.62 3.62v8.38c0 .55-.45 1-1 1z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
			<path d="M5.5 1.5v4h5v-4M10.5 14.5v-5h-5v5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
		</svg>
	</button>
	{#if programId}
		<button class="btn-download-program" onclick={onDownload} title="Download program with audio">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
				<path d="M8 1v10M8 11l-3-3M8 11l3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
				<path d="M2 11v2c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2v-2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
			</svg>
		</button>
		<button class="btn-delete-program" onclick={onDelete} title="Delete program">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
				<path d="M2 4h12M5.5 4V2.5h5V4M6.5 7.5v4M9.5 7.5v4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
				<path d="M3.5 4l.5 9.5c0 .55.45 1 1 1h6c.55 0 1-.45 1-1L13 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
			</svg>
		</button>
	{/if}
</div>

<style>
	.waveform-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		background: transparent;
	}

	.spacer {
		flex: 1;
	}

	.btn-program-play,
	.btn-program-pause,
	.btn-program-stop {
		padding: 0.5rem 1rem;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		font-size: 1rem;
		cursor: pointer;
		transition: all 0.2s;
		background-color: transparent;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		box-sizing: border-box;
	}

	.btn-program-play {
		color: #555;
	}

	.btn-program-play:hover {
		background-color: #111;
		color: #22c55e;
		border-color: #222;
	}

	.btn-program-pause {
		color: #555;
	}

	.btn-program-pause:hover {
		background-color: #111;
		color: #f59e0b;
		border-color: #222;
	}

	.btn-program-stop {
		color: #555;
	}

	.btn-program-stop:hover {
		background-color: #111;
		color: #ef4444;
		border-color: #222;
	}

	.song-name-input {
		flex: 0 0 250px;
		background-color: #0a0a0a;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.9rem;
		transition: border-color 0.2s;
	}

	.song-name-input:hover {
		border-color: #333;
	}

	.song-name-input:focus {
		outline: none;
		border-color: #222;
	}

	.song-name-input::placeholder {
		color: #444;
	}

	.track-input {
		width: 45px;
		background-color: #0a0a0a;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 0.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		text-align: center;
		transition: border-color 0.2s;
	}

	.track-input:hover {
		border-color: #333;
	}

	.track-input:focus {
		outline: none;
		border-color: #222;
	}

	.track-input::placeholder {
		color: #444;
	}

	.bpm-input {
		width: 60px;
		background-color: #0a0a0a;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 0.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		text-align: center;
		transition: border-color 0.2s;
		-moz-appearance: textfield;
	}

	.bpm-input::-webkit-outer-spin-button,
	.bpm-input::-webkit-inner-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}

	.bpm-input:hover {
		border-color: #333;
	}

	.bpm-input:focus {
		outline: none;
		border-color: #222;
	}

	.bpm-input::placeholder {
		color: #444;
	}

	.file-name {
		font-size: 0.9rem;
		color: #555;
		font-weight: 400;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 200px;
		flex-shrink: 1;
	}

	.btn-save {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
		padding: 0.5rem 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		height: 36px;
		box-sizing: border-box;
	}

	.btn-save:hover {
		background-color: #111;
		color: #22c55e;
		border-color: #222;
	}

	.btn-save:active {
		background-color: #0f0f0f;
	}

	.btn-download-program {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
		padding: 0.5rem 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		height: 36px;
		box-sizing: border-box;
	}

	.btn-download-program:hover {
		background-color: #111;
		color: #888;
		border-color: #222;
	}

	.btn-download-program:active {
		background-color: #0f0f0f;
	}

	.btn-delete-program {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
		padding: 0.5rem 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		height: 36px;
		box-sizing: border-box;
	}

	.btn-delete-program:hover {
		background-color: #1a1212;
		color: #c44;
		border-color: #331a1a;
	}

	.btn-delete-program:active {
		background-color: #150f0f;
	}
</style>
