<script lang="ts">
	import { boards, boardsLoading, boardsError } from '$lib/store';
	import {
		setBoardPower,
		setBoardColor,
		setBoardBrightness,
		setBoardEffect,
		setBoardPreset,
		setBoardLedCount,
		resetBoardSegment,
		addBoard,
		updateBoard,
		deleteBoard as deleteBoardService,
		refreshGroups,
		fetchBoards,
	} from '$lib/boards-db';
	import { addGroup, deleteGroup, updateGroup } from '$lib/groups-db';
	import { WLED_EFFECTS } from '$lib/wled-effects';
	import ColorWheel from '$lib/ColorWheel.svelte';
	import type { BoardState } from '$lib/types';

	// WLED Presets (1-15 from presets.json)
	const WLED_PRESETS = [
		{ id: 0, name: 'No Preset' },
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
	let editBoardId = '';
	let editBoardIp = '';
	let editingBoardId = ''; // Original ID being edited
	let ledCountTimeout: number | null = null;
	let editingBoard: string | null = null; // Board currently being edited inline
	
	// Group editing state
	let editingGroupId = '';
	let editGroupName = '';
	let editGroupMembers: string[] = [];
	let editingGroup: string | null = null; // Group currently being edited

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

				// Check if group ID conflicts with existing boards or groups
				const existingBoardIds = $boards.map(b => b.id);
				if (existingBoardIds.includes(newBoardId.trim())) {
					alert(`A board or group with ID "${newBoardId.trim()}" already exists. Please choose a different name.`);
					return;
				}

				await addGroup(newBoardId, selectedMemberIds);
				await refreshGroups(); // Refresh groups to show the new group

				newBoardId = '';
				selectedMemberIds = [];
				isCreatingGroup = false;
				showAddForm = false;
			} else {
				if (!newBoardIp.trim()) {
					alert('Please enter an IP address');
					return;
				}

				// Check if board ID conflicts with existing boards or groups
				const existingBoardIds = $boards.map(b => b.id);
				if (existingBoardIds.includes(newBoardId.trim())) {
					alert(`A board or group with ID "${newBoardId.trim()}" already exists. Please choose a different name.`);
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

	

	function openEditForm(board: BoardState) {
		editingBoardId = board.id;
		editBoardId = board.id;
		editBoardIp = board.ip;
		editingBoard = board.id; // Set inline edit mode
	}

	function openEditGroupForm(group: BoardState) {
		editingGroupId = group.id;
		editGroupName = group.id;
		editGroupMembers = group.memberIds || [];
		editingGroup = group.id; // Set group edit mode
	}

	async function handleSyncPresets(boardId: string) {
		if (!confirm(`Load all presets to board "${boardId}"? This will overwrite existing presets on the board.`)) {
			return;
		}

		try {
			const response = await fetch(`${window.location.protocol}//${window.location.hostname}:3010/api/board/${boardId}/presets/sync`, {
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

	async function handleResetSegment(boardId: string) {
		if (!confirm(`Reset segment settings to defaults for board "${boardId}"? This will reset grouping, spacing, and offset to defaults.`)) {
			return;
		}

		try {
			await resetBoardSegment(boardId);
			alert(`Successfully reset segment for board "${boardId}"`);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to reset segment');
		}
	}

	function handleLedCountChange(boardId: string, value: number) {
		// Clear existing timeout
		if (ledCountTimeout !== null) {
			clearTimeout(ledCountTimeout);
		}

		// Set new timeout - only send after 15ms of no changes
		ledCountTimeout = setTimeout(() => {
			setBoardLedCount(boardId, value);
		}, 15) as unknown as number;
	}

	async function handleUpdateBoard() {
		if (!editBoardId.trim() || !editBoardIp.trim()) {
			alert('Board ID and IP are required');
			return;
		}

		try {
			await updateBoard(editingBoardId, editBoardId, editBoardIp);

			// Clear inline edit mode
			editingBoard = null;
			editBoardId = '';
			editBoardIp = '';
			editingBoardId = '';
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to update board');
		}
	}

	function cancelEdit() {
		editingBoard = null;
		editBoardId = '';
		editBoardIp = '';
		editingBoardId = '';
	}

	async function handleUpdateGroup() {
		if (!editGroupName.trim()) {
			alert('Group name is required');
			return;
		}

		if (editGroupMembers.length === 0) {
			alert('Please select at least one member board');
			return;
		}

		try {
			await updateGroup(editingGroupId, editGroupName, editGroupMembers);

			// Clear group edit mode
			editingGroup = null;
			editGroupName = '';
			editGroupMembers = [];
			editingGroupId = '';
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to update group');
		}
	}

	function cancelGroupEdit() {
		editingGroup = null;
		editGroupName = '';
		editGroupMembers = [];
		editingGroupId = '';
	}

	// Custom confirmation dialog state
	let showDeleteConfirm = false;
	let itemToDelete = '';
	let itemTypeToDelete = ''; // 'board' or 'group'

	async function handleDeleteGroup(groupId: string) {
		itemToDelete = groupId;
		itemTypeToDelete = 'group';
		showDeleteConfirm = true;
	}

	async function handleDeleteBoard(boardId: string) {
		itemToDelete = boardId;
		itemTypeToDelete = 'board';
		showDeleteConfirm = true;
	}

	async function confirmDelete() {
		showDeleteConfirm = false;
		
		try {
			if (itemTypeToDelete === 'group') {
				await deleteGroup(itemToDelete);
				await refreshGroups(); // Refresh only groups, not individual boards
			} else {
				await deleteBoardService(itemToDelete);
				await fetchBoards(); // Refresh all boards for individual board deletion
			}
		} catch (e) {
			alert(e instanceof Error ? e.message : `Failed to delete ${itemTypeToDelete}`);
		}
		
		itemToDelete = '';
		itemTypeToDelete = '';
	}

	function cancelDelete() {
		showDeleteConfirm = false;
		itemToDelete = '';
		itemTypeToDelete = '';
	}

</script>

<main>
	{#if showAddForm}
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
										on:change={() => setBoardPower(board.id, !board.on)}
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
								{#if editingBoard === board.id}
									<!-- Board Edit Form -->
									<div class="edit-form">
										<div class="form-group">
											<label for="edit-board-id-{board.id}">Board ID:</label>
											<input
												id="edit-board-id-{board.id}"
												type="text"
												bind:value={editBoardId}
												placeholder="e.g., bedroom-lights"
												class="form-input"
											/>
										</div>
										<div class="form-group">
											<label for="edit-board-ip-{board.id}">IP Address:</label>
											<input
												id="edit-board-ip-{board.id}"
												type="text"
												bind:value={editBoardIp}
												placeholder="e.g., 192.168.1.100"
												class="form-input"
											/>
										</div>
										<div class="form-actions">
											<button class="submit-btn" on:click={handleUpdateBoard}>Update</button>
											<button class="cancel-btn" on:click={cancelEdit}>Cancel</button>
										</div>
									</div>
								{:else if editingGroup === board.id}
									<!-- Group Edit Form -->
									<div class="edit-form">
										<div class="form-group">
											<label for="edit-group-name-{board.id}">Group Name</label>
											<input
												id="edit-group-name-{board.id}"
												type="text"
												bind:value={editGroupName}
												placeholder="e.g., upstairs-lights"
												class="form-input"
											/>
										</div>
										<div class="form-group">
											<label>Member Boards</label>
											<div class="member-grid">
												{#each $boards as board}
													{#if !board.isGroup}
														<label class="member-label">
															<input
																type="checkbox"
																bind:group={editGroupMembers}
																value={board.id}
																class="member-checkbox-input"
															/>
															<span class="member-name">{board.id}</span>
														</label>
													{/if}
												{/each}
											</div>
										</div>
										<div class="form-actions">
											<button class="submit-btn" on:click={handleUpdateGroup}>Update</button>
											<button class="cancel-btn" on:click={cancelGroupEdit}>Cancel</button>
										</div>
									</div>
								{:else}
									<!-- Normal Controls -->
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

									{#if !board.isGroup && board.ledCount !== undefined && board.maxLeds}
										<div class="led-count-wrapper">
											<div class="led-count-label">
												<span>LED Range</span>
												<span class="led-count-value">{board.ledCount} / {board.maxLeds}</span>
											</div>
											<input
												type="range"
												min="1"
												max={board.maxLeds}
												value={board.ledCount}
												disabled={!board.connected}
												on:input={(e) => handleLedCountChange(board.id, parseInt(e.currentTarget.value))}
												class="led-count-slider"
											/>
										</div>
									{/if}

									<div class="action-buttons">
										{#if !board.isGroup}
											<button class="action-btn action-btn-edit" on:click={() => openEditForm(board)}>
												Edit
											</button>
											<button class="action-btn action-btn-presets" on:click={() => handleSyncPresets(board.id)}>
												Presets
											</button>
											<button class="action-btn action-btn-reset" on:click={() => handleResetSegment(board.id)}>
												Reset
											</button>
										{:else}
											<button class="action-btn action-btn-edit" on:click={() => openEditGroupForm(board)}>
												Edit
											</button>
										{/if}
										{#if board.isGroup}
											<button class="action-btn action-btn-delete" on:click={() => handleDeleteGroup(board.id)}>
												Delete
											</button>
										{:else}
											<button class="action-btn action-btn-delete" on:click={() => handleDeleteBoard(board.id)}>
												Delete
											</button>
										{/if}
									</div>
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}

		<!-- Add Board Button - Always visible -->
		<button class="add-board-btn" on:click={() => (showAddForm = !showAddForm)}>
			Add Board
		</button>
	{/if}
</main>

<!-- Custom Delete Confirmation Dialog -->
{#if showDeleteConfirm}
	<div class="confirm-dialog-overlay" on:click={cancelDelete}>
		<div class="confirm-dialog" on:click|stopPropagation>
			<h3>Confirm Delete</h3>
			{#if itemTypeToDelete === 'group'}
				<p>Are you sure you want to delete group "{itemToDelete}"? This will not delete individual boards.</p>
			{:else}
				<p>Are you sure you want to delete board "{itemToDelete}"?</p>
			{/if}
			<div class="confirm-dialog-buttons">
				<button class="btn btn-cancel" on:click={cancelDelete}>Cancel</button>
				<button class="btn btn-delete" on:click={confirmDelete}>Delete</button>
			</div>
		</div>
	</div>
{/if}

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

	.action-buttons {
		display: flex;
		gap: 0.5rem;
		margin-top: 1.5rem;
	}

	.action-btn {
		flex: 1;
		padding: 0.6rem 0.75rem;
		background: #252525;
		border: 1px solid #333;
		border-radius: 6px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.action-btn-edit {
		color: #64b5f6;
	}

	.action-btn-edit:hover {
		background: #2d2d2d;
		border-color: #64b5f6;
	}

	.action-btn-presets {
		color: #81c784;
	}

	.action-btn-presets:hover {
		background: #2d2d2d;
		border-color: #81c784;
	}

	.action-btn-reset {
		color: #ffb74d;
	}

	.action-btn-reset:hover {
		background: #2d2620;
		border-color: #ffb74d;
	}

	.action-btn-delete {
		color: #e57373;
	}

	.action-btn-delete:hover {
		background: #2d2020;
		border-color: #e57373;
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

	.led-count-wrapper {
		margin-bottom: 1.5rem;
	}

	.led-count-label {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
		font-size: 0.875rem;
		color: #9ca3af;
	}

	.led-count-value {
		color: #e5e5e5;
		font-weight: 500;
	}

	.led-count-slider {
		width: 100%;
		height: 8px;
		border-radius: 4px;
		background: linear-gradient(to right, #444, #a855f7);
		outline: none;
		-webkit-appearance: none;
		cursor: pointer;
	}

	.led-count-slider:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.led-count-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #4caf50;
		cursor: pointer;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
	}

	.led-count-slider::-moz-range-thumb {
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #4caf50;
		cursor: pointer;
		border: none;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
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

	.edit-form {
		padding: 0.75rem;
	}

	.edit-form .form-group {
		margin-bottom: 1rem;
	}

	.edit-form .form-group label {
		display: block;
		margin-bottom: 0.5rem;
		color: #e0e0e0;
		font-weight: 500;
	}

	.edit-form .form-input {
		width: 100%;
		padding: 0.75rem;
		background: #333;
		color: #e0e0e0;
		border: 1px solid #444;
		border-radius: 4px;
		font-size: 1rem;
		box-sizing: border-box;
	}

	.edit-form .form-input:focus {
		outline: none;
		border-color: #4caf50;
	}

	.edit-form .form-input::placeholder {
		color: #888;
	}

	.edit-form .form-actions {
		display: flex;
		gap: 1rem;
		margin-top: 1rem;
	}

	.edit-form .submit-btn,
	.edit-form .cancel-btn {
		flex: 1;
		padding: 0.6rem 0.75rem;
		background: #252525;
		border: 1px solid #333;
		border-radius: 6px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
		color: #e0e0e0;
	}

	.edit-form .submit-btn {
		color: #64b5f6;
		border-color: #64b5f6;
	}

	.edit-form .submit-btn:hover {
		background: #2d2d2d;
		border-color: #64b5f6;
	}

	.edit-form .cancel-btn {
		color: #e57373;
		border-color: #e57373;
	}

	.edit-form .cancel-btn:hover {
		background: #2d2020;
		border-color: #e57373;
	}

	.label-icon {
		margin-right: 0.5rem;
		font-size: 0.9rem;
	}

	.member-count {
		margin-left: auto;
		background: #4a5568;
		color: #e2e8f0;
		padding: 0.2rem 0.5rem;
		border-radius: 12px;
		font-size: 0.75rem;
		font-weight: 500;
	}

	.member-grid {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		max-height: 180px;
		overflow-y: auto;
		overflow-x: hidden;
	}

	.member-label {
		display: flex;
		align-items: center;
		padding: 0.2rem 0.5rem;
		cursor: pointer;
		gap: 0.5rem;
		width: 100%;
		white-space: nowrap;
	}

	.member-checkbox-input {
		width: 16px;
		height: 16px;
		appearance: none;
		border: 2px solid #4a5568;
		border-radius: 3px;
		background: #1a202c;
		cursor: pointer;
		position: relative;
		transition: all 0.2s ease;
		vertical-align: middle;
		margin-top: 0;
		margin-bottom: 0;
	}

	.member-checkbox-input:checked {
		background: #2d3748;
		border-color: #4caf50;
	}

	.member-checkbox-input:checked::after {
		content: '✓';
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		color: #4caf50;
		font-size: 10px;
		font-weight: bold;
	}

	.member-checkbox-input:hover {
		border-color: #718096;
	}

	.member-name {
		font-weight: 500;
		color: #f7fafc;
		font-size: 0.9rem;
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.member-status {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.8rem;
		color: #a0aec0;
		flex-shrink: 0;
	}

	.status-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
	}

	.status-dot.connected {
		background-color: #48bb78;
		box-shadow: 0 0 4px rgba(72, 187, 120, 0.6);
	}

	.status-dot.disconnected {
		background-color: #f56565;
		box-shadow: 0 0 4px rgba(245, 101, 101, 0.6);
	}

	.btn-icon {
		margin-right: 0.5rem;
	}

	/* Custom Confirmation Dialog */
	.confirm-dialog-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background-color: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.confirm-dialog {
		background-color: #2a2a2a;
		border: 1px solid #444;
		border-radius: 8px;
		padding: 2rem;
		max-width: 400px;
		width: 90%;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
	}

	.confirm-dialog h3 {
		margin: 0 0 1rem 0;
		color: #ff6b6b;
		font-size: 1.2rem;
	}

	.confirm-dialog p {
		margin: 0 0 1.5rem 0;
		color: #e0e0e0;
		line-height: 1.4;
	}

	.confirm-dialog-buttons {
		display: flex;
		gap: 1rem;
		justify-content: flex-end;
	}

	.btn {
		padding: 0.5rem 1rem;
		border: none;
		border-radius: 4px;
		cursor: pointer;
		font-size: 0.9rem;
		transition: background-color 0.2s;
	}

	.btn-cancel {
		background-color: #555;
		color: #e0e0e0;
	}

	.btn-cancel:hover {
		background-color: #666;
	}

	.btn-delete {
		background-color: #ff6b6b;
		color: white;
	}

	.btn-delete:hover {
		background-color: #ff5252;
	}
</style>
