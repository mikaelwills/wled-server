<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		historyStore,
		historyLoading,
		historyError,
		fetchHistory,
		deleteSession,
		clearHistory,
		formatDuration,
		formatTimestamp,
		getDriftStatus,
		type PlaybackSession
	} from '$lib/history-store';
	import { timingSnapshot, fetchTimingSnapshot } from '$lib/timing-store';

	let expandedId: string | null = $state(null);
	let pollInterval: ReturnType<typeof setInterval> | null = null;

	function toggleExpand(id: string) {
		expandedId = expandedId === id ? null : id;
	}

	async function handleDelete(id: string, e: Event) {
		e.stopPropagation();
		if (confirm('Delete this session?')) {
			await deleteSession(id);
		}
	}

	async function handleClearAll() {
		if (confirm('Clear all history? This cannot be undone.')) {
			const count = await clearHistory();
			if (count > 0) {
				console.log(`Cleared ${count} sessions`);
			}
		}
	}

	function startPolling() {
		fetchHistory();
		fetchTimingSnapshot();
		pollInterval = setInterval(() => {
			fetchHistory();
			if ($historyStore.current) {
				fetchTimingSnapshot();
			}
		}, 2000);
	}

	function stopPolling() {
		if (pollInterval) {
			clearInterval(pollInterval);
			pollInterval = null;
		}
	}

	onMount(() => {
		startPolling();
	});

	onDestroy(() => {
		stopPolling();
	});

	function getLiveDuration(session: PlaybackSession): string {
		if (session.ended_at) return formatDuration(session.duration_ms);
		const now = Date.now();
		return formatDuration(now - session.started_at);
	}

	function getLiveCueCount(): number {
		return $timingSnapshot?.cue_count ?? 0;
	}

	function getLiveCuesDrifted(): number {
		return $timingSnapshot?.cues_drifted ?? 0;
	}

	function getLiveDriftAvg(): number {
		const snap = $timingSnapshot;
		if (!snap || snap.cue_count === 0) return 0;
		return snap.cue_drift_total_ms / snap.cue_count;
	}
</script>

<div class="history-page">
	<div class="history-container">
		<div class="header">
			<h1>Playback History</h1>
			{#if $historyStore.sessions.length > 0}
				<button class="clear-button" onclick={handleClearAll}>Clear All</button>
			{/if}
		</div>

		{#if $historyLoading && $historyStore.sessions.length === 0}
			<p class="loading">Loading history...</p>
		{:else if $historyError}
			<p class="error">{$historyError}</p>
		{:else}
			{#if $historyStore.current}
				<div class="current-session">
					<div class="session-badge live">LIVE</div>
					<div class="session-header">
						<div class="session-title">{$historyStore.current.program_name}</div>
						<div class="session-time">{formatTimestamp($historyStore.current.started_at)}</div>
					</div>
					<div class="session-stats live-stats">
						<div class="stat">
							<span class="stat-value">{getLiveDuration($historyStore.current)}</span>
							<span class="stat-label">Duration</span>
						</div>
						<div class="stat">
							<span class="stat-value">{getLiveCueCount()}</span>
							<span class="stat-label">Cues Fired</span>
						</div>
						<div class="stat">
							<span class="stat-value" class:drift-bad={getLiveCuesDrifted() > 0}>{getLiveCuesDrifted()}</span>
							<span class="stat-label">Drifted</span>
						</div>
						<div class="stat">
							<span class="stat-value drift-{getDriftStatus(getLiveDriftAvg())}">{getLiveDriftAvg().toFixed(1)}ms</span>
							<span class="stat-label">Avg Drift</span>
						</div>
					</div>
				</div>
			{/if}

			{#if $historyStore.sessions.length === 0 && !$historyStore.current}
				<p class="empty">No playback history yet</p>
			{:else}
				<div class="sessions-list">
					{#each $historyStore.sessions as session (session.id)}
						<div
							class="session-card"
							class:expanded={expandedId === session.id}
							onclick={() => toggleExpand(session.id)}
						>
							<div class="session-row">
								<div class="session-info">
									<span class="session-title">{session.program_name}</span>
									<span class="session-time">{formatTimestamp(session.started_at)}</span>
									<span class="session-duration">{formatDuration(session.duration_ms)}</span>
								</div>
								<div class="session-stats-inline">
									<span class="inline-stat">{session.cue_count} cues</span>
									<span class="inline-stat" class:drift-bad={session.cues_drifted > 0}>{session.cues_drifted} drifted</span>
									<span class="inline-stat drift-{getDriftStatus(session.cue_drift_avg_ms)}">{session.cue_drift_avg_ms.toFixed(1)}ms</span>
								</div>
								<button class="delete-btn" onclick={(e) => handleDelete(session.id, e)}>×</button>
							</div>

							{#if expandedId === session.id}
								<div class="session-details">
									<div class="detail-row">
										<span class="detail-label">Frames</span>
										<span class="detail-value">{session.frame_count.toLocaleString()}</span>
									</div>
									<div class="detail-row">
										<span class="detail-label">Frame Avg</span>
										<span class="detail-value">{session.frame_avg_ms.toFixed(2)}ms</span>
									</div>
									<div class="detail-row">
										<span class="detail-label">Packets OK</span>
										<span class="detail-value">{session.packets_ok.toLocaleString()}</span>
									</div>
									{#if session.packets_wouldblock > 0}
										<div class="detail-row warning">
											<span class="detail-label">Would Block</span>
											<span class="detail-value">{session.packets_wouldblock}</span>
										</div>
									{/if}
									{#if session.packets_err > 0}
										<div class="detail-row error">
											<span class="detail-label">Errors</span>
											<span class="detail-value">{session.packets_err}</span>
										</div>
									{/if}
									<div class="detail-row">
										<span class="detail-label">Status</span>
										<span class="detail-value status-{session.completed ? 'complete' : 'stopped'}">
											{session.completed ? 'Completed' : 'Stopped'}
										</span>
									</div>
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
</div>

<style>
	.history-page {
		min-height: calc(100vh - 60px);
		padding: 1rem;
	}

	.history-container {
		max-width: 600px;
		margin: 0 auto;
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1.5rem;
	}

	h1 {
		font-size: 1.25rem;
		margin: 0;
		color: #e5e5e5;
		font-weight: 600;
	}

	.clear-button {
		padding: 0.5rem 1rem;
		background: transparent;
		color: #666;
		border: 1px solid #333;
		border-radius: 6px;
		font-size: 0.75rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	.clear-button:hover {
		color: #ff6b6b;
		border-color: #ff6b6b;
	}

	.loading, .empty {
		text-align: center;
		color: #666;
		padding: 3rem 1rem;
	}

	.error {
		text-align: center;
		color: #ff6b6b;
		padding: 2rem 1rem;
	}

	.current-session {
		background: linear-gradient(135deg, rgba(168, 85, 247, 0.1) 0%, rgba(168, 85, 247, 0.05) 100%);
		border: 1px solid rgba(168, 85, 247, 0.3);
		border-radius: 12px;
		padding: 1rem;
		margin-bottom: 1.5rem;
		position: relative;
	}

	.session-badge {
		position: absolute;
		top: -8px;
		right: 12px;
		padding: 0.25rem 0.75rem;
		border-radius: 4px;
		font-size: 0.625rem;
		font-weight: 700;
		letter-spacing: 0.1em;
	}

	.session-badge.live {
		background: #a855f7;
		color: white;
		animation: pulse 2s infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.7; }
	}

	.sessions-list {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.session-card {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 8px;
		padding: 0.75rem 1rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	.session-card:hover {
		border-color: rgba(255, 255, 255, 0.08);
	}

	.session-card.expanded {
		border-color: rgba(168, 85, 247, 0.3);
	}

	.session-row {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.session-info {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex: 1;
		min-width: 0;
	}

	.session-title {
		font-weight: 600;
		color: #e5e5e5;
		font-size: 0.875rem;
		white-space: nowrap;
	}

	.session-time {
		font-size: 0.7rem;
		color: #555;
		white-space: nowrap;
	}

	.session-duration {
		font-size: 0.75rem;
		color: #777;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
		white-space: nowrap;
	}

	.session-stats-inline {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.inline-stat {
		font-size: 0.75rem;
		color: #888;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
		white-space: nowrap;
	}

	.delete-btn {
		background: transparent;
		border: none;
		color: #444;
		font-size: 1.25rem;
		cursor: pointer;
		padding: 0;
		line-height: 1;
		transition: color 0.2s;
	}

	.delete-btn:hover {
		color: #ff6b6b;
	}

	.session-stats {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 0.5rem;
	}

	.live-stats {
		margin-top: 0.75rem;
	}

	.stat {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.125rem;
	}

	.stat-value {
		font-size: 1rem;
		font-weight: 600;
		color: #e5e5e5;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
	}

	.stat-label {
		font-size: 0.625rem;
		color: #666;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.drift-good { color: #4ade80; }
	.drift-warning { color: #facc15; }
	.drift-bad { color: #ff6b6b; }

	.session-details {
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
	}

	.detail-row {
		display: flex;
		justify-content: space-between;
		padding: 0.375rem 0;
		font-size: 0.8rem;
	}

	.detail-label {
		color: #666;
	}

	.detail-value {
		color: #999;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
	}

	.detail-row.warning .detail-value {
		color: #facc15;
	}

	.detail-row.error .detail-value {
		color: #ff6b6b;
	}

	.status-complete {
		color: #4ade80;
	}

	.status-stopped {
		color: #888;
	}
</style>
