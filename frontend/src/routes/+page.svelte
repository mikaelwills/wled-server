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

	// Live slider values (for real-time display while dragging)
	let liveSpeed: { [boardId: string]: number } = {};
	let liveIntensity: { [boardId: string]: number } = {};

	// UI state (local to component)
	let expandedBoard: string | null = null;
	let showAddForm = false;
	let newBoardId = '';
	let newBoardIp = '';
	let isCreatingGroup = false;
	let selectedMemberIds: string[] = [];
	let newGroupUniverse: string = ''; // User-specified universe (empty = auto-assign)
	let editBoardId = '';
	let editBoardIp = '';
	let editBoardLedCount = '';
	let editBoardUniverse = '';
	let editingBoardId = ''; // Original ID being edited
	let ledCountTimeout: number | null = null;
	let transitionTimeout: number | null = null;
	let editingBoard: string | null = null; // Board currently being edited inline
	
	// Group editing state
	let editingGroupId = '';
	let editGroupName = '';
	let editGroupMembers: string[] = [];
	let editGroupUniverse: string = ''; // User-specified universe for editing
	let editingGroup: string | null = null; // Group currently being edited

	// Save preset state
	let savingPresetForBoard: string | null = null; // Board ID showing save preset form
	let newPresetName = '';

	// Selected preset per board (for Update Preset button)
	let selectedPreset: { [boardId: string]: number } = {};

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

				// Check for failures and build detailed message
				const failures = result.member_results?.filter((r: any) => r.error) || [];
				const successes = result.member_results?.filter((r: any) => !r.error) || [];

				// Refresh presets for all member boards
				if (board.memberIds) {
					for (const memberId of board.memberIds) {
						const memberBoard = $boards.find(b => b.id === memberId);
						if (memberBoard && !memberBoard.isGroup) {
							await fetchBoardPresets(memberId, memberBoard.ip);
						}
					}
				}

				// Show detailed sync results
				let message = `Synced ${result.total_successful_syncs} presets to ${successes.length}/${board.memberIds?.length || 0} boards`;

				if (failures.length > 0) {
					message += `\n\n⚠️ ${failures.length} board(s) failed:`;
					failures.forEach((f: any, i: number) => {
						const memberBoard = board.memberIds?.[f.board_index];
						message += `\n- ${memberBoard || 'Unknown'}: ${f.error}`;
					});
				}

				alert(message);
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

				const universe = newGroupUniverse && newGroupUniverse.toString().trim()
					? parseInt(newGroupUniverse.toString())
					: undefined;
				await addGroup(newBoardId, selectedMemberIds, universe);
				await refreshGroups(); // Refresh groups to show the new group

				newBoardId = '';
				selectedMemberIds = [];
				newGroupUniverse = '';
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
		editBoardLedCount = board.ledCount?.toString() || '';
		editBoardUniverse = board.universe?.toString() || '';
		editingBoard = board.id;
	}

	function openEditGroupForm(group: BoardState) {
		editingGroupId = group.id;
		editGroupName = group.id;
		editGroupMembers = group.memberIds || [];
		editGroupUniverse = group.universe?.toString() || ''; // Populate universe for editing
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

			// Refresh global presets list first
			await fetchPresets();

			// Sync presets to the board so the new preset is available
			await syncPresetsToBoard(boardId);

			// Refresh board presets to show the new preset
			await fetchBoardPresets(boardId, board.ip);

			cancelSavePreset();
		} catch (error) {
			console.error('Error saving preset:', error);
			alert(error instanceof Error ? error.message : 'Failed to save preset');
		}
	}

	async function handleUpdatePreset(boardId: string) {
		const presetSlot = selectedPreset[boardId];
		if (!presetSlot) {
			alert('Please select a preset to update');
			return;
		}

		try {
			// Get current board state
			const board = $boards.find(b => b.id === boardId);
			if (!board) {
				throw new Error('Board not found');
			}

			// Get the preset name from presets list
			const preset = $presets.find(p => p.id === presetSlot);
			if (!preset) {
				throw new Error('Preset not found');
			}

			const response = await fetch(`${API_URL}/presets/${presetSlot}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					name: preset.name, // Keep existing name
					wled_slot: presetSlot,
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
				throw new Error('Failed to update preset');
			}

			console.log(`✓ Updated preset "${preset.name}" (slot ${presetSlot})`);
			alert(`Updated preset: ${preset.name}`);

			// Refresh presets list
			await fetchPresets();
		} catch (error) {
			console.error('Error updating preset:', error);
			alert(error instanceof Error ? error.message : 'Failed to update preset');
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
			const ledCount = editBoardLedCount ? parseInt(editBoardLedCount) : undefined;
			const universe = editBoardUniverse ? parseInt(editBoardUniverse) : undefined;
			await updateBoard(editingBoardId, editBoardId, editBoardIp, ledCount, universe);

			editingBoard = null;
			editBoardId = '';
			editBoardIp = '';
			editBoardLedCount = '';
			editBoardUniverse = '';
			editingBoardId = '';
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to update board');
		}
	}

	function cancelEdit() {
		editingBoard = null;
		editBoardId = '';
		editBoardIp = '';
		editBoardLedCount = '';
		editBoardUniverse = '';
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
			const universe = editGroupUniverse && editGroupUniverse.toString().trim()
				? parseInt(editGroupUniverse.toString())
				: undefined;
			await updateGroup(editingGroupId, editGroupName, editGroupMembers, universe);
			await refreshGroups(); // Refresh groups to show updated universe

			// Clear group edit mode
			editingGroup = null;
			editGroupName = '';
			editGroupMembers = [];
			editGroupUniverse = '';
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
			<div class="form-group">
				<div style="display: flex; gap: 0.75rem;">
					<div style="flex: 1;">
						<label for="board-id" style="display: block; margin-bottom: 0.5rem;">{isCreatingGroup ? 'Group Name' : 'Board Name'}:</label>
						<input
							id="board-id"
							type="text"
							bind:value={newBoardId}
							placeholder={isCreatingGroup ? 'e.g., all-lights' : 'e.g., bedroom-lights'}
							class="form-input"
						/>
					</div>
					<div>
						<label style="display: block; margin-bottom: 0.5rem; color: #e0e0e0; font-weight: 500;">Group</label>
						<button
							type="button"
							class="group-toggle-btn"
							class:active={isCreatingGroup}
							on:click={() => isCreatingGroup = !isCreatingGroup}
						>
							{#if isCreatingGroup}
								<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
									<polyline points="20 6 9 17 4 12"></polyline>
								</svg>
							{/if}
						</button>
					</div>
				</div>
			</div>

			{#if isCreatingGroup}
				<!-- Member selection for groups -->
				<div class="form-group">
					<div class="board-selection-grid">
						{#each $boards.filter((b) => !b.isGroup) as board}
							<label class="board-selection-item">
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
								<span class="board-selection-name">{board.id}</span>
								<span class="board-selection-ip">{board.ip}</span>
							</label>
						{/each}
					</div>
				</div>
				<!-- Universe field for groups -->
				<div class="form-group">
					<label for="group-universe">E1.31 Universe (optional):</label>
					<input
						id="group-universe"
						type="number"
						bind:value={newGroupUniverse}
						placeholder="Auto-assign if empty"
						min="1"
						max="63999"
						class="form-input"
					/>
					<small style="color: #888; font-size: 0.85rem;">Leave empty to auto-assign universe number</small>
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
					<div class="board-card" class:expanded={expandedBoard === board.id}>
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
										<div class="form-row">
											<div class="form-group half">
												<label for="edit-board-leds-{board.id}">LED Count:</label>
												<input
													id="edit-board-leds-{board.id}"
													type="number"
													bind:value={editBoardLedCount}
													placeholder="60"
													class="form-input"
												/>
											</div>
											<div class="form-group half">
												<label for="edit-board-universe-{board.id}">Universe:</label>
												<input
													id="edit-board-universe-{board.id}"
													type="number"
													bind:value={editBoardUniverse}
													placeholder="1"
													class="form-input"
												/>
											</div>
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
										<div class="form-group">
											<label for="edit-group-universe-{board.id}">E1.31 Universe (optional)</label>
											<input
												id="edit-group-universe-{board.id}"
												type="number"
												bind:value={editGroupUniverse}
												placeholder="Auto-assign if empty"
												min="1"
												max="63999"
												class="form-input"
											/>
											<small style="color: #888; font-size: 0.85rem;">Leave empty to auto-assign universe number</small>
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
										{#if !board.isGroup}
											<a
												href="http://{board.ip}"
												target="_blank"
												rel="noopener noreferrer"
												class="wled-gui-btn"
												title="Open WLED GUI"
											>
												<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
													<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
													<polyline points="15 3 21 3 21 9"></polyline>
													<line x1="10" y1="14" x2="21" y2="3"></line>
												</svg>
											</a>
										{/if}
										<div class="color-wheel-container">
											<ColorWheel
												color={board.color}
												disabled={board.isGroup ? false : !board.connected}
												onColorChange={(r, g, b) => setBoardColor(board.id, r, g, b)}
											/>
										</div>
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
														bind:value={selectedPreset[board.id]}
														on:change={(e) => {
															const value = e.currentTarget.value;
															if (value) {
																handleLoadPreset(board.id, parseInt(value));
															}
														}}
														class="preset-dropdown"
													>
														<option value="">Server Presets ({$presets.length})</option>
														{#each $presets.sort((a, b) => a.name.localeCompare(b.name)) as preset}
															<option value={preset.id}>{preset.name}</option>
														{/each}
													</select>
													{#if selectedPreset[board.id]}
														<button
															class="update-preset-btn"
															on:click={() => handleUpdatePreset(board.id)}
															title="Update selected preset with current board state"
														>
															Update
														</button>
													{/if}
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
											<div class="preset-container">
												<select
													bind:value={selectedPreset[board.id]}
													on:change={(e) => {
														const value = e.currentTarget.value;
														if (value) {
															handleLoadPreset(board.id, parseInt(value));
														}
													}}
													class="preset-dropdown"
												>
													<option value="">Presets ({boardPresets[board.id].length})</option>
													{#each boardPresets[board.id].sort((a, b) => a.name.localeCompare(b.name)) as preset}
														<option value={preset.wled_slot}>{preset.name}</option>
													{/each}
												</select>
												{#if selectedPreset[board.id]}
													<button
														class="update-preset-btn"
														on:click={() => handleUpdatePreset(board.id)}
														title="Update selected preset with current board state"
													>
														Update
													</button>
												{/if}
											</div>
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
											<label for="speed-{board.id}">Speed <span class="param-value">{liveSpeed[board.id] ?? board.speed ?? 0}</span></label>
											<input
												id="speed-{board.id}"
												type="range"
												min="0"
												max="255"
												value={board.speed}
												disabled={!board.connected}
												on:input={(e) => liveSpeed = { ...liveSpeed, [board.id]: parseInt(e.currentTarget.value) }}
												on:change={(e) =>
													setBoardSpeed(board.id, parseInt(e.currentTarget.value))}
												class="effect-param-slider"
											/>
										</div>

										<div class="effect-params-section">
											<label for="intensity-{board.id}">Intensity <span class="param-value">{liveIntensity[board.id] ?? board.intensity ?? 0}</span></label>
											<input
												id="intensity-{board.id}"
												type="range"
												min="0"
												max="255"
												value={board.intensity}
												disabled={!board.connected}
												on:input={(e) => liveIntensity = { ...liveIntensity, [board.id]: parseInt(e.currentTarget.value) }}
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
	main {
		padding: 2rem;
		max-width: 1200px;
		margin: 0 auto;
	}

	@media (max-width: 768px) {
		main {
			padding: 0 0.75rem 0.75rem 0.75rem;
		}
	}

	.add-board-btn {
		width: 100%;
		padding: 0.75rem 1.5rem;
		margin-top: 2rem;
		background: #0c0c0c;
		color: #555;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		font-size: 1rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
	}

	.add-board-btn:hover {
		background: #0c0e14;
		color: #fff;
		border-color: rgba(255, 255, 255, 0.1);
	}

	.add-board-fullscreen {
		max-width: 500px;
		margin: 0 auto;
		padding: 2rem;
	}

	.form-group {
		margin-bottom: 1rem;
	}

	.form-group label {
		display: block;
		margin-bottom: 0.5rem;
		color: #888;
		font-weight: 500;
	}

	.form-input {
		width: 100%;
		padding: 0.75rem;
		background: #0f0f0f;
		color: #e5e5e5;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		font-size: 1rem;
		box-sizing: border-box;
	}

	.form-input:focus {
		outline: none;
		border-color: #333;
	}

	.group-toggle-btn {
		width: 60px;
		height: 44px;
		padding: 0;
		background: #0f0f0f;
		color: #888;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.group-toggle-btn:hover {
		background: #141414;
		border-color: #222;
	}

	.group-toggle-btn.active {
		background: #111;
		border-color: #333;
		color: #fff;
	}

	.form-input::placeholder {
		color: #444;
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
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		font-size: 1rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.submit-btn {
		background: transparent;
		color: #666;
	}

	.submit-btn:hover {
		background: #111;
		color: #fff;
		border-color: #222;
	}

	.cancel-btn {
		background: transparent;
		color: #555;
	}

	.cancel-btn:hover {
		background: #1a1212;
		color: #c44;
		border-color: #331a1a;
	}

	.board-selection-grid {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin-top: 0.75rem;
	}

	.board-selection-item {
		display: flex !important;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem;
		cursor: pointer;
		transition: background 0.2s;
		border-radius: 4px;
	}

	.board-selection-item:hover {
		background: #111;
	}

	.board-selection-item input[type="checkbox"] {
		width: 18px;
		height: 18px;
		cursor: pointer;
		flex-shrink: 0;
		margin: 0;
		padding: 0;
	}

	.board-selection-item input[type="checkbox"]:checked + .board-selection-name {
		color: #fff;
	}

	.board-selection-name {
		font-weight: 500;
		color: #888;
		display: inline;
	}

	.board-selection-ip {
		font-size: 0.9rem;
		color: #444;
		margin-left: auto;
		display: inline;
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
		background: #0b0d14;
		border: 1px solid rgba(56, 89, 138, 0.2);
		border-radius: 6px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.action-btn-edit {
		background: rgba(56, 89, 138, 0.15);
		color: #8bb8de;
		border-color: rgba(56, 89, 138, 0.3);
	}

	.action-btn-edit:hover {
		background: rgba(56, 89, 138, 0.25);
		color: #a8d0f0;
		border-color: rgba(56, 89, 138, 0.4);
	}

	.action-btn-save {
		background: rgba(90, 138, 56, 0.15);
		color: #9cb88b;
		border-color: rgba(90, 138, 56, 0.3);
	}

	.action-btn-save:hover {
		background: rgba(90, 138, 56, 0.25);
		color: #b8d4a8;
		border-color: rgba(90, 138, 56, 0.4);
	}

	.action-btn-presets {
		background: rgba(139, 92, 246, 0.15);
		color: #b8a8d8;
		border-color: rgba(139, 92, 246, 0.3);
	}

	.action-btn-presets:hover {
		background: rgba(139, 92, 246, 0.25);
		color: #d0c0f0;
		border-color: rgba(139, 92, 246, 0.4);
	}

	.action-btn-delete {
		background: rgba(180, 60, 60, 0.15);
		color: #e08888;
		border-color: rgba(180, 60, 60, 0.3);
	}

	.action-btn-delete:hover {
		background: rgba(180, 60, 60, 0.25);
		color: #f0a0a0;
		border-color: rgba(180, 60, 60, 0.4);
	}

	.boards {
		column-count: 3;
		column-gap: 1rem;
	}

	@media (max-width: 900px) {
		.boards {
			column-count: 2;
		}
	}

	@media (max-width: 600px) {
		.boards {
			column-count: 1;
		}
	}

	.board-card {
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		background: #0c0c0c;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		overflow: hidden;
		break-inside: avoid;
		margin-bottom: 1rem;
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
		color: #e5e5e5;
	}

	.ip-text {
		margin: 0;
		font-size: 0.75rem;
		color: #555;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.connection-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		transition: background-color 0.3s;
	}

	.connection-dot.connected {
		background-color: #22c55e;
		box-shadow: 0 0 4px rgba(34, 197, 94, 0.6);
	}

	.connection-dot.disconnected {
		background-color: #ef4444;
		box-shadow: 0 0 4px rgba(239, 68, 68, 0.6);
	}

	.expand-icon {
		font-size: 1.2rem;
		color: #444;
		transition: transform 0.2s;
	}

	.board-controls {
		padding: 0.75rem;
		border-top: 1px solid #1a1a1a;
	}

	.color-section {
		position: relative;
		display: flex;
		justify-content: center;
		margin-bottom: 1rem;
	}

	.color-wheel-container {
		position: relative;
		display: inline-block;
	}

	.wled-gui-btn {
		position: absolute;
		top: 0;
		right: 0.75rem;
		z-index: 10;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: #0b0d14;
		border: 1px solid rgba(56, 89, 138, 0.2);
		border-radius: 50%;
		color: #6b9fc8;
		text-decoration: none;
		transition: all 0.2s;
	}

	.wled-gui-btn:hover {
		background: rgba(56, 89, 138, 0.15);
		border-color: rgba(56, 89, 138, 0.4);
		color: #8bb8de;
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
		background-color: #222;
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
		background-color: #666;
		transition: 0.3s;
		border-radius: 50%;
	}

	.toggle-switch input:checked + .toggle-slider.color-toggle {
		background-color: var(--board-color, #22c55e);
	}

	.toggle-switch input:checked + .toggle-slider:before {
		transform: translateX(24px);
		background-color: white;
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
		background: #0b0d14;
		color: #888;
		border: 1px solid rgba(56, 89, 138, 0.2);
		border-radius: 8px;
		font-size: 1rem;
		cursor: pointer;
		outline: none;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23555' d='M6 9L1 4h10z'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
	}

	.preset-dropdown:hover {
		background: #0d1117;
		border-color: rgba(56, 89, 138, 0.3);
	}

	.preset-dropdown:focus {
		border-color: rgba(56, 89, 138, 0.5);
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
		background: #080a10;
		color: #444;
		border-color: rgba(56, 89, 138, 0.15);
	}

	.sync-presets-btn.prominent {
		background: #0b0d14;
		color: #555;
		border: 1px solid rgba(56, 89, 138, 0.2);
		padding: 0.75rem 1rem;
		border-radius: 8px;
		font-size: 0.9rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s ease;
		text-align: center;
	}

	.sync-presets-btn.prominent:hover:not(:disabled) {
		background: #111;
		color: #888;
		border-color: #222;
	}

	.sync-presets-btn.prominent:active:not(:disabled) {
		background: #0f0f0f;
	}

	.sync-presets-btn.prominent:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.update-preset-btn {
		width: 100%;
		padding: 0.6rem 0.75rem;
		background: transparent;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
		color: #555;
		margin-top: 0.5rem;
	}

	.update-preset-btn:hover {
		background: #111;
		color: #888;
		border-color: #222;
	}

	.effects-section {
		margin-bottom: 1.5rem;
	}

	.effects-dropdown {
		width: 100%;
		padding: 0.75rem;
		padding-right: 0.75rem;
		background: #0b0d14;
		color: #888;
		border: 1px solid rgba(56, 89, 138, 0.2);
		border-radius: 8px;
		font-size: 1rem;
		cursor: pointer;
		outline: none;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23555' d='M6 9L1 4h10z'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
	}

	.effects-dropdown:hover {
		background: #0d1117;
		border-color: rgba(56, 89, 138, 0.3);
	}

	.effects-dropdown:focus {
		border-color: rgba(56, 89, 138, 0.5);
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
		color: #8bb8de;
	}

	.led-count-value {
		color: #a8d0f0;
		font-weight: 500;
	}

	.led-count-slider {
		width: 100%;
		height: 4px;
		border-radius: 2px;
		background: rgba(56, 89, 138, 0.3);
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
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #fff;
		cursor: pointer;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
	}

	.led-count-slider::-moz-range-thumb {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #fff;
		cursor: pointer;
		border: none;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
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
		color: #fff;
	}

	.transition-wrapper:has(input:disabled) .transition-label,
	.transition-wrapper:has(input:disabled) .transition-value {
		color: #555;
	}

	.transition-value {
		color: #a8d0f0;
		font-weight: 500;
	}

	.transition-slider {
		width: 100%;
		height: 4px;
		border-radius: 2px;
		background: rgba(56, 89, 138, 0.3);
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
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #e0e0e0;
		cursor: pointer;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
	}

	.transition-slider::-moz-range-thumb {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #e0e0e0;
		cursor: pointer;
		border: none;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
	}

	.brightness-slider {
		width: 100%;
		height: 4px;
		border-radius: 2px;
		background: linear-gradient(to right, #111, #666);
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
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #fff;
		cursor: pointer;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
	}

	.brightness-slider::-moz-range-thumb {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #fff;
		cursor: pointer;
		border: none;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
	}

	.effect-params-section {
		display: flex;
		align-items: center;
		gap: 12px;
		margin: 12px 0;
	}

	.effect-params-section label {
		min-width: 100px;
		font-size: 0.9rem;
		color: #fff;
	}

	.effect-params-section:has(input:disabled) label,
	.effect-params-section:has(input:disabled) .param-value {
		color: #555;
	}

	.param-value {
		color: #a8d0f0;
		font-weight: 500;
		margin-left: 4px;
	}

	.effect-param-slider {
		flex: 1;
		-webkit-appearance: none;
		appearance: none;
		height: 4px;
		border-radius: 2px;
		background: rgba(56, 89, 138, 0.3);
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
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #e0e0e0;
		cursor: pointer;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
	}

	.effect-param-slider::-moz-range-thumb {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #e0e0e0;
		cursor: pointer;
		border: none;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
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
		color: #888;
		font-weight: 500;
	}

	.edit-form .form-row {
		display: flex;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.edit-form .form-group.half {
		flex: 1;
		margin-bottom: 0;
	}

	.edit-form .form-input {
		width: 100%;
		padding: 0.75rem;
		background: #0f0f0f;
		color: #e5e5e5;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		font-size: 1rem;
		box-sizing: border-box;
	}

	.edit-form .form-input:focus {
		outline: none;
		border-color: #333;
	}

	.edit-form .form-input::placeholder {
		color: #444;
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
		background: transparent;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
		color: #555;
	}

	.edit-form .submit-btn {
		color: #555;
	}

	.edit-form .submit-btn:hover {
		background: #111;
		color: #fff;
		border-color: #222;
	}

	.edit-form .cancel-btn {
		color: #444;
	}

	.edit-form .cancel-btn:hover {
		background: #1a1212;
		color: #c44;
		border-color: #331a1a;
	}

	.member-grid {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
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
		border: 1px solid #333;
		border-radius: 3px;
		background: #111;
		cursor: pointer;
		position: relative;
		transition: all 0.2s ease;
		vertical-align: middle;
		margin-top: 0;
		margin-bottom: 0;
	}

	.member-checkbox-input:checked {
		background: #1a1a1a;
		border-color: #444;
	}

	.member-checkbox-input:checked::after {
		content: '✓';
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		color: #fff;
		font-size: 10px;
		font-weight: bold;
	}

	.member-checkbox-input:hover {
		border-color: #444;
	}

	.member-name {
		font-weight: 500;
		color: #888;
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
		background-color: rgba(0, 0, 0, 0.8);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.confirm-dialog {
		background-color: #0f0f0f;
		border: 1px solid #1a1a1a;
		border-radius: 12px;
		padding: 2rem;
		max-width: 400px;
		width: 90%;
	}

	.confirm-dialog h3 {
		margin: 0 0 1rem 0;
		color: #fff;
		font-size: 1.2rem;
	}

	.confirm-dialog p {
		margin: 0 0 1.5rem 0;
		color: #888;
		line-height: 1.4;
	}

	.confirm-dialog-buttons {
		display: flex;
		gap: 1rem;
		justify-content: flex-end;
	}

	.btn {
		padding: 0.5rem 1rem;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		cursor: pointer;
		font-size: 0.9rem;
		transition: all 0.2s;
		background: transparent;
	}

	.btn-cancel {
		color: #555;
	}

	.btn-cancel:hover {
		background: #111;
		color: #888;
		border-color: #222;
	}

	.btn-delete {
		color: #555;
	}

	.btn-delete:hover {
		background: #1a1212;
		color: #c44;
		border-color: #331a1a;
	}
</style>
