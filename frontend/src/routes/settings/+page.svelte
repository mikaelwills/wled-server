<script lang="ts">
	import { onMount } from 'svelte';
	import { loopyProSettings, loopyProSettingsLoading, resamplingQuality, resamplingQualityLoading } from '$lib/store';
	import { updateLoopyProSettings } from '$lib/loopy-db';
	import { updateResamplingQuality } from '$lib/audio-db';
	import { initPrograms } from '$lib/programs-db';
	import { timingMonitorVisible, toggleTimingMonitor, timingSnapshot, updateDriftThreshold } from '$lib/timing-store';
	import { API_URL } from '$lib/api';

	import type { AudioSource, ResamplingQuality } from '$lib/store';

	let ip = $state($loopyProSettings.ip);
	let port = $state($loopyProSettings.port);
	let audioSource: AudioSource = $state($loopyProSettings.audio_source);
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

	interface AudioDevice {
		id: string;
		name: string;
		output_channels: number;
		is_default: boolean;
	}

	let storageStatus = $state<StorageStatus | null>(null);
	let storageLoading = $state(true);
	let restarting = $state(false);
	let reloadingPrograms = $state(false);
	let audioDevices: AudioDevice[] = $state([]);
	let audioDevicesLoading = $state(true);
	let selectedDeviceId: string | null = $state(null);
	let switchingDevice = $state(false);

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

	async function fetchAudioSettings() {
		try {
			const res = await fetch(`${API_URL}/audio/settings`);
			if (res.ok) {
				const settings = await res.json();
				selectedDeviceId = settings.preferred_device_id;
			}
		} catch (e) {
			console.error('Failed to fetch audio settings:', e);
		}
	}

	async function selectDevice(deviceId: string | null) {
		if (switchingDevice) return;
		switchingDevice = true;
		try {
			const res = await fetch(`${API_URL}/audio/device/select`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ device_id: deviceId })
			});
			if (res.ok) {
				selectedDeviceId = deviceId;
			}
		} catch (e) {
			console.error('Failed to select device:', e);
		} finally {
			switchingDevice = false;
		}
	}

	async function fetchAudioDevices() {
		audioDevicesLoading = true;
		try {
			const res = await fetch(`${API_URL}/audio/devices`);
			if (res.ok) {
				audioDevices = await res.json();
			}
		} catch (e) {
			console.error('Failed to fetch audio devices:', e);
		}
		audioDevicesLoading = false;
	}

	onMount(() => {
		fetchStorageStatus();
		fetchAudioSettings();
		fetchAudioDevices();
	});
	$effect(() => {
		ip = $loopyProSettings.ip;
		port = $loopyProSettings.port;
		audioSource = $loopyProSettings.audio_source;
		audioSyncDelay = $loopyProSettings.audio_sync_delay_ms ?? 0;
	});

	async function saveSettings() {
		try {
			await updateLoopyProSettings({ ip, port, audio_source: audioSource, audio_sync_delay_ms: audioSyncDelay });
			saved = true;
			setTimeout(() => {
				saved = false;
			}, 2000);
		} catch (err) {
			console.error('Failed to save settings:', err);
			alert('Failed to save settings');
		}
	}

	function setAudioSource(source: AudioSource) {
		audioSource = source;
		saveSettings();
	}
</script>

<div class="settings-page">
	<div class="settings-container">
		<div class="card">
			<h2>Audio Source</h2>

			{#if $loopyProSettingsLoading}
				<p>Loading settings...</p>
			{:else}
			<div class="audio-source-row">
				<button
					class="source-button"
					class:active={audioSource === 'audio_engine'}
					onclick={() => setAudioSource('audio_engine')}
				>
					Audio Engine
				</button>
				<button
					class="source-button"
					class:active={audioSource === 'loopy_pro'}
					onclick={() => setAudioSource('loopy_pro')}
				>
					Loopy Pro
				</button>
			</div>

			{#if audioSource === 'loopy_pro'}
			<p class="help-text">Loopy Pro IP address</p>
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
			{/if}

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

			{#if audioSource === 'audio_engine'}
			<div class="section-divider"></div>
			<h2>Audio Devices</h2>

			{#if audioDevicesLoading}
				<p class="help-text" style="text-align: center;">Loading devices...</p>
			{:else}
				<div class="device-list" class:disabled={switchingDevice}>
					{#each audioDevices as device}
						<div
							class="device-card"
							class:is-selected={selectedDeviceId === device.id}
							onclick={() => selectDevice(device.id)}
						>
							<div class="device-info">
								<span class="device-name">{device.name}</span>
								<span class="device-channels">{device.output_channels} channels{device.sample_rate ? ` @ ${device.sample_rate/1000}kHz` : ''}</span>
							</div>
							{#if selectedDeviceId === device.id}
								<span class="selected-badge">{switchingDevice ? 'Switching...' : 'Selected'}</span>
							{:else if device.is_default}
								<span class="default-badge">Default</span>
							{/if}
						</div>
					{/each}
				</div>

				<button onclick={fetchAudioDevices} class="save-button">
					Refresh Devices
				</button>

				<div class="section-divider"></div>
				<h2>Resampling Quality</h2>
				<p class="help-text">Higher quality takes longer but sounds better</p>

				{#if $resamplingQualityLoading}
					<p class="help-text" style="text-align: center;">Loading...</p>
				{:else}
					<div class="quality-row">
						<button
							class="quality-button"
							class:active={$resamplingQuality === 'fast'}
							onclick={() => updateResamplingQuality('fast')}
						>
							<span class="quality-name">Fast</span>
							<span class="quality-desc">Quick testing</span>
						</button>
						<button
							class="quality-button"
							class:active={$resamplingQuality === 'balanced'}
							onclick={() => updateResamplingQuality('balanced')}
						>
							<span class="quality-name">Balanced</span>
							<span class="quality-desc">Good quality</span>
						</button>
						<button
							class="quality-button"
							class:active={$resamplingQuality === 'high'}
							onclick={() => updateResamplingQuality('high')}
						>
							<span class="quality-name">High</span>
							<span class="quality-desc">Best quality</span>
						</button>
					</div>
				{/if}
			{/if}
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

	.section-divider {
		height: 1px;
		background: rgba(255, 255, 255, 0.05);
		margin: 0.5rem 0;
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

	.audio-source-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.5rem;
	}

	.source-button {
		padding: 0.75rem 1rem;
		background: #0c0c0c;
		color: #666;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
	}

	.source-button:hover {
		background: #111;
		color: #888;
		border-color: rgba(255, 255, 255, 0.08);
	}

	.source-button.active {
		background: rgba(168, 85, 247, 0.1);
		color: #a855f7;
		border-color: rgba(168, 85, 247, 0.3);
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

	.device-list {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.device-list.disabled {
		pointer-events: none;
		opacity: 0.6;
	}

	.device-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 1rem;
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.device-card:hover {
		background: #111;
		border-color: rgba(255, 255, 255, 0.08);
	}

	.device-card.is-selected {
		border-color: rgba(34, 197, 94, 0.5);
		background: rgba(34, 197, 94, 0.05);
	}

	.device-info {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.device-name {
		color: #e5e5e5;
		font-size: 0.85rem;
		font-weight: 500;
	}

	.device-channels {
		color: #6b7280;
		font-size: 0.7rem;
	}

	.default-badge {
		font-size: 0.65rem;
		color: #a855f7;
		background: rgba(168, 85, 247, 0.1);
		padding: 0.2rem 0.4rem;
		border-radius: 4px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 500;
	}

	.selected-badge {
		font-size: 0.65rem;
		color: #22c55e;
		background: rgba(34, 197, 94, 0.1);
		padding: 0.2rem 0.4rem;
		border-radius: 4px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 500;
	}

	.quality-row {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 0.5rem;
	}

	.quality-button {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.25rem;
		padding: 0.75rem 0.5rem;
		background: #0c0c0c;
		color: #666;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 8px;
		font-size: 0.875rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	.quality-button:hover {
		background: #111;
		color: #888;
		border-color: rgba(255, 255, 255, 0.08);
	}

	.quality-button.active {
		background: rgba(34, 197, 94, 0.1);
		color: #22c55e;
		border-color: rgba(34, 197, 94, 0.3);
	}

	.quality-name {
		font-weight: 500;
	}

	.quality-desc {
		font-size: 0.65rem;
		opacity: 0.7;
	}
</style>
