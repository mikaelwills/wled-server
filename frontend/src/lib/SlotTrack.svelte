<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import WaveSurfer from 'wavesurfer.js';
	import type { SlotConfig } from '$lib/slots';
	import type { ResamplingProgress } from '$lib/sse';

	interface Props {
		slot: SlotConfig;
		programId: string;
		blobUrl: string | null;
		cachedPeaks: { peaks: Array<number[]>; duration: number } | null;
		resamplingProgress: ResamplingProgress | null;
		mainWavesurfer: WaveSurfer | null;
		onRemove: () => void;
		onRoutingClick: () => void;
	}

	let {
		slot,
		programId,
		blobUrl,
		cachedPeaks,
		resamplingProgress,
		mainWavesurfer,
		onRemove,
		onRoutingClick,
	}: Props = $props();

	let wavesurfer: WaveSurfer | null = $state(null);
	let isLoaded = $state(false);
	let containerId = $derived(`${slot.id}-waveform-${programId.replace(/[^a-zA-Z0-9-_]/g, '-')}`);

	function initWaveSurfer(url: string) {
		if (wavesurfer) {
			wavesurfer.destroy();
		}

		wavesurfer = WaveSurfer.create({
			container: `#${containerId}`,
			waveColor: slot.waveformColor,
			progressColor: slot.progressColor,
			cursorColor: slot.cursorColor,
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

<div class="slot-track" style="--slot-color: {slot.color}">
	<div class="slot-label">
		<span>{slot.label}</span>
		{#if resamplingProgress}
			<span class="resampling-inline">
				{resamplingProgress.trackName.replace(/^guide:/, '')} • {(resamplingProgress.fromRate / 1000).toFixed(1)}kHz → {(resamplingProgress.toRate / 1000).toFixed(1)}kHz • {Math.round((resamplingProgress.current / resamplingProgress.total) * 100)}%
			</span>
		{/if}
		<div class="slot-label-actions">
			<button class="routing-btn" onclick={onRoutingClick} title="Channel routing">
				<svg width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
					<path d="M2 4h4M10 4h4M2 8h4M10 8h4M2 12h4M10 12h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
					<circle cx="8" cy="4" r="1.5" stroke="currentColor" stroke-width="1.5"/>
					<circle cx="8" cy="8" r="1.5" stroke="currentColor" stroke-width="1.5"/>
					<circle cx="8" cy="12" r="1.5" stroke="currentColor" stroke-width="1.5"/>
				</svg>
			</button>
			<button class="btn-remove" onclick={onRemove} title="Remove track">×</button>
		</div>
	</div>
	<div class="waveform-inner">
		{#if blobUrl && !isLoaded}
			<div class="waveform-skeleton"></div>
		{/if}
		<div id={containerId} class:hidden={!isLoaded}></div>
	</div>
</div>

<style>
	.slot-track {
		margin-top: 0.5rem;
	}

	.slot-label {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.25rem 1rem;
		font-size: 0.75rem;
		color: var(--slot-color, #888);
		text-transform: uppercase;
		letter-spacing: 0.05em;
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

	.slot-label-actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}

	.routing-btn {
		background: transparent;
		border: none;
		color: color-mix(in srgb, var(--slot-color, #888) 50%, transparent);
		cursor: pointer;
		padding: 0.25rem;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 4px;
		transition: all 0.15s;
	}

	.routing-btn:hover {
		color: var(--slot-color, #888);
		background: color-mix(in srgb, var(--slot-color, #888) 10%, transparent);
	}

	.btn-remove {
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
			color-mix(in srgb, var(--slot-color, #888) 5%, transparent) 50%,
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
