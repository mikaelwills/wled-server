<script lang="ts">
	import { onDestroy } from 'svelte';
	import {
		timingSnapshot,
		timingMonitorVisible,
		toggleTimingMonitor,
		clearTimingEvents,
		resetTimingMetrics,
		startTimingPolling,
		stopTimingPolling,
		type DriftEvent
	} from './timing-store';

	let expanded = $state(false);

	function formatTime(timestamp: number): string {
		const date = new Date(timestamp);
		return date.toLocaleTimeString('en-US', {
			hour12: false,
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit'
		});
	}

	function getDriftClass(drift_ms: number): string {
		const abs = Math.abs(drift_ms);
		if (abs < 5) return 'good';
		if (abs < 15) return 'warn';
		return 'bad';
	}

	function getPacketSuccessRate(snapshot: typeof $timingSnapshot): number {
		if (!snapshot) return 100;
		const total = snapshot.packets_ok + snapshot.packets_wouldblock + snapshot.packets_err;
		if (total === 0) return 100;
		return (snapshot.packets_ok / total) * 100;
	}

	$effect(() => {
		if ($timingMonitorVisible) {
			startTimingPolling(500);
		}
		return () => {
			stopTimingPolling();
		};
	});
</script>

{#if $timingMonitorVisible}
	<div class="timing-monitor" class:expanded>
		<div class="header" onclick={() => (expanded = !expanded)}>
			<span class="title">Timing Monitor</span>
			<div class="header-actions">
				<button class="icon-btn" onclick={(e) => { e.stopPropagation(); resetTimingMetrics(); }} title="Reset all metrics">↻</button>
				<button class="icon-btn" onclick={(e) => { e.stopPropagation(); toggleTimingMonitor(); }} title="Close">×</button>
			</div>
		</div>

		{#if $timingSnapshot}
			<div class="metrics">
				<div class="metric">
					<span class="label">Cues</span>
					<span class="value">{$timingSnapshot.cue_count}</span>
				</div>
				<div class="metric">
					<span class="label">Avg Drift</span>
					<span class="value {getDriftClass($timingSnapshot.cue_count > 0 ? $timingSnapshot.cue_drift_total_ms / $timingSnapshot.cue_count : 0)}">
						{$timingSnapshot.cue_count > 0 ? ($timingSnapshot.cue_drift_total_ms / $timingSnapshot.cue_count).toFixed(1) : '0.0'}ms
					</span>
				</div>
				<div class="metric">
					<span class="label">Max Drift</span>
					<span class="value {getDriftClass($timingSnapshot.cue_drift_max_ms)}">
						{$timingSnapshot.cue_drift_max_ms.toFixed(1)}ms
					</span>
				</div>
				<div class="metric">
					<span class="label">Threshold</span>
					<span class="value">{$timingSnapshot.drift_threshold_ms}ms</span>
				</div>
			</div>

			{#if expanded}
				<div class="expanded-section">
					<div class="section-header">
						<span>Recent Drift Events ({$timingSnapshot.recent_events.length})</span>
						<button class="clear-btn" onclick={() => clearTimingEvents()}>Clear</button>
					</div>
					<div class="events-list">
						{#if $timingSnapshot.recent_events.length === 0}
							<div class="no-events">No drift events recorded</div>
						{:else}
							{#each $timingSnapshot.recent_events.slice().reverse() as event}
								<div class="event {getDriftClass(event.drift_ms)}">
									<span class="event-time">{formatTime(event.timestamp)}</span>
									<span class="event-source">{event.source}</span>
									<span class="event-label">{event.label}</span>
									<span class="event-drift">{event.drift_ms >= 0 ? '+' : ''}{event.drift_ms.toFixed(1)}ms</span>
								</div>
							{/each}
						{/if}
					</div>
				</div>
			{/if}
		{:else}
			<div class="loading">Loading...</div>
		{/if}
	</div>
{/if}

<style>
	.timing-monitor {
		position: fixed;
		bottom: 1rem;
		right: 1rem;
		background: rgba(20, 20, 25, 0.95);
		border: 1px solid #333;
		border-radius: 8px;
		min-width: 280px;
		max-width: 400px;
		font-size: 0.85rem;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.timing-monitor.expanded {
		max-height: 80vh;
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.5rem 0.75rem;
		background: #1a1a20;
		border-radius: 8px 8px 0 0;
		cursor: pointer;
		user-select: none;
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
		font-size: 1rem;
		font-weight: 600;
		color: #ccc;
		font-family: monospace;
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

	.expanded-section {
		border-top: 1px solid #333;
		padding: 0.5rem;
	}

	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.25rem 0.25rem 0.5rem;
		font-size: 0.75rem;
		color: #888;
	}

	.clear-btn {
		background: #333;
		border: none;
		color: #888;
		font-size: 0.7rem;
		padding: 0.2rem 0.5rem;
		border-radius: 4px;
		cursor: pointer;
	}

	.clear-btn:hover {
		background: #444;
		color: #fff;
	}

	.events-list {
		max-height: 200px;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.no-events {
		text-align: center;
		color: #555;
		padding: 1rem;
		font-style: italic;
	}

	.event {
		display: grid;
		grid-template-columns: auto auto 1fr auto;
		gap: 0.5rem;
		padding: 0.3rem 0.5rem;
		background: #1a1a1a;
		border-radius: 4px;
		font-family: monospace;
		font-size: 0.75rem;
		align-items: center;
	}

	.event.good {
		border-left: 2px solid #4ade80;
	}

	.event.warn {
		border-left: 2px solid #fbbf24;
	}

	.event.bad {
		border-left: 2px solid #f87171;
	}

	.event-time {
		color: #666;
	}

	.event-source {
		color: #888;
		background: #252525;
		padding: 0.1rem 0.3rem;
		border-radius: 2px;
		font-size: 0.65rem;
	}

	.event-label {
		color: #aaa;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.event-drift {
		color: #ccc;
		font-weight: 600;
	}

	.loading {
		padding: 1rem;
		text-align: center;
		color: #666;
	}
</style>
