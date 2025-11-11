<script lang="ts">
	import { onMount } from 'svelte';
	import { boards, boardsLoading, boardsError, presets } from '$lib/store';
	import {
		setBoardPower,
		setBoardColor,
		setBoardBrightness,
		setBoardEffect,
		setBoardSpeed,
		setBoardIntensity,
		setBoardPreset,
		setBoardLedCount,
		setBoardTransition,
		resetBoardSegment,
		addBoard,
		updateBoard,
		deleteBoard as deleteBoardService,
		refreshGroups,
		fetchBoards,
		fetchPresets,
		syncPresetsToBoard,
	} from '$lib/boards-db';
	import { addGroup, deleteGroup, updateGroup } from '$lib/groups-db';
	import { WLED_EFFECTS } from '$lib/wled-effects';
	import { API_URL } from '$lib/api';
	import ColorWheel from '$lib/ColorWheel.svelte';
	import type { BoardState, WledPreset } from '$lib/types';

	// Per-board presets loaded from API
	let boardPresets: { [boardId: string]: any[] } = {};
	let boardPresetsLoading: { [boardId: string]: boolean } = {};

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
	let transitionTimeout: number | null = null;
	let editingBoard: string | null = null; // Board currently being edited inline
	
	// Group editing state
	let editingGroupId = '';
	let editGroupName = '';
	let editGroupMembers: string[] = [];
	let editingGroup: string | null = null; // Group currently being edited

	// Save preset state
	let savingPresetForBoard: string | null = null; // Board ID showing save preset form
	let newPresetName = '';

	// Sync presets state
	let syncingBoard: string | null = null;

	// Sync presets handler
	async function handleSyncPresets(boardId: string) {
		const board = $boards.find(b => b.id === boardId);
		if (!board) return;

		syncingBoard = boardId;

		try {
			if (board.isGroup) {
				// Sync presets to all group members
				const response = await fetch(`${API_URL}/group/${boardId}/presets/sync`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' }
				});

				if (!response.ok) {
					throw new Error(`Failed to sync group presets: ${response.statusText}`);
				}

				const result = await response.json();
				console.log('Group sync result:', result);

				// Refresh presets for all member boards
				if (board.memberIds) {
					for (const memberId of board.memberIds) {
						const memberBoard = $boards.find(b => b.id === memberId);
						if (memberBoard && !memberBoard.isGroup) {
							await fetchBoardPresets(memberId, memberBoard.ip);
						}
					}
				}

				alert(`Successfully synced ${result.total_successful_syncs} presets to group members`);
			} else {
				// Sync presets to individual board
				const result = await syncPresetsToBoard(boardId);
				if (result.success) {
					// Refresh presets for this board after syncing
					await fetchBoardPresets(boardId, board.ip);
				} else {
					throw new Error(result.message);
				}
			}
		} catch (error) {
			console.error('Error syncing presets:', error);
			alert(error instanceof Error ? error.message : 'Failed to sync presets');
		} finally {
			syncingBoard = null;
		}
	}

	// Fetch presets for a specific board from its WLED API
	async function fetchBoardPresets(boardId: string, boardIp: string) {
		if (boardPresetsLoading[boardId]) return; // Already loading

		boardPresetsLoading = { ...boardPresetsLoading, [boardId]: true };
		try {
			const response = await fetch(`${API_URL}/board/${boardId}/presets`);
			if (response.ok) {
				const presetsData = await response.json();
				// Convert WLED presets format to our format
				const presets = [];
				if (presetsData && Object.keys(presetsData).length > 0) {
					for (const [slot, preset] of Object.entries(presetsData)) {
						if (preset && preset.n) { // Only include presets with names
							presets.push({
								wled_slot: parseInt(slot),
								name: preset.n,
								data: preset
							});
						}
					}
				}
				boardPresets = { ...boardPresets, [boardId]: presets };
			} else {
				// Only log error if it's not a gateway error (board unreachable/no presets)
				if (response.status !== 502) {
					const errorText = await response.text();
					console.error(`Failed to fetch presets for board ${boardId}:`, response.status, errorText);
				}
				// Set empty array - board has no presets or is unreachable
				boardPresets = { ...boardPresets, [boardId]: [] };
			}
		} catch (error) {
			console.error(`Error fetching presets for board ${boardId}:`, error);
			boardPresets = { ...boardPresets, [boardId]: [] };
		} finally {
			boardPresetsLoading = { ...boardPresetsLoading, [boardId]: false };
		}
	}

	// Fetch presets for all boards when they load
	$: if ($boards && $boards.length > 0) {
		for (const board of $boards) {
			if (!board.isGroup && board.connected && !boardPresets[board.id] && !boardPresetsLoading[board.id]) {
				fetchBoardPresets(board.id, board.ip);
			}
		}
	}

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

	function openSavePresetForm(board: BoardState) {
		savingPresetForBoard = board.id;
		newPresetName = '';
	}

	function cancelSavePreset() {
		savingPresetForBoard = null;
		newPresetName = '';
	}

	async function handleSavePreset(boardId: string) {
		if (!newPresetName.trim()) {
			return;
		}

		try {
			// Get current board state
			const board = $boards.find(b => b.id === boardId);
			if (!board) {
				throw new Error('Board not found');
			}

			const response = await fetch(`${API_URL}/presets`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					name: newPresetName.trim(),
					wled_slot: 0, // Let server auto-assign
					board_id: boardId,
					state: {
						on: board.on,
						brightness: board.brightness,
						color: board.color,
						effect: board.effect,
						speed: board.speed,
						intensity: board.intensity
					}
				}),
			});

			if (!response.ok) {
				throw new Error('Failed to save preset');
			}

			const preset = await response.json();
			console.log('Preset saved with slot:', preset.wled_slot);

			// Apply the preset to the current board immediately
			await handleLoadPreset(boardId, preset.wled_slot);

			// Refresh presets list
			await fetchPresets();

			// Refresh board presets to show the new preset
			await fetchBoardPresets(boardId, board.ip);

			cancelSavePreset();
		} catch (error) {
			console.error('Error saving preset:', error);
			alert(error instanceof Error ? error.message : 'Failed to save preset');
		}
	}

	async function handleLoadPreset(boardId: string, presetSlot: number) {
		try {
			// Check if this is a group or individual board
			const board = $boards.find(b => b.id === boardId);
			if (!board) {
				throw new Error('Board/Group not found');
			}

			// Use appropriate endpoint for groups vs boards
			const endpoint = board.isGroup
				? `${API_URL}/group/${boardId}/preset`
				: `${API_URL}/board/${boardId}/preset`;

			const response = await fetch(endpoint, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ preset: presetSlot, transition: 0 })
			});

			if (!response.ok) {
				throw new Error(`Failed to load preset: ${response.statusText}`);
			}
		} catch (error) {
			console.error('Error loading preset:', error);
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

	function handleTransitionChange(boardId: string, value: number) {
		// Clear existing timeout
		if (transitionTimeout !== null) {
			clearTimeout(transitionTimeout);
		}

		// Set new timeout - only send after 15ms of no changes
		transitionTimeout = setTimeout(() => {
			setBoardTransition(boardId, value);
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
							{#if board.connected}
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
								{:else if savingPresetForBoard === board.id}
									<!-- Save Preset Form -->
									<div class="edit-form">
										<div class="form-group">
											<label for="preset-name-{board.id}">Preset Name:</label>
											<input
												id="preset-name-{board.id}"
												type="text"
												bind:value={newPresetName}
												placeholder="e.g., Relaxing Blue"
												class="form-input"
												on:keydown={(e) => e.key === 'Enter' && handleSavePreset(board.id)}
											/>
										</div>
										<div class="form-actions">
											<button class="submit-btn" on:click={() => handleSavePreset(board.id)}>Save</button>
											<button class="cancel-btn" on:click={cancelSavePreset}>Cancel</button>
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
										{#if board.isGroup}
											<!-- Groups show server presets -->
											<div class="preset-container">
												{#if $presets && $presets.length > 0}
													<select
														value=""
														on:change={(e) => {
															const value = e.currentTarget.value;
															if (value) {
																handleLoadPreset(board.id, parseInt(value));
																e.currentTarget.value = ''; // Reset dropdown
															}
														}}
														class="preset-dropdown"
													>
														<option value="">Server Presets ({$presets.length})</option>
														{#each $presets.sort((a, b) => a.name.localeCompare(b.name)) as preset}
															<option value={preset.id}>{preset.name}</option>
														{/each}
													</select>
												{:else}
													<select disabled class="preset-dropdown no-presets">
														<option>No server presets available</option>
													</select>
												{/if}
												<button
													class="sync-presets-btn prominent"
													on:click={() => handleSyncPresets(board.id)}
													disabled={syncingBoard === board.id}
												>
													{#if syncingBoard === board.id}
														Syncing...
													{:else}
														Sync Presets
													{/if}
												</button>
											</div>
										{:else if !board.connected}
											<select disabled class="preset-dropdown">
												<option>Board disconnected</option>
											</select>
										{:else if boardPresetsLoading[board.id]}
											<select disabled class="preset-dropdown">
												<option>Loading presets...</option>
											</select>
										{:else if boardPresets[board.id] && boardPresets[board.id].length > 0}
											<select
												value=""
												on:change={(e) => {
													const value = e.currentTarget.value;
													if (value) {
														handleLoadPreset(board.id, parseInt(value));
														e.currentTarget.value = ''; // Reset dropdown
													}
												}}
												class="preset-dropdown"
											>
												<option value="">Presets ({boardPresets[board.id].length})</option>
												{#each boardPresets[board.id].sort((a, b) => a.name.localeCompare(b.name)) as preset}
													<option value={preset.wled_slot}>{preset.name}</option>
												{/each}
											</select>
										{:else}
											<div class="no-presets-container">
												<select disabled class="preset-dropdown no-presets">
													<option>No presets - click Sync</option>
												</select>
												<button
													class="sync-presets-btn prominent"
													on:click={() => {
														fetchBoardPresets(board.id, board.ip);
														handleSyncPresets(board.id);
													}}
													disabled={syncingBoard === board.id}
												>
													{#if syncingBoard === board.id}
														Syncing...
													{:else}
														Sync Presets
													{/if}
												</button>
											</div>
										{/if}
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

									{#if !board.isGroup}
										<div class="effect-params-section">
											<label for="speed-{board.id}">Speed</label>
											<input
												id="speed-{board.id}"
												type="range"
												min="0"
												max="255"
												value={board.speed}
												disabled={!board.connected}
												on:change={(e) =>
													setBoardSpeed(board.id, parseInt(e.currentTarget.value))}
												class="effect-param-slider"
											/>
										</div>

										<div class="effect-params-section">
											<label for="intensity-{board.id}">Intensity</label>
											<input
												id="intensity-{board.id}"
												type="range"
												min="0"
												max="255"
												value={board.intensity}
												disabled={!board.connected}
												on:change={(e) =>
													setBoardIntensity(board.id, parseInt(e.currentTarget.value))}
												class="effect-param-slider"
											/>
										</div>
									{/if}

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

									{#if !board.isGroup}
										<div class="transition-wrapper">
											<div class="transition-label">
												<span>Transition</span>
												<span class="transition-value">{board.transition * 100}ms</span>
											</div>
											<input
												type="range"
												min="0"
												max="100"
												value={board.transition}
												disabled={!board.connected}
												on:input={(e) => handleTransitionChange(board.id, parseInt(e.currentTarget.value))}
												class="transition-slider"
											/>
										</div>
									{/if}

									<div class="action-buttons-column">
										{#if !board.isGroup}
											<div class="action-buttons-row">
												<button class="action-btn action-btn-save" on:click={() => openSavePresetForm(board)}>
													Save Preset
												</button>
												<button
													class="action-btn action-btn-presets"
													on:click={() => handleSyncPresets(board.id)}
													disabled={syncingBoard === board.id}
												>
													{syncingBoard === board.id ? 'Syncing...' : 'Sync Presets'}
												</button>
											</div>
											<div class="action-buttons-row">
												<button class="action-btn action-btn-edit" on:click={() => openEditForm(board)}>
													Edit
												</button>
												<button class="action-btn action-btn-delete" on:click={() => handleDeleteBoard(board.id)}>
													Delete
												</button>
											</div>
										{:else}
											<div class="action-buttons-row">
												<button class="action-btn action-btn-edit" on:click={() => openEditGroupForm(board)}>
													Edit
												</button>
												<button class="action-btn action-btn-delete" on:click={() => handleDeleteGroup(board.id)}>
													Delete
												</button>
											</div>
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

	.action-buttons-column {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin-top: 1.5rem;
	}

	.action-buttons-row {
		display: flex;
		gap: 0.5rem;
	}

	.action-btn {
		width: 100%;
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

	.action-btn-save {
		color: #4caf50;
	}

	.action-btn-save:hover {
		background: #1e2d20;
		border-color: #4caf50;
	}

	.action-btn-presets {
		color: #81c784;
	}

	.action-btn-presets:hover {
		background: #2d2d2d;
		border-color: #81c784;
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

	.no-presets-container {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.preset-container {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.preset-dropdown.no-presets {
		background: #444;
		color: #999;
		border-color: #555;
	}

	.sync-presets-btn.prominent {
		background: linear-gradient(135deg, #4caf50, #45a049);
		color: white;
		border: none;
		padding: 0.75rem 1rem;
		border-radius: 6px;
		font-size: 0.9rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s ease;
		text-align: center;
		box-shadow: 0 2px 4px rgba(76, 175, 80, 0.3);
	}

	.sync-presets-btn.prominent:hover:not(:disabled) {
		background: linear-gradient(135deg, #45a049, #3d8b40);
		transform: translateY(-1px);
		box-shadow: 0 4px 8px rgba(76, 175, 80, 0.4);
	}

	.sync-presets-btn.prominent:active:not(:disabled) {
		transform: translateY(0);
		box-shadow: 0 2px 4px rgba(76, 175, 80, 0.3);
	}

	.sync-presets-btn.prominent:disabled {
		opacity: 0.6;
		cursor: not-allowed;
		transform: none;
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
		margin-bottom: 0.5rem;
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

	.transition-wrapper {
		margin-bottom: 1.5rem;
	}

	.transition-label {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
		font-size: 0.875rem;
		color: #9ca3af;
	}

	.transition-value {
		color: #e5e5e5;
		font-weight: 500;
	}

	.transition-slider {
		width: 100%;
		height: 8px;
		border-radius: 4px;
		background: linear-gradient(to right, #444, #ff9800);
		outline: none;
		-webkit-appearance: none;
		cursor: pointer;
	}

	.transition-slider:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.transition-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #ff9800;
		cursor: pointer;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
	}

	.transition-slider::-moz-range-thumb {
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #ff9800;
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

	.effect-params-section {
		display: flex;
		align-items: center;
		gap: 12px;
		margin: 12px 0;
	}

	.effect-params-section label {
		min-width: 70px;
		font-size: 0.9rem;
		color: #aaa;
	}

	.effect-param-slider {
		flex: 1;
		-webkit-appearance: none;
		appearance: none;
		height: 6px;
		border-radius: 3px;
		background: #333;
		outline: none;
		transition: background 0.2s;
	}

	.effect-param-slider:disabled {
		opacity: 0.3;
		cursor: not-allowed;
	}

	.effect-param-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: #2196f3;
		cursor: pointer;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
	}

	.effect-param-slider::-moz-range-thumb {
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: #2196f3;
		cursor: pointer;
		border: none;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
	}

	.error {
		color: #ff6b6b;
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
