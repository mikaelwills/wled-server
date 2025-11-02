<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { API_URL } from '$lib/api';
	import ColorWheel from '$lib/ColorWheel.svelte';
	import { sendOSC } from '$lib/osc';
	import { createSseConnection } from '$lib/sse';
	import type { BoardState } from '$lib/types';

	let boards: BoardState[] = [];
	let loading = true;
	let error = '';
	let expandedBoard: string | null = null;
	let showAddForm = false;
	let newBoardId = '';
	let newBoardIp = '';
	let sseConnection: EventSource | null = null;

	async function fetchBoards() {
		try {
			const response = await fetch(`${API_URL}/boards`);
			if (!response.ok) throw new Error('Failed to fetch boards');
			boards = await response.json();
			loading = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
			loading = false;
		}
	}

	async function togglePower(boardId: string) {
		try {
			const response = await fetch(`${API_URL}/board/${boardId}/toggle`, {
				method: 'POST'
			});
			if (!response.ok) throw new Error('Failed to toggle power');
			await fetchBoards();
		} catch (e) {
			console.error('Error toggling power:', e);
		}
	}

	async function setColor(boardId: string, r: number, g: number, b: number) {
		try {
			const response = await fetch(`${API_URL}/board/${boardId}/color`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ r, g, b })
			});
			if (!response.ok) throw new Error('Failed to set color');
			await fetchBoards();
		} catch (e) {
			console.error('Error setting color:', e);
		}
	}

	async function setBrightness(boardId: string, brightness: number) {
		try {
			const response = await fetch(`${API_URL}/board/${boardId}/brightness`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ brightness })
			});
			if (!response.ok) throw new Error('Failed to set brightness');
			await fetchBoards();
		} catch (e) {
			console.error('Error setting brightness:', e);
		}
	}

	async function setEffect(boardId: string, effect: number) {
		try {
			const response = await fetch(`${API_URL}/board/${boardId}/effect`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ effect })
			});
			if (!response.ok) throw new Error('Failed to set effect');
			await fetchBoards();
		} catch (e) {
			console.error('Error setting effect:', e);
		}
	}

	function toggleExpanded(boardId: string) {
		expandedBoard = expandedBoard === boardId ? null : boardId;
	}

	async function addBoard() {
		if (!newBoardId.trim() || !newBoardIp.trim()) {
			alert('Please enter both ID and IP address');
			return;
		}

		try {
			const response = await fetch(`${API_URL}/boards`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ id: newBoardId, ip: newBoardIp })
			});

			if (!response.ok) {
				if (response.status === 409) {
					alert('A board with this ID already exists');
				} else {
					throw new Error('Failed to add board');
				}
				return;
			}

			// Clear form and refresh boards
			newBoardId = '';
			newBoardIp = '';
			showAddForm = false;
			await fetchBoards();
		} catch (e) {
			console.error('Error adding board:', e);
			alert('Failed to add board');
		}
	}

	async function deleteBoard(boardId: string) {
		if (!confirm(`Are you sure you want to delete "${boardId}"?`)) {
			return;
		}

		try {
			const response = await fetch(`${API_URL}/boards/${boardId}`, {
				method: 'DELETE'
			});

			if (!response.ok) {
				if (response.status === 404) {
					alert('Board not found');
				} else {
					throw new Error('Failed to delete board');
				}
				return;
			}

			await fetchBoards();
		} catch (e) {
			console.error('Error deleting board:', e);
			alert('Failed to delete board');
		}
	}

	// Combined Loopy Pro + LED control
	function triggerLoopyAndLED(boardId: string, oscAddress: string, r: number, g: number, b: number) {
		// Send OSC to Loopy Pro
		sendOSC(oscAddress);

		// Set LED color
		setColor(boardId, r, g, b);
	}

	// WLED effects list (from your device)
	const effectsRaw = ["Solid","Blink","Breathe","Wipe","Wipe Random","Random Colors","Sweep","Dynamic","Colorloop","Rainbow","Scan","Scan Dual","Fade","Theater","Theater Rainbow","Running","Saw","Twinkle","Dissolve","Dissolve Rnd","Sparkle","Sparkle Dark","Sparkle+","Strobe","Strobe Rainbow","Strobe Mega","Blink Rainbow","Android","Chase","Chase Random","Chase Rainbow","Chase Flash","Chase Flash Rnd","Rainbow Runner","Colorful","Traffic Light","Sweep Random","Chase 2","Aurora","Stream","Scanner","Lighthouse","Fireworks","Rain","Tetrix","Fire Flicker","Gradient","Loading","Rolling Balls","Fairy","Two Dots","Fairytwinkle","Running Dual","RSVD","Chase 3","Tri Wipe","Tri Fade","Lightning","ICU","Multi Comet","Scanner Dual","Stream 2","Oscillate","Pride 2015","Juggle","Palette","Fire 2012","Colorwaves","Bpm","Fill Noise","Noise 1","Noise 2","Noise 3","Noise 4","Colortwinkles","Lake","Meteor","Meteor Smooth","Railway","Ripple","Twinklefox","Twinklecat","Halloween Eyes","Solid Pattern","Solid Pattern Tri","Spots","Spots Fade","Glitter","Candle","Fireworks Starburst","Fireworks 1D","Bouncing Balls","Sinelon","Sinelon Dual","Sinelon Rainbow","Popcorn","Drip","Plasma","Percent","Ripple Rainbow","Heartbeat","Pacifica","Candle Multi","Solid Glitter","Sunrise","Phased","Twinkleup","Noise Pal","Sine","Phased Noise","Flow","Chunchun","Dancing Shadows","Washing Machine","Rotozoomer","Blends","TV Simulator","Dynamic Smooth","Spaceships","Crazy Bees","Ghost Rider","Blobs","Scrolling Text","Drift Rose","Distortion Waves","Soap","Octopus","Waving Cell","Pixels","Pixelwave","Juggles","Matripix","Gravimeter","Plasmoid","Puddles","Midnoise","Noisemeter","Freqwave","Freqmatrix","GEQ","Waterfall","Freqpixels","RSVD","Noisefire","Puddlepeak","Noisemove","Noise2D","Perlin Move","Ripple Peak","Firenoise","Squared Swirl","RSVD","DNA","Matrix","Metaballs","Freqmap","Gravcenter","Gravcentric","Gravfreq","DJ Light","Funky Plank","RSVD","Pulser","Blurz","Drift","Waverly","Sun Radiation","Colored Bursts","Julia","RSVD","RSVD","RSVD","Game Of Life","Tartan","Polar Lights","Swirl","Lissajous","Frizzles","Plasma Ball","Flow Stripe","Hiphotic","Sindots","DNA Spiral","Black Hole","Wavesins","Rocktaves","Akemi"];
	const effects = effectsRaw.map((name, id) => ({ id, name })).sort((a, b) => a.name.localeCompare(b.name));

	function updateBoardState(boardId: string, state: BoardState) {
		boards = boards.map((b) => (b.id === boardId ? state : b));
	}

	function updateBoardConnectionStatus(boardId: string, connected: boolean) {
		boards = boards.map((b) => (b.id === boardId ? { ...b, connected } : b));
	}

	onMount(() => {
		fetchBoards();

		// Connect to SSE for real-time updates
		sseConnection = createSseConnection(updateBoardState, updateBoardConnectionStatus);
	});

	onDestroy(() => {
		if (sseConnection) {
			sseConnection.close();
		}
	});
</script>

<main>
	{#if showAddForm}
		<div class="add-board-fullscreen">
			<h2>Add New Board</h2>
			<div class="form-group">
				<label for="board-id">Board ID:</label>
				<input
					id="board-id"
					type="text"
					bind:value={newBoardId}
					placeholder="e.g., bedroom-lights"
					class="form-input"
				/>
			</div>
			<div class="form-group">
				<label for="board-ip">IP Address:</label>
				<input
					id="board-ip"
					type="text"
					bind:value={newBoardIp}
					placeholder="e.g., 192.168.1.100"
					class="form-input"
				/>
			</div>
			<div class="form-actions">
				<button class="submit-btn" on:click={addBoard}>Add Board</button>
				<button class="cancel-btn" on:click={() => (showAddForm = false)}>Cancel</button>
			</div>
		</div>
	{:else}
		<h1>WLED Control Panel</h1>

		<!-- Loopy Pro Test Button -->
		<div style="margin: 20px 0; padding: 20px; background: #2a2a2a; border-radius: 8px;">
			<h3 style="margin-top: 0;">Loopy Pro + LED Control</h3>
			<button
				class="loopy-btn"
				on:click={() => triggerLoopyAndLED('mikaels-bed', '/PlayStop/06', 255, 0, 255)}
			>
				▶ Play Track 6 + Purple Lights
			</button>
		</div>

		{#if loading}
			<p>Loading boards...</p>
		{:else if error}
			<p class="error">Error: {error}</p>
		{:else if boards.length === 0}
			<p>No boards configured. Add boards in boards.toml</p>
		{:else}
			<div class="boards">
				{#each boards as board}
					<div class="board-card">
						<div class="board-header">
							<div on:click={() => toggleExpanded(board.id)} style="flex: 1; cursor: pointer;">
								<h2>{board.id}</h2>
								<p class="ip-text">
									<span class="connection-dot {board.connected ? 'connected' : 'disconnected'}"></span>
									IP: {board.ip}
								</p>
							</div>
							{#if board.connected}
								<label class="toggle-switch" on:click={(e) => e.stopPropagation()}>
									<input
										type="checkbox"
										checked={board.on}
										on:change={() => togglePower(board.id)}
									/>
									<span class="toggle-slider"></span>
								</label>
							{/if}
							<span class="expand-icon" on:click={() => toggleExpanded(board.id)} style="cursor: pointer;">
								{expandedBoard === board.id ? '▼' : '▶'}
							</span>
						</div>

						{#if expandedBoard === board.id}
							<div class="board-controls">
								<div class="color-section">
									<ColorWheel
										color={board.color}
										disabled={!board.connected}
										onColorChange={(r, g, b) => setColor(board.id, r, g, b)}
									/>
								</div>

								<div class="brightness-section">
									<input
										id="brightness-{board.id}"
										type="range"
										min="0"
										max="255"
										value={board.brightness}
										disabled={!board.connected}
										on:change={(e) => setBrightness(board.id, parseInt(e.currentTarget.value))}
										class="brightness-slider"
									/>
								</div>

								<div class="effects-section">
									<select
										value={board.effect}
										disabled={!board.connected}
										on:change={(e) => setEffect(board.id, parseInt(e.currentTarget.value))}
										class="effects-dropdown"
									>
										{#each effects as effect}
											<option value={effect.id}>{effect.name}</option>
										{/each}
									</select>
								</div>

								<button class="delete-btn" on:click={() => deleteBoard(board.id)}>
									Delete Board
								</button>
							</div>
						{/if}
					</div>
				{/each}
			</div>

			<button class="add-board-btn" on:click={() => (showAddForm = !showAddForm)}>
				Add Board
			</button>
		{/if}
	{/if}
</main>

<style>
	:global(body) {
		background-color: #1a1a1a;
		color: #e0e0e0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
		margin: 0;
		padding: 0;
	}

	main {
		padding: 2rem;
		max-width: 1200px;
		margin: 0 auto;
	}

	h1 {
		margin-bottom: 2rem;
		color: #ffffff;
	}

	.add-board-btn {
		width: 100%;
		padding: 0.75rem 1.5rem;
		margin-top: 2rem;
		background: #2a2a2a;
		color: #e0e0e0;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
	}

	.add-board-btn:hover {
		background: #333;
		border-color: #555;
	}

	.add-board-fullscreen {
		max-width: 500px;
		margin: 0 auto;
		padding: 2rem;
	}

	.add-board-fullscreen h2 {
		margin-top: 0;
		margin-bottom: 2rem;
		color: #ffffff;
		text-align: center;
	}

	.form-group {
		margin-bottom: 1rem;
	}

	.form-group label {
		display: block;
		margin-bottom: 0.5rem;
		color: #e0e0e0;
		font-weight: 500;
	}

	.form-input {
		width: 100%;
		padding: 0.75rem;
		background: #333;
		color: #e0e0e0;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 1rem;
		box-sizing: border-box;
	}

	.form-input:focus {
		outline: none;
		border-color: #4caf50;
	}

	.form-input::placeholder {
		color: #666;
	}

	.form-actions {
		display: flex;
		gap: 1rem;
		margin-top: 1.5rem;
	}

	.submit-btn,
	.cancel-btn {
		flex: 1;
		padding: 0.75rem;
		border: none;
		border-radius: 6px;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
	}

	.submit-btn {
		background: #4caf50;
		color: white;
	}

	.submit-btn:hover {
		background: #45a049;
	}

	.cancel-btn {
		background: #444;
		color: white;
	}

	.cancel-btn:hover {
		background: #555;
	}

	.delete-btn {
		width: 100%;
		padding: 0.75rem;
		margin-top: 1rem;
		border: none;
		border-radius: 6px;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		background: #d32f2f;
		color: white;
	}

	.delete-btn:hover {
		background: #c62828;
	}

	.delete-btn:active {
		transform: scale(0.98);
	}

	.boards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
		gap: 1rem;
	}

	.board-card {
		border: 1px solid #333;
		border-radius: 8px;
		background: #2a2a2a;
		overflow: hidden;
	}

	.board-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 1rem;
		gap: 1rem;
		transition: background 0.2s;
	}

	.board-header h2 {
		margin: 0 0 0.25rem 0;
		font-size: 1.2rem;
		color: #ffffff;
	}

	.ip-text {
		margin: 0;
		font-size: 0.85rem;
		color: #888;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.connection-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		transition: background-color 0.3s;
	}

	.connection-dot.connected {
		background-color: #4caf50;
		box-shadow: 0 0 6px rgba(76, 175, 80, 0.8);
	}

	.connection-dot.disconnected {
		background-color: #f44336;
		box-shadow: 0 0 6px rgba(244, 67, 54, 0.8);
	}

	.expand-icon {
		font-size: 1.2rem;
		color: #888;
		transition: transform 0.2s;
	}

	.board-controls {
		padding: 1rem;
		border-top: 1px solid #333;
	}

	.color-section {
		display: flex;
		justify-content: center;
		margin-bottom: 1.5rem;
	}

	.toggle-switch {
		position: relative;
		display: inline-block;
		width: 50px;
		height: 26px;
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
		background-color: #444;
		transition: 0.3s;
		border-radius: 26px;
	}

	.toggle-slider:before {
		position: absolute;
		content: '';
		height: 20px;
		width: 20px;
		left: 3px;
		bottom: 3px;
		background-color: white;
		transition: 0.3s;
		border-radius: 50%;
	}

	.toggle-switch input:checked + .toggle-slider {
		background-color: #4caf50;
	}

	.toggle-switch input:checked + .toggle-slider:before {
		transform: translateX(24px);
	}

	.toggle-switch input:disabled + .toggle-slider {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.brightness-section {
		margin-bottom: 1rem;
	}

	.effects-section {
		margin-bottom: 1.5rem;
	}

	.effects-dropdown {
		width: 100%;
		padding: 0.75rem;
		background: #333;
		color: #e0e0e0;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 1rem;
		cursor: pointer;
		outline: none;
	}

	.effects-dropdown:hover {
		background: #3a3a3a;
	}

	.effects-dropdown:focus {
		border-color: #4caf50;
	}

	.effects-dropdown:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.brightness-slider {
		width: 100%;
		height: 8px;
		border-radius: 4px;
		background: linear-gradient(to right, #000, #fff);
		outline: none;
		-webkit-appearance: none;
		cursor: pointer;
	}

	.brightness-slider:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.brightness-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #4caf50;
		cursor: pointer;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
	}

	.brightness-slider::-moz-range-thumb {
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #4caf50;
		cursor: pointer;
		border: none;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
	}

	.error {
		color: #ff6b6b;
	}

	.loopy-btn {
		padding: 12px 24px;
		font-size: 1.1rem;
		background: #7b2cbf;
		color: white;
		border: none;
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.2s;
		font-weight: 600;
	}

	.loopy-btn:hover {
		background: #9d4edd;
		transform: translateY(-2px);
		box-shadow: 0 4px 8px rgba(123, 44, 191, 0.4);
	}

	.loopy-btn:active {
		transform: translateY(0);
	}
</style>
