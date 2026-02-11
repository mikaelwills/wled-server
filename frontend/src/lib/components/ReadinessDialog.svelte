<script lang="ts">
	import { onMount } from 'svelte';
	import { API_URL } from '$lib/api';

	interface ReadinessData {
		ready: boolean;
		boards_total: number;
		boards_connected: number;
		audio_thread_active: boolean;
		audio_device_found: boolean;
		programs_loaded: number;
		e131_transports: number;
		errors: string[];
	}

	let open = $state(true);
	let data = $state<ReadinessData | null>(null);
	let fetchError = $state<string | null>(null);
	let loading = $state(true);

	onMount(async () => {
		try {
			const res = await fetch(`${API_URL}/ready`);
			if (res.ok) {
				data = await res.json();
			} else {
				fetchError = `Server returned ${res.status}`;
			}
		} catch (e) {
			fetchError = 'Cannot reach server';
		}
		loading = false;
	});

	function close() {
		open = false;
	}
</script>

{#if open}
<div class="modal-overlay" onclick={close}>
	<div class="modal-content" onclick={(e) => e.stopPropagation()}>
		<div class="modal-header">
			<h3>System Status</h3>
		</div>
		<div class="modal-body">
			{#if loading}
				<p class="status-text">Checking systems...</p>
			{:else if fetchError}
				<div class="status-row error">
					<span class="indicator error"></span>
					<span>{fetchError}</span>
				</div>
			{:else if data}
				<div class="status-row" class:ok={data.boards_connected > 0} class:warn={data.boards_connected === 0}>
					<span class="indicator" class:ok={data.boards_connected > 0} class:warn={data.boards_connected === 0}></span>
					<span>Boards: {data.boards_connected}/{data.boards_total} connected</span>
				</div>
				<div class="status-row" class:ok={data.audio_thread_active} class:error={!data.audio_thread_active}>
					<span class="indicator" class:ok={data.audio_thread_active} class:error={!data.audio_thread_active}></span>
					<span>Audio thread: {data.audio_thread_active ? 'running' : 'dead'}</span>
				</div>
				<div class="status-row" class:ok={data.audio_device_found} class:warn={!data.audio_device_found}>
					<span class="indicator" class:ok={data.audio_device_found} class:warn={!data.audio_device_found}></span>
					<span>Audio device: {data.audio_device_found ? 'found' : 'none'}</span>
				</div>
				<div class="status-row" class:ok={data.programs_loaded > 0} class:warn={data.programs_loaded === 0}>
					<span class="indicator" class:ok={data.programs_loaded > 0} class:warn={data.programs_loaded === 0}></span>
					<span>Programs: {data.programs_loaded} loaded</span>
				</div>
				<div class="status-row" class:ok={data.e131_transports > 0} class:warn={data.e131_transports === 0}>
					<span class="indicator" class:ok={data.e131_transports > 0} class:warn={data.e131_transports === 0}></span>
					<span>E1.31 transports: {data.e131_transports}</span>
				</div>
				{#if data.errors.length > 0}
					<div class="errors">
						{#each data.errors as error}
							<p class="error-text">{error}</p>
						{/each}
					</div>
				{/if}
				<div class="overall" class:ready={data.ready} class:not-ready={!data.ready}>
					{data.ready ? 'READY' : 'NOT READY'}
				</div>
			{/if}
		</div>
		<div class="modal-footer">
			<button class="dismiss-btn" onclick={close}>Dismiss</button>
		</div>
	</div>
</div>
{/if}

<style>
	.modal-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: rgba(0, 0, 0, 0.85);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 2000;
	}

	.modal-content {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 12px;
		min-width: 320px;
		max-width: 90vw;
		overflow: hidden;
	}

	.modal-header {
		padding: 1rem 1.25rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	.modal-header h3 {
		margin: 0;
		font-size: 1rem;
		font-weight: 500;
		color: #e5e5e5;
	}

	.modal-body {
		padding: 1.25rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.status-text {
		color: #888;
		text-align: center;
		margin: 0;
	}

	.status-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		font-size: 0.9rem;
		color: #ccc;
	}

	.indicator {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		flex-shrink: 0;
		background: #555;
	}

	.indicator.ok {
		background: #22c55e;
		box-shadow: 0 0 6px rgba(34, 197, 94, 0.4);
	}

	.indicator.warn {
		background: #eab308;
		box-shadow: 0 0 6px rgba(234, 179, 8, 0.4);
	}

	.indicator.error {
		background: #ef4444;
		box-shadow: 0 0 6px rgba(239, 68, 68, 0.4);
	}

	.errors {
		padding: 0.5rem 0.75rem;
		background: rgba(239, 68, 68, 0.1);
		border-radius: 6px;
	}

	.error-text {
		margin: 0;
		color: #ef4444;
		font-size: 0.8rem;
	}

	.overall {
		text-align: center;
		font-size: 1.25rem;
		font-weight: 700;
		letter-spacing: 0.1em;
		padding: 0.75rem;
		border-radius: 8px;
		margin-top: 0.25rem;
	}

	.overall.ready {
		color: #22c55e;
		background: rgba(34, 197, 94, 0.1);
		border: 1px solid rgba(34, 197, 94, 0.2);
	}

	.overall.not-ready {
		color: #ef4444;
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid rgba(239, 68, 68, 0.2);
	}

	.modal-footer {
		padding: 0.75rem 1.25rem;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
		display: flex;
		justify-content: flex-end;
	}

	.dismiss-btn {
		background: rgba(255, 255, 255, 0.1);
		border: none;
		color: #ccc;
		padding: 0.5rem 1.25rem;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
	}

	.dismiss-btn:hover {
		background: rgba(255, 255, 255, 0.15);
	}
</style>
