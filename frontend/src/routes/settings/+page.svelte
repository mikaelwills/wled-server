<script lang="ts">
	import { onMount } from 'svelte';
	import { loopyProSettings, loopyProSettingsLoading } from '$lib/store';
	import { updateLoopyProSettings } from '$lib/loopy-db';
	import { initPrograms } from '$lib/programs-db';
	import { timingMonitorVisible, toggleTimingMonitor, timingSnapshot, updateDriftThreshold } from '$lib/timing-store';
	import { API_URL } from '$lib/api';

	let ip = $state($loopyProSettings.ip);
	let port = $state($loopyProSettings.port);
	let muteAudio = $state($loopyProSettings.mute_audio);
	let audioSyncDelay = $state($loopyProSettings.audio_sync_delay_ms ?? 0);
	let saved = $state(false);

	interface StorageStatus {
		usb_mounted: boolean;
		programs_path: string;
		programs_available: boolean;
		programs_count: number;
		audio_path: string;
		audio_available: boolean;
		audio_count: number;
	}

	let storageStatus = $state<StorageStatus | null>(null);
	let storageLoading = $state(true);
	let restarting = $state(false);
	let reloadingPrograms = $state(false);

	async function fetchStorageStatus() {
		try {
			const res = await fetch(`${API_URL}/settings/storage`);
			storageStatus = await res.json();
		} catch (e) {
			console.error('Failed to fetch storage status:', e);
		} finally {
			storageLoading = false;
		}
	}

	async function reloadPrograms() {
		reloadingPrograms = true;
		try {
			const res = await fetch(`${API_URL}/programs/reload`, { method: 'POST' });
			if (res.ok) {
				await fetchStorageStatus();
				await initPrograms();
			} else {
				alert('Failed to reload programs');
			}
		} catch (e) {
			console.error('Failed to reload programs:', e);
			alert('Failed to reload programs');
		} finally {
			reloadingPrograms = false;
		}
	}

	async function restartServer() {
		if (!confirm('Restart the server? The page will reload in a few seconds.')) return;

		restarting = true;
		try {
			await fetch(`${API_URL}/server/restart`, { method: 'POST' });
			setTimeout(() => {
				window.location.reload();
			}, 3000);
		} catch (e) {
			console.error('Failed to restart server:', e);
			restarting = false;
		}
	}

	onMount(() => {
		fetchStorageStatus();
	});

	$effect(() => {
		ip = $loopyProSettings.ip;
		port = $loopyProSettings.port;
		muteAudio = $loopyProSettings.mute_audio;
		audioSyncDelay = $loopyProSettings.audio_sync_delay_ms ?? 0;
	});

	async function saveSettings() {
		try {
			await updateLoopyProSettings({ ip, port, mute_audio: muteAudio, audio_sync_delay_ms: audioSyncDelay });
			saved = true;
			setTimeout(() => {
				saved = false;
			}, 2000);
		} catch (err) {
			console.error('Failed to save settings:', err);
			alert('Failed to save settings');
		}
	}
</script>

<div class="settings-page">
	<div class="settings-container">
		<div class="card">
			<h2>Loopy Pro</h2>

			{#if $loopyProSettingsLoading}
				<p>Loading settings...</p>
			{:else}
			<div class="input-row">
				<input
					id="ip"
					type="text"
					bind:value={ip}
					placeholder="192.168.1.242"
					class="text-input ip-input"
				/>
				<input
					id="port"
					type="number"
					bind:value={port}
					placeholder="9595"
					class="text-input port-input"
				/>
			</div>

			<div class="toggle-row">
				<label for="mute-toggle" class="toggle-label">
					Mute App Audio
				</label>
				<label class="toggle-switch">
					<input
						id="mute-toggle"
						type="checkbox"
						bind:checked={muteAudio}
					/>
					<span class="toggle-slider"></span>
				</label>
			</div>

			<div class="delay-row">
				<label for="audio-sync-delay" class="delay-label">
					Sync Offset (ms)
				</label>
				<input
					id="audio-sync-delay"
					type="number"
					bind:value={audioSyncDelay}
					min="-1000"
					max="1000"
					class="text-input delay-input"
				/>
			</div>
			<p class="help-text">+ delays audio / − delays lights</p>

			<button onclick={saveSettings} class="save-button">
				{saved ? 'Saved' : 'Save'}
			</button>
			{/if}
		</div>

		<div class="card">
			<h2>Developer Tools</h2>

			<div class="toggle-row">
				<label for="timing-toggle" class="toggle-label">
					Timing Monitor
				</label>
				<label class="toggle-switch">
					<input
						id="timing-toggle"
						type="checkbox"
						checked={$timingMonitorVisible}
						onchange={toggleTimingMonitor}
					/>
					<span class="toggle-slider"></span>
				</label>
			</div>
			<p class="help-text">Press Ctrl+T to toggle (tracks cue drift)</p>
		</div>

		<div class="card">
			<h2>Server Status</h2>

			{#if storageLoading}
				<p class="status-text">Loading...</p>
			{:else if storageStatus}
				<div class="status-grid">
					<div class="status-row">
						<span class="status-label">USB Storage</span>
						<span class="status-value {storageStatus.usb_mounted ? 'ok' : 'error'}">
							{storageStatus.usb_mounted ? 'Mounted' : 'NOT MOUNTED'}
						</span>
					</div>
					<div class="status-row">
						<span class="status-label">Programs</span>
						<span class="status-value {storageStatus.programs_available ? 'ok' : 'error'}">
							{storageStatus.programs_available ? `${storageStatus.programs_count} loaded` : 'Missing'}
						</span>
					</div>
					<div class="status-row">
						<span class="status-label">Audio</span>
						<span class="status-value {storageStatus.audio_available ? 'ok' : 'error'}">
							{storageStatus.audio_available ? `${storageStatus.audio_count} files` : 'Missing'}
						</span>
					</div>
				</div>

				{#if !storageStatus.usb_mounted || !storageStatus.programs_available}
					<p class="warning-text">⚠️ USB storage issue detected. Try restarting the server after ensuring USB is connected.</p>
				{/if}
			{/if}

			<button onclick={reloadPrograms} class="reload-button" disabled={reloadingPrograms}>
				{reloadingPrograms ? 'Reloading...' : 'Reload Programs'}
			</button>
			<p class="help-text">Use if USB was mounted after server started</p>

			<button onclick={restartServer} class="restart-button" disabled={restarting}>
				{restarting ? 'Restarting...' : 'Restart Server'}
			</button>
		</div>
	</div>
</div>

<style>
	:global(body) {
		background-color: #0a0a0a;
		color: #e5e5e5;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
	}

	.settings-page {
		min-height: calc(100vh - 60px);
		padding: 2rem;
	}

	.settings-container {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1.5rem;
		max-width: 1200px;
		margin: 0 auto;
	}

	.card {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		padding: 1.5rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	h2 {
		font-size: 0.875rem;
		margin: 0;
		color: #9ca3af;
		text-align: center;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	@media (max-width: 900px) {
		.settings-container {
			grid-template-columns: 1fr;
			max-width: 400px;
		}
	}

	.input-row {
		display: grid;
		grid-template-columns: 3fr 1fr;
		gap: 0.5rem;
		width: 100%;
	}

	.text-input {
		padding: 0.75rem 1rem;
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 6px;
		color: #e5e5e5;
		font-size: 1rem;
		transition: border-color 0.2s;
		text-align: center;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
		box-sizing: border-box;
		width: 100%;
	}

	.ip-input {
		grid-column: 1;
	}

	.port-input {
		grid-column: 2;
	}

	.text-input:focus {
		outline: none;
		border-color: rgba(255, 255, 255, 0.08);
	}

	.text-input::placeholder {
		color: #4b5563;
	}

	/* Remove number input arrows */
	input[type='number']::-webkit-inner-spin-button,
	input[type='number']::-webkit-outer-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}

	input[type='number'] {
		-moz-appearance: textfield;
	}

	.save-button {
		width: 100%;
		padding: 0.75rem 1rem;
		background: #0c0c0c;
		color: #888;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		box-sizing: border-box;
	}

	.save-button:hover {
		background: #0e0e0e;
		color: #fff;
		border-color: rgba(255, 255, 255, 0.05);
	}

	.save-button:active {
		background: #0a0a0a;
	}

	.toggle-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.toggle-label {
		font-size: 0.875rem;
		color: #e5e5e5;
		cursor: pointer;
	}

	.toggle-switch {
		position: relative;
		display: inline-block;
		width: 48px;
		height: 28px;
	}

	.toggle-switch input {
		opacity: 0;
		width: 0;
		height: 0;
	}

	.toggle-slider {
		position: absolute;
		cursor: pointer;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background-color: rgba(255, 255, 255, 0.05);
		transition: 0.3s;
		border-radius: 28px;
	}

	.toggle-slider:before {
		position: absolute;
		content: '';
		height: 20px;
		width: 20px;
		left: 4px;
		bottom: 4px;
		background-color: rgba(255, 255, 255, 0.2);
		transition: 0.3s;
		border-radius: 50%;
	}

	input:checked + .toggle-slider {
		background-color: #a855f7;
	}

	input:checked + .toggle-slider:before {
		background-color: white;
		transform: translateX(20px);
	}

	.delay-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-top: 0.5rem;
	}

	.delay-label {
		font-size: 0.875rem;
		color: #e5e5e5;
	}

	.delay-input {
		width: 80px;
		text-align: center;
	}

	.help-text {
		font-size: 0.75rem;
		color: #6b7280;
		margin: 0.25rem 0 0.5rem 0;
		line-height: 1.4;
	}

	.status-grid {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.75rem;
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 8px;
	}

	.status-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.status-label {
		font-size: 0.8rem;
		color: #9ca3af;
	}

	.status-value {
		font-size: 0.8rem;
		font-weight: 500;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
	}

	.status-value.ok {
		color: #4ade80;
	}

	.status-value.error {
		color: #f87171;
	}

	.status-text {
		font-size: 0.875rem;
		color: #6b7280;
		text-align: center;
	}

	.warning-text {
		font-size: 0.75rem;
		color: #fbbf24;
		margin: 0.5rem 0;
		line-height: 1.4;
	}

	.reload-button {
		width: 100%;
		padding: 0.75rem 1rem;
		background: #1a1a1a;
		color: #4ade80;
		border: 1px solid rgba(74, 222, 128, 0.2);
		border-radius: 12px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
	}

	.reload-button:hover:not(:disabled) {
		background: #1a2a1a;
		border-color: rgba(74, 222, 128, 0.4);
	}

	.reload-button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.restart-button {
		width: 100%;
		padding: 0.75rem 1rem;
		background: #1a1a1a;
		color: #f87171;
		border: 1px solid rgba(248, 113, 113, 0.2);
		border-radius: 12px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		margin-top: 0.5rem;
	}

	.restart-button:hover:not(:disabled) {
		background: #2a1a1a;
		border-color: rgba(248, 113, 113, 0.4);
	}

	.restart-button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
