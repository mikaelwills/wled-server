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
	let isCreatingGroup = false;
	let selectedMemberIds: string[] = [];
	let showEditForm = false;
	let editBoardId = '';
	let editBoardIp = '';
	let editingBoardId = ''; // Original ID being edited
	let sseConnection: EventSource | null = null;

	async function fetchBoards() {
		try {
			const response = await fetch(`${API_URL}/boards`);
			if (!response.ok) throw new Error('Failed to fetch boards');
			boards = await response.json();

			// Derive group state from member boards
			boards = boards.map(board => {
				if (board.isGroup && board.memberIds) {
					const members = boards.filter(b =>
						!b.isGroup && board.memberIds?.includes(b.id)
					);

					if (members.length > 0) {
						// Derive state from members
						const firstMember = members[0];
						return {
							...board,
							color: firstMember.color,
							brightness: firstMember.brightness,
							effect: firstMember.effect,
							on: members.every(m => m.on)
						};
					}
				}
				return board;
			});

			// Sort: groups first, then regular boards
			boards.sort((a, b) => {
				if (a.isGroup && !b.isGroup) return -1;
				if (!a.isGroup && b.isGroup) return 1;
				return 0;
			});

			loading = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
			loading = false;
		}
	}

	async function togglePower(boardId: string) {
		const board = boards.find(b => b.id === boardId);
		if (board?.isGroup && board.memberIds) {
			// Toggle all members in parallel and collect their states
			const memberStatesPromises = board.memberIds.map(async (memberId) => {
				try {
					const response = await fetch(`${API_URL}/board/${memberId}/toggle`, {
						method: 'POST'
					});
					if (!response.ok) throw new Error(`Failed to toggle power for ${memberId}`);
					return await response.json();
				} catch (e) {
					console.error(`Error toggling power for ${memberId}:`, e);
					return null;
				}
			});

			const memberStates = (await Promise.all(memberStatesPromises)).filter((state): state is BoardState => state !== null);

			// Update group's state based on member states (all must be ON for group to show ON)
			const allOn = memberStates.length > 0 && memberStates.every(m => m.on);
			boards = boards.map(b => b.id === boardId ? {...b, on: allOn} : b);
		} else {
			// Regular board
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
	}

	async function setColor(boardId: string, r: number, g: number, b: number) {
		// Ensure r, g, b are actually numbers
		const red = Number(r);
		const green = Number(g);
		const blue = Number(b);

		const board = boards.find(b => b.id === boardId);
		if (board?.isGroup && board.memberIds) {
			// Set color for all members in parallel
			await Promise.all(
				board.memberIds.map(async (memberId) => {
					try {
						const response = await fetch(`${API_URL}/board/${memberId}/color`, {
							method: 'POST',
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify({ r: red, g: green, b: blue })
						});
						if (!response.ok) throw new Error(`Failed to set color for ${memberId}`);
					} catch (e) {
						console.error(`Error setting color for ${memberId}:`, e);
					}
				})
			);
			// Update the group's own state
			boards = boards.map(b => b.id === boardId ? {...b, color: [red, green, blue] as [number, number, number]} : b);
		} else {
			// Regular board
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
	}

	async function setBrightness(boardId: string, brightness: number) {
		const board = boards.find(b => b.id === boardId);
		if (board?.isGroup && board.memberIds) {
			// Set brightness for all members in parallel
			await Promise.all(
				board.memberIds.map(async (memberId) => {
					try {
						const response = await fetch(`${API_URL}/board/${memberId}/brightness`, {
							method: 'POST',
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify({ brightness })
						});
						if (!response.ok) throw new Error(`Failed to set brightness for ${memberId}`);
					} catch (e) {
						console.error(`Error setting brightness for ${memberId}:`, e);
					}
				})
			);
			// Update the group's own state
			boards = boards.map(b => b.id === boardId ? {...b, brightness} : b);
		} else {
			// Regular board
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
	}

	async function setEffect(boardId: string, effect: number) {
		const board = boards.find(b => b.id === boardId);
		if (board?.isGroup && board.memberIds) {
			// Set effect for all members in parallel
			await Promise.all(
				board.memberIds.map(async (memberId) => {
					try {
						const response = await fetch(`${API_URL}/board/${memberId}/effect`, {
							method: 'POST',
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify({ effect })
						});
						if (!response.ok) throw new Error(`Failed to set effect for ${memberId}`);
					} catch (e) {
						console.error(`Error setting effect for ${memberId}:`, e);
					}
				})
			);
			// Update the group's own state
			boards = boards.map(b => b.id === boardId ? {...b, effect} : b);
		} else {
			// Regular board
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
	}

	function toggleExpanded(boardId: string) {
		expandedBoard = expandedBoard === boardId ? null : boardId;
	}

	// One-time migration: move groups from localStorage to backend
	async function migrateLocalStorageGroups() {
		try {
			const stored = localStorage.getItem('wled-groups');
			if (!stored) return; // No groups to migrate

			const groups = JSON.parse(stored);
			console.log(`Migrating ${groups.length} group(s) from localStorage to backend...`);

			for (const group of groups) {
				try {
					const response = await fetch(`${API_URL}/groups`, {
						method: 'POST',
						headers: { 'Content-Type': 'application/json' },
						body: JSON.stringify({
							id: group.id,
							members: group.memberIds || []
						})
					});

					if (response.ok) {
						console.log(`Migrated group: ${group.id}`);
					} else if (response.status === 409) {
						console.log(`Group ${group.id} already exists, skipping`);
					} else {
						console.error(`Failed to migrate group ${group.id}`);
					}
				} catch (e) {
					console.error(`Error migrating group ${group.id}:`, e);
				}
			}

			// Clear localStorage after successful migration
			localStorage.removeItem('wled-groups');
			console.log('Migration complete, localStorage cleared');
		} catch (e) {
			console.error('Error during migration:', e);
		}
	}

	async function addBoard() {
		// Validation
		if (!newBoardId.trim()) {
			alert('Please enter an ID');
			return;
		}

		if (isCreatingGroup) {
			// Creating a group
			if (selectedMemberIds.length === 0) {
				alert('Please select at least one board for the group');
				return;
			}

			try {
				const response = await fetch(`${API_URL}/groups`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({
						id: newBoardId,
						members: selectedMemberIds
					})
				});

				if (!response.ok) {
					if (response.status === 409) {
						alert('A group with this ID already exists');
					} else if (response.status === 400) {
						alert('One or more member boards not found');
					} else {
						throw new Error('Failed to create group');
					}
					return;
				}

				// Clear form and refresh boards
				newBoardId = '';
				selectedMemberIds = [];
				isCreatingGroup = false;
				showAddForm = false;
				await fetchBoards();
			} catch (e) {
				console.error('Error creating group:', e);
				alert('Failed to create group');
			}
		} else {
			// Creating a regular board
			if (!newBoardIp.trim()) {
				alert('Please enter an IP address');
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
	}

	async function deleteBoard(boardId: string) {
		const board = boards.find(b => b.id === boardId);

		if (!confirm(`Are you sure you want to delete "${boardId}"?`)) {
			return;
		}

		if (board?.isGroup) {
			// Delete group from backend
			try {
				const response = await fetch(`${API_URL}/groups/${boardId}`, {
					method: 'DELETE'
				});

				if (!response.ok) throw new Error('Failed to delete group');
				await fetchBoards();
			} catch (e) {
				console.error('Error deleting group:', e);
				alert('Failed to delete group');
			}
		} else {
			// Delete regular board from backend
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
	}

	function openEditForm(board: BoardState) {
		editingBoardId = board.id;
		editBoardId = board.id;
		editBoardIp = board.ip;
		showEditForm = true;
	}

	async function updateBoard() {
		if (!editBoardId.trim() || !editBoardIp.trim()) {
			alert('Board ID and IP are required');
			return;
		}

		try {
			const response = await fetch(`${API_URL}/boards/${editingBoardId}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					new_id: editBoardId !== editingBoardId ? editBoardId : undefined,
					new_ip: editBoardIp
				})
			});

			if (response.ok) {
				showEditForm = false;
				editBoardId = '';
				editBoardIp = '';
				editingBoardId = '';
				await fetchBoards();
			} else {
				throw new Error('Failed to update board');
			}
		} catch (e) {
			console.error('Error updating board:', e);
			alert('Failed to update board');
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

	onMount(async () => {
		// Migrate localStorage groups to backend (one-time operation)
		await migrateLocalStorageGroups();

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
	{#if showEditForm}
		<div class="add-board-fullscreen">
			<h2>Edit Board</h2>
			<div class="form-group">
				<label for="edit-board-id">Board ID:</label>
				<input
					id="edit-board-id"
					type="text"
					bind:value={editBoardId}
					placeholder="e.g., bedroom-lights"
					class="form-input"
				/>
			</div>
			<div class="form-group">
				<label for="edit-board-ip">IP Address:</label>
				<input
					id="edit-board-ip"
					type="text"
					bind:value={editBoardIp}
					placeholder="e.g., 192.168.1.100"
					class="form-input"
				/>
			</div>
			<div class="form-actions">
				<button class="submit-btn" on:click={updateBoard}>Update Board</button>
				<button class="cancel-btn" on:click={() => (showEditForm = false)}>Cancel</button>
			</div>
		</div>
	{:else if showAddForm}
		<div class="add-board-fullscreen">
			<h2>Add New {isCreatingGroup ? 'Group' : 'Board'}</h2>

			<!-- Group checkbox -->
			<div class="form-group" style="margin-bottom: 1.5rem;">
				<label style="display: flex; align-items: center; gap: 0.5rem; cursor: pointer;">
					<input type="checkbox" bind:checked={isCreatingGroup} />
					<span>This is a group</span>
				</label>
			</div>

			<div class="form-group">
				<label for="board-id">{isCreatingGroup ? 'Group' : 'Board'} ID:</label>
				<input
					id="board-id"
					type="text"
					bind:value={newBoardId}
					placeholder={isCreatingGroup ? 'e.g., all-lights' : 'e.g., bedroom-lights'}
					class="form-input"
				/>
			</div>

			{#if isCreatingGroup}
				<!-- Member selection for groups -->
				<div class="form-group">
					<label>Select Boards:</label>
					<div style="max-height: 200px; overflow-y: auto; border: 1px solid #444; border-radius: 4px; padding: 0.5rem;">
						{#each boards.filter(b => !b.isGroup) as board}
							<label style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; cursor: pointer; border-radius: 4px;" class:selected={selectedMemberIds.includes(board.id)}>
								<input
									type="checkbox"
									value={board.id}
									checked={selectedMemberIds.includes(board.id)}
									on:change={(e) => {
										if (e.currentTarget.checked) {
											selectedMemberIds = [...selectedMemberIds, board.id];
										} else {
											selectedMemberIds = selectedMemberIds.filter(id => id !== board.id);
										}
									}}
								/>
								<span>{board.id} ({board.ip})</span>
							</label>
						{/each}
					</div>
				</div>
			{:else}
				<!-- IP field for regular boards -->
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
			{/if}
			<div class="form-actions">
				<button class="submit-btn" on:click={addBoard}>Add Board</button>
				<button class="cancel-btn" on:click={() => (showAddForm = false)}>Cancel</button>
			</div>
		</div>
	{:else}
		<h1>WLED Control Panel</h1>

		<!-- Loopy Pro Test Button -->
		<!-- <div style="margin: 20px 0; padding: 20px; background: #2a2a2a; border-radius: 8px;">
			<h3 style="margin-top: 0;">Loopy Pro + LED Control</h3>
			<button
				class="loopy-btn"
				on:click={() => triggerLoopyAndLED('mikaels-bed', '/PlayStop/06', 255, 0, 255)}
			>
				▶ Play Track 6 + Purple Lights
			</button>
		</div> -->

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
									{#if board.isGroup}
										Group ({board.memberIds?.length || 0} boards)
									{:else}
										<span class="connection-dot {board.connected ? 'connected' : 'disconnected'}"></span>
										{board.ip}
									{/if}
								</p>
							</div>
							{#if board.connected}
								<label class="toggle-switch" on:click={(e) => e.stopPropagation()}>
									<input
										type="checkbox"
										checked={board.on}
										on:change={() => togglePower(board.id)}
									/>
									<span
										class="toggle-slider color-toggle"
										style={Array.isArray(board.color) && board.color.length === 3 ? `--board-color: rgb(${Number(board.color[0])}, ${Number(board.color[1])}, ${Number(board.color[2])})` : ''}
									></span>
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
										disabled={board.isGroup ? false : !board.connected}
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
										disabled={board.isGroup ? false : !board.connected}
										on:change={(e) => setBrightness(board.id, parseInt(e.currentTarget.value))}
										class="brightness-slider"
									/>
								</div>

								<div class="effects-section">
									<select
										value={board.effect}
										disabled={board.isGroup ? false : !board.connected}
										on:change={(e) => setEffect(board.id, parseInt(e.currentTarget.value))}
										class="effects-dropdown"
									>
										{#each effects as effect}
											<option value={effect.id}>{effect.name}</option>
										{/each}
									</select>
								</div>

								<div style="display: flex; gap: 10px; margin-top: 20px;">
									{#if !board.isGroup}
										<button class="edit-btn" on:click={() => openEditForm(board)}>
											Edit Board
										</button>
									{/if}
									<button class="delete-btn" on:click={() => deleteBoard(board.id)}>
										Delete {board.isGroup ? 'Group' : 'Board'}
									</button>
								</div>
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
		margin-top: 0;
		margin-bottom: 1.5rem;
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

	.edit-btn {
		flex: 1;
		padding: 0.75rem;
		border: none;
		border-radius: 6px;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		background: #1976d2;
		color: white;
	}

	.edit-btn:hover {
		background: #1565c0;
	}

	.edit-btn:active {
		transform: scale(0.98);
	}

	.delete-btn {
		flex: 1;
		padding: 0.75rem;
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
		padding: 0.75rem;
		gap: 0.75rem;
		transition: background 0.2s;
	}

	.board-header h2 {
		margin: 0 0 0.2rem 0;
		font-size: 1rem;
		color: #ffffff;
	}

	.ip-text {
		margin: 0;
		font-size: 0.75rem;
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
		padding: 0.75rem;
		border-top: 1px solid #333;
	}

	.color-section {
		display: flex;
		justify-content: center;
		margin-bottom: 1rem;
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

	.toggle-switch input:checked + .toggle-slider.color-toggle {
		background-color: var(--board-color, #4caf50);
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
