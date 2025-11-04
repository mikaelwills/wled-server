<script lang="ts">
	import { boards, boardsLoading, boardsError } from '$lib/store';
	import {
		toggleBoardPower,
		setBoardColor,
		setBoardBrightness,
		setBoardEffect,
		setBoardPreset,
		addBoard,
		updateBoard,
		deleteBoard as deleteBoardService,
	} from '$lib/boards-db';
	import { addGroup, deleteGroup } from '$lib/groups-db';
	import { WLED_EFFECTS } from '$lib/wled-effects';
	import ColorWheel from '$lib/ColorWheel.svelte';
	import type { BoardState } from '$lib/types';

	// WLED Presets (1-15 from presets.json)
	const WLED_PRESETS = [
		{ id: 0, name: 'None (Manual Control)' },
		{ id: 1, name: 'Lightning Cyan' },
		{ id: 2, name: 'Lightning Cyan' },
		{ id: 3, name: 'Lightning Red' },
		{ id: 4, name: 'Lightning Green' },
		{ id: 5, name: 'Puddles Green' },
		{ id: 7, name: 'Puddles Cyan' },
		{ id: 8, name: 'Puddles Red' },
		{ id: 9, name: 'Candles' },
		{ id: 11, name: 'Puddles Pink' },
		{ id: 12, name: 'Wipe Cyan' },
		{ id: 13, name: 'Wipe White' },
		{ id: 14, name: 'Wipe Red' },
		{ id: 15, name: 'Wipe Green' },
	];

	// UI state (local to component)
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

	// Event handlers that call service functions
	function toggleExpanded(boardId: string) {
		expandedBoard = expandedBoard === boardId ? null : boardId;
	}

	async function handleAddBoard() {
		if (!newBoardId.trim()) {
			alert('Please enter an ID');
			return;
		}

		try {
			if (isCreatingGroup) {
				if (selectedMemberIds.length === 0) {
					alert('Please select at least one board for the group');
					return;
				}

				await addGroup(newBoardId, selectedMemberIds);

				newBoardId = '';
				selectedMemberIds = [];
				isCreatingGroup = false;
				showAddForm = false;
			} else {
				if (!newBoardIp.trim()) {
					alert('Please enter an IP address');
					return;
				}

				await addBoard(newBoardId, newBoardIp);

				newBoardId = '';
				newBoardIp = '';
				showAddForm = false;
			}
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to add board/group');
		}
	}

	async function handleDeleteBoard(boardId: string) {
		const board = $boards.find((b) => b.id === boardId);

		if (!confirm(`Are you sure you want to delete "${boardId}"?`)) {
			return;
		}

		try {
			if (board?.isGroup) {
				await deleteGroup(boardId);
			} else {
				await deleteBoardService(boardId);
			}
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to delete board/group');
		}
	}

	function openEditForm(board: BoardState) {
		editingBoardId = board.id;
		editBoardId = board.id;
		editBoardIp = board.ip;
		showEditForm = true;
	}

	async function handleSyncPresets(boardId: string) {
		if (!confirm(`Load all presets to board "${boardId}"? This will overwrite existing presets on the board.`)) {
			return;
		}

		try {
			const response = await fetch(`http://localhost:3010/board/${boardId}/presets/sync`, {
				method: 'POST'
			});

			if (!response.ok) {
				const errorText = await response.text();
				throw new Error(`Failed to sync presets: ${errorText}`);
			}

			const result = await response.json();
			alert(`Successfully loaded ${result.synced} of ${result.total} presets to board "${boardId}"`);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to sync presets');
		}
	}

	async function handleUpdateBoard() {
		if (!editBoardId.trim() || !editBoardIp.trim()) {
			alert('Board ID and IP are required');
			return;
		}

		try {
			await updateBoard(editingBoardId, editBoardId, editBoardIp);

			// Clear form
			showEditForm = false;
			editBoardId = '';
			editBoardIp = '';
			editingBoardId = '';
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to update board');
		}
	}

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
				<button class="submit-btn" on:click={handleUpdateBoard}>Update Board</button>
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
					<div
						style="max-height: 200px; overflow-y: auto; border: 1px solid #444; border-radius: 4px; padding: 0.5rem;"
					>
						{#each $boards.filter((b) => !b.isGroup) as board}
							<label
								style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; cursor: pointer; border-radius: 4px;"
								class:selected={selectedMemberIds.includes(board.id)}
							>
								<input
									type="checkbox"
									value={board.id}
									checked={selectedMemberIds.includes(board.id)}
									on:change={(e) => {
										if (e.currentTarget.checked) {
											selectedMemberIds = [...selectedMemberIds, board.id];
										} else {
											selectedMemberIds = selectedMemberIds.filter((id) => id !== board.id);
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
				<button class="submit-btn" on:click={handleAddBoard}>Add Board</button>
				<button class="cancel-btn" on:click={() => (showAddForm = false)}>Cancel</button>
			</div>
		</div>
	{:else}
		<h1>WLED Control Panel</h1>

		{#if $boardsLoading}
			<p>Loading boards...</p>
		{:else if $boardsError}
			<p class="error">Error: {$boardsError}</p>
		{:else if $boards.length === 0}
			<p>No boards configured. Add boards using the button below.</p>
		{:else}
			<div class="boards">
				{#each $boards as board}
					<div class="board-card">
						<div class="board-header">
							<div on:click={() => toggleExpanded(board.id)} style="flex: 1; cursor: pointer;">
								<h2>{board.id}</h2>
								<p class="ip-text">
									{#if board.isGroup}
										Group ({board.memberIds?.length || 0} boards)
									{:else}
										<span
											class="connection-dot {board.connected ? 'connected' : 'disconnected'}"
										></span>
										{board.ip}
									{/if}
								</p>
							</div>
							{#if board.connected || board.isGroup}
								<label class="toggle-switch" on:click={(e) => e.stopPropagation()}>
									<input
										type="checkbox"
										checked={board.on}
										on:change={() => toggleBoardPower(board.id)}
									/>
									<span
										class="toggle-slider color-toggle"
										style={Array.isArray(board.color) && board.color.length === 3
											? `--board-color: rgb(${Number(board.color[0])}, ${Number(board.color[1])}, ${Number(board.color[2])})`
											: ''}
									></span>
								</label>
							{/if}
							<span
								class="expand-icon"
								on:click={() => toggleExpanded(board.id)}
								style="cursor: pointer;"
							>
								{expandedBoard === board.id ? '▼' : '▶'}
							</span>
						</div>

						{#if expandedBoard === board.id}
							<div class="board-controls">
								<div class="color-section">
									<ColorWheel
										color={board.color}
										disabled={board.isGroup ? false : !board.connected}
										onColorChange={(r, g, b) => setBoardColor(board.id, r, g, b)}
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
										on:change={(e) =>
											setBoardBrightness(board.id, parseInt(e.currentTarget.value))}
										class="brightness-slider"
									/>
								</div>

								<div class="preset-section">
									<select
										value={0}
										disabled={board.isGroup ? false : !board.connected}
										on:change={(e) => setBoardPreset(board.id, parseInt(e.currentTarget.value))}
										class="preset-dropdown"
									>
										{#each WLED_PRESETS as preset}
											<option value={preset.id}>{preset.name}</option>
										{/each}
									</select>
								</div>

								<div class="effects-section">
									<select
										value={board.effect}
										disabled={board.isGroup ? false : !board.connected}
										on:change={(e) => setBoardEffect(board.id, parseInt(e.currentTarget.value))}
										class="effects-dropdown"
									>
										{#each WLED_EFFECTS as effect}
											<option value={effect.id}>{effect.name}</option>
										{/each}
									</select>
								</div>

								<div style="display: flex; gap: 10px; margin-top: 20px;">
									{#if !board.isGroup}
										<button class="edit-btn" on:click={() => openEditForm(board)}>
											Edit Board
										</button>
										<button class="sync-presets-btn" on:click={() => handleSyncPresets(board.id)}>
											Load Presets
										</button>
									{/if}
									<button class="delete-btn" on:click={() => handleDeleteBoard(board.id)}>
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

	@media (max-width: 768px) {
		main {
			padding: 0.75rem;
		}
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
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 0.9rem;
		font-weight: 400;
		cursor: pointer;
		transition: all 0.2s;
		background: #2a2a2a;
		color: #64b5f6;
	}

	.edit-btn:hover {
		background: #333;
		border-color: #64b5f6;
	}

	.edit-btn:active {
		transform: scale(0.98);
	}

	.sync-presets-btn {
		flex: 1;
		padding: 0.75rem;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 0.9rem;
		font-weight: 400;
		cursor: pointer;
		transition: all 0.2s;
		background: #2a2a2a;
		color: #81c784;
	}

	.sync-presets-btn:hover {
		background: #333;
		border-color: #81c784;
	}

	.sync-presets-btn:active {
		transform: scale(0.98);
	}

	.delete-btn {
		flex: 1;
		padding: 0.75rem;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 0.9rem;
		font-weight: 400;
		cursor: pointer;
		transition: all 0.2s;
		background: #2a2a2a;
		color: #e57373;
	}

	.delete-btn:hover {
		background: #333;
		border-color: #e57373;
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

	.preset-section {
		margin-bottom: 0.75rem;
	}

	.preset-dropdown {
		width: 100%;
		padding: 0.75rem;
		padding-right: 0.75rem;
		background: #333;
		color: #e0e0e0;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 1rem;
		cursor: pointer;
		outline: none;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23e0e0e0' d='M6 9L1 4h10z'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
	}

	.preset-dropdown:hover {
		background: #3a3a3a;
	}

	.preset-dropdown:focus {
		border-color: #4caf50;
	}

	.preset-dropdown:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.effects-section {
		margin-bottom: 1.5rem;
	}

	.effects-dropdown {
		width: 100%;
		padding: 0.75rem;
		padding-right: 0.75rem;
		background: #333;
		color: #e0e0e0;
		border: 1px solid #444;
		border-radius: 6px;
		font-size: 1rem;
		cursor: pointer;
		outline: none;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23e0e0e0' d='M6 9L1 4h10z'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
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
