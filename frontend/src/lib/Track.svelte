<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import WaveSurfer from 'wavesurfer.js';
	import type { SlotConfig } from '$lib/slots';
	import { toggleSlotMute } from '$lib/audio-db';
	import { slotMuted, type SlotResamplingProgress } from '$lib/store';

	interface Props {
		track: SlotConfig;
		programId: string;
		blobUrl: string | null;
		cachedPeaks: { peaks: Array<number[]>; duration: number } | null;
		resamplingProgress: SlotResamplingProgress | null;
		mainWavesurfer: WaveSurfer | null;
		onRemove: () => void;
	}

	let {
		track,
		programId,
		blobUrl,
		cachedPeaks,
		resamplingProgress,
		mainWavesurfer,
		onRemove,
	}: Props = $props();

	let wavesurfer: WaveSurfer | null = $state(null);
	let isLoaded = $state(false);
	let containerId = $derived(`${track.id}-waveform-${programId.replace(/[^a-zA-Z0-9-_]/g, '-')}`);

	function initWaveSurfer(url: string) {
		if (wavesurfer) {
			wavesurfer.destroy();
		}

		wavesurfer = WaveSurfer.create({
			container: `#${containerId}`,
			waveColor: track.waveformColor,
			progressColor: track.progressColor,
			cursorColor: track.cursorColor,
			barWidth: 2,
			barRadius: 3,
			height: 80,
			interact: false,
		});

		wavesurfer.on('decode', () => {
			isLoaded = true;
		});

		if (cachedPeaks) {
			wavesurfer.load(url, cachedPeaks.peaks, cachedPeaks.duration);
		} else {
			wavesurfer.load(url);
		}
	}

	function syncPlayhead() {
		if (wavesurfer && mainWavesurfer && isLoaded) {
			const currentTime = mainWavesurfer.getCurrentTime();
			const duration = wavesurfer.getDuration();
			if (duration > 0) {
				wavesurfer.seekTo(Math.min(currentTime / duration, 1));
			}
		}
	}

	$effect(() => {
		if (blobUrl && !wavesurfer) {
			setTimeout(() => initWaveSurfer(blobUrl), 0);
		}
	});

	$effect(() => {
		if (mainWavesurfer && wavesurfer && isLoaded) {
			mainWavesurfer.on('timeupdate', syncPlayhead);
			mainWavesurfer.on('seeking', syncPlayhead);
		}
	});

	onDestroy(() => {
		if (wavesurfer) {
			wavesurfer.destroy();
			wavesurfer = null;
		}
	});
</script>

<div class="track" style="--track-color: {track.color}">
	<div class="track-label">
		<button class="mute-btn" class:muted={$slotMuted[track.id]} onclick={() => toggleSlotMute(track.id)}>{track.label}</button>
		{#if resamplingProgress}
			<span class="resampling-inline">
				{resamplingProgress.trackName} • {(resamplingProgress.fromRate / 1000).toFixed(1)}kHz → {(resamplingProgress.toRate / 1000).toFixed(1)}kHz • {Math.round((resamplingProgress.current / resamplingProgress.total) * 100)}%
			</span>
		{/if}
		<button class="btn-remove" onclick={onRemove} title="Remove track">×</button>
	</div>
	<div class="waveform-inner">
		{#if blobUrl && !isLoaded}
			<div class="waveform-skeleton"></div>
		{/if}
		<div id={containerId} class:hidden={!isLoaded}></div>
	</div>
</div>

<style>
	.track {
		margin-top: 0.5rem;
	}

	.track-label {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.25rem 1rem;
		font-size: 0.75rem;
		color: var(--track-color, #888);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.mute-btn {
		background: none;
		border: none;
		padding: 0;
		font-size: inherit;
		font-weight: inherit;
		text-transform: inherit;
		letter-spacing: inherit;
		color: var(--track-color, #888);
		cursor: pointer;
		transition: color 0.15s;
	}

	.mute-btn:hover {
		opacity: 0.7;
	}

	.mute-btn.muted {
		color: #444;
	}

	.resampling-inline {
		color: #666;
		font-size: 0.7rem;
		text-transform: none;
		letter-spacing: normal;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.btn-remove {
		margin-left: auto;
		background: transparent;
		border: none;
		color: rgba(255, 255, 255, 0.5);
		cursor: pointer;
		font-size: 1rem;
		padding: 0 0.25rem;
		line-height: 1;
	}

	.btn-remove:hover {
		color: #ef4444;
	}

	.waveform-inner {
		position: relative;
		min-height: 80px;
	}

	.waveform-skeleton {
		position: absolute;
		top: 0;
		left: 1rem;
		right: 1rem;
		bottom: 0;
		background: linear-gradient(90deg,
			transparent 25%,
			color-mix(in srgb, var(--track-color, #888) 5%, transparent) 50%,
			transparent 75%);
		background-size: 200% 100%;
		animation: shimmer 2.5s infinite ease-in-out;
		border-radius: 8px;
	}

	@keyframes shimmer {
		0% { background-position: 200% 0; }
		100% { background-position: -200% 0; }
	}

	.hidden {
		opacity: 0;
		position: absolute;
		pointer-events: none;
	}

	:global([id*="-waveform-"]) {
		padding: 0 !important;
	}

	:global([id*="-waveform-"] > div) {
		padding: 0.5rem 1rem !important;
	}
</style>
