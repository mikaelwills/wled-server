<script lang="ts">
	import {
		audioHealthSnapshot,
		audioHealthMonitorVisible,
		toggleAudioHealthMonitor,
		startAudioHealthPolling,
		stopAudioHealthPolling,
		resetAudioHealth
	} from './audio-health-store';

	function formatSamples(n: number): string {
		if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + 'G';
		if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M';
		if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
		return n.toString();
	}

	function formatRate(rate: number): string {
		return (rate / 1000).toFixed(1) + 'kHz';
	}

	function getUnderrunClass(count: number): string {
		if (count === 0) return 'good';
		if (count < 10) return 'warn';
		return 'bad';
	}

	function getBufferClass(size: number): string {
		if (size === 0) return '';
		if (size <= 256) return 'good';
		if (size <= 1024) return '';
		return 'warn';
	}

	function getLateClass(count: number): string {
		if (count === 0) return 'good';
		if (count < 5) return 'warn';
		return 'bad';
	}

	function getJitterClass(actual: number, expected: number): string {
		if (expected === 0) return '';
		const ratio = actual / expected;
		if (ratio < 1.5) return 'good';
		if (ratio < 2.5) return 'warn';
		return 'bad';
	}

	$effect(() => {
		if ($audioHealthMonitorVisible) {
			startAudioHealthPolling(250);
		}
		return () => {
			stopAudioHealthPolling();
		};
	});
</script>

{#if $audioHealthMonitorVisible}
	<div class="audio-health-monitor">
		<div class="header">
			<span class="title">Audio Health</span>
			<div class="header-actions">
				<button class="icon-btn" onclick={() => resetAudioHealth()} title="Reset stats">↻</button>
				<button class="icon-btn" onclick={() => toggleAudioHealthMonitor()} title="Close">×</button>
			</div>
		</div>

		{#if $audioHealthSnapshot}
			<div class="metrics">
				<div class="metric">
					<span class="label">Status</span>
					<span class="value" class:playing={$audioHealthSnapshot.playing}>
						{$audioHealthSnapshot.playing ? '▶ Playing' : '⏸ Idle'}
					</span>
				</div>
				<div class="metric">
					<span class="label">Sample Rate</span>
					<span class="value">{formatRate($audioHealthSnapshot.sample_rate)}</span>
				</div>
				<div class="metric">
					<span class="label">Buffer</span>
					<span class="value {getBufferClass($audioHealthSnapshot.buffer_size)}">
						{$audioHealthSnapshot.buffer_size} ({$audioHealthSnapshot.expected_interval_ms.toFixed(1)}ms)
					</span>
				</div>
				<div class="metric">
					<span class="label">Max Jitter</span>
					<span class="value {getJitterClass($audioHealthSnapshot.max_callback_interval_ms, $audioHealthSnapshot.expected_interval_ms)}">
						{$audioHealthSnapshot.max_callback_interval_ms.toFixed(1)}ms
					</span>
				</div>
				<div class="metric">
					<span class="label">Late CBs</span>
					<span class="value {getLateClass($audioHealthSnapshot.late_callbacks)}">
						{$audioHealthSnapshot.late_callbacks}
					</span>
				</div>
				<div class="metric">
					<span class="label">Underruns</span>
					<span class="value {getUnderrunClass($audioHealthSnapshot.underrun_count)}">
						{$audioHealthSnapshot.underrun_count}
					</span>
				</div>
				<div class="metric">
					<span class="label">Silence</span>
					<span class="value">{formatSamples($audioHealthSnapshot.silence_frames)}</span>
				</div>
				<div class="metric">
					<span class="label">Delivered</span>
					<span class="value">{formatSamples($audioHealthSnapshot.samples_delivered)}</span>
				</div>
			</div>

			{#if $audioHealthSnapshot.playing}
				<div class="position-bar">
					<span class="position-label">Position</span>
					<span class="position-value">{$audioHealthSnapshot.position_secs.toFixed(2)}s</span>
				</div>
			{/if}
		{:else}
			<div class="loading">Loading...</div>
		{/if}
	</div>
{/if}

<style>
	.audio-health-monitor {
		position: fixed;
		bottom: 1rem;
		left: 1rem;
		background: rgba(20, 20, 25, 0.95);
		border: 1px solid #333;
		border-radius: 8px;
		min-width: 240px;
		font-size: 0.85rem;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.5rem 0.75rem;
		background: #1a1a20;
		border-radius: 8px 8px 0 0;
	}

	.title {
		font-weight: 600;
		color: #aaa;
	}

	.header-actions {
		display: flex;
		gap: 0.25rem;
	}

	.icon-btn {
		background: none;
		border: none;
		color: #666;
		font-size: 1.1rem;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
	}

	.icon-btn:hover {
		color: #fff;
		background: #333;
	}

	.metrics {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.5rem;
		padding: 0.75rem;
	}

	.metric {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}

	.label {
		font-size: 0.7rem;
		color: #666;
		text-transform: uppercase;
	}

	.value {
		font-size: 0.95rem;
		font-weight: 600;
		color: #ccc;
		font-family: monospace;
	}

	.value.playing {
		color: #4ade80;
	}

	.value.good {
		color: #4ade80;
	}

	.value.warn {
		color: #fbbf24;
	}

	.value.bad {
		color: #f87171;
	}

	.position-bar {
		display: flex;
		justify-content: space-between;
		padding: 0.5rem 0.75rem;
		border-top: 1px solid #333;
		font-family: monospace;
	}

	.position-label {
		font-size: 0.7rem;
		color: #666;
		text-transform: uppercase;
	}

	.position-value {
		font-size: 0.85rem;
		color: #4ade80;
	}

	.loading {
		padding: 1rem;
		text-align: center;
		color: #666;
	}
</style>
