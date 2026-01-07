<script>
	import { loopyProSettings, loopyProSettingsLoading } from '$lib/store';
	import { updateLoopyProSettings } from '$lib/loopy-db';

	let ip = $state($loopyProSettings.ip);
	let port = $state($loopyProSettings.port);
	let muteAudio = $state($loopyProSettings.mute_audio);
	let audioSyncDelay = $state($loopyProSettings.audio_sync_delay_ms ?? 0);
	let saved = $state(false);

	// Sync local state with store changes
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
		<h1>Loopy Pro IP Address</h1>

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
</div>

<style>
	:global(body) {
		background-color: #0a0a0a;
		color: #e5e5e5;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
	}

	.settings-page {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: calc(100vh - 60px);
		padding: 2rem;
	}

	.settings-container {
		width: 100%;
		max-width: 320px;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	h1 {
		font-size: 0.875rem;
		margin: 0;
		color: #9ca3af;
		text-align: center;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.input-row {
		display: grid;
		grid-template-columns: 3fr 1fr;
		gap: 0.5rem;
		width: 100%;
	}

	.text-input {
		padding: 0.75rem 1rem;
		background: linear-gradient(145deg, #0d1117 0%, #0b0d14 50%, #080a12 100%);
		border: 1px solid rgba(56, 89, 138, 0.2);
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
		border-color: rgba(56, 89, 138, 0.5);
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
		background-color: #a855f7;
		color: white;
		border: none;
		border-radius: 6px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		box-sizing: border-box;
	}

	.save-button:hover {
		background-color: #9333ea;
	}

	.save-button:active {
		background-color: #7e22ce;
		transform: scale(0.98);
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
		background-color: rgba(56, 89, 138, 0.2);
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
		background-color: rgba(56, 89, 138, 0.5);
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
</style>
