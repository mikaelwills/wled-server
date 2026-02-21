<script lang="ts">
	import { flip } from 'svelte/animate';
	import { programs, programsLoading, programsError, currentlyPlayingProgram, playbackPosition, activeSetlistId } from '$lib/stores/store';
	import { playProgram as playProgramService, stopPlayback as stopPlaybackService } from '$lib/db/playback-db';
	import { updateProgram, reorderPrograms } from '$lib/db/programs-db';
	import { setBoardBrightness } from '$lib/db/boards-db';
	import { Program, type TransitionType } from '$lib/models/Program';

	let filteredPrograms = $derived(
		$programs
			.filter(p => p.setlistId === $activeSetlistId)
			.sort((a, b) => a.displayOrder - b.displayOrder)
	);

	let playbackProgress = $state<Record<string, number>>({});

	// Flag to indicate manual stop (breaks chain)
	let manualStop = false;

	// Track the program that was last playing (for chain detection on stop)
	let lastPlayedProgramId: string | null = null;

	// Subscribe to currently playing program to sync state
	let currentPlayingId = $derived($currentlyPlayingProgram?.id || null);

	// Context menu state
	let contextMenu = $state<{
		visible: boolean;
		x: number;
		y: number;
		programId: string | null;
		showChainSubmenu: boolean;
		showTransitionSubmenu: boolean;
		submenuX: number;
		submenuY: number;
	}>({
		visible: false,
		x: 0,
		y: 0,
		programId: null,
		showChainSubmenu: false,
		showTransitionSubmenu: false,
		submenuX: 0,
		submenuY: 0
	});

	// Drag-and-drop state
	let dragState = $state<{
		isDragging: boolean;
		draggedIndex: number | null;
		dragOverIndex: number | null;
		longPressTimer: ReturnType<typeof setTimeout> | null;
		longPressActive: boolean;
		initialX: number;
		initialY: number;
		currentX: number;
		currentY: number;
		programIdForLongPress: string | null;
		draggedElement: HTMLElement | null;
	}>({
		isDragging: false,
		draggedIndex: null,
		dragOverIndex: null,
		longPressTimer: null,
		longPressActive: false,
		initialX: 0,
		initialY: 0,
		currentX: 0,
		currentY: 0,
		programIdForLongPress: null,
		draggedElement: null
	});

	$effect(() => {
		const pos = $playbackPosition;
		if (pos) {
			const pct = pos.durationSecs > 0
				? Math.min((pos.positionSecs / pos.durationSecs) * 100, 100)
				: 0;
			playbackProgress[pos.programId] = pct;
		}
	});

	$effect(() => {
		const pos = $playbackPosition;
		if (pos === null && lastPlayedProgramId && !manualStop) {
			const ended = $programs.find(p => p.id === lastPlayedProgramId);
			if (ended) {
				playbackProgress[ended.id] = 0;
				if (ended.nextProgramId) {
					const nextProgram = $programs.find(p => p.id === ended.nextProgramId);
					if (nextProgram) {
						applyTransition(ended).then(() => playProgram(nextProgram));
					}
				}
			}
			lastPlayedProgramId = null;
		} else if (pos === null) {
			manualStop = false;
		}
	});

	async function toggleProgram(program: Program) {
		if (currentPlayingId === program.id) {
			// Stop the current program
			await stopProgram(program);
		} else {
			// Stop any currently playing program first
			if (currentPlayingId) {
				const currentProgram = $programs.find(p => p.id === currentPlayingId);
				if (currentProgram) {
					await stopProgram(currentProgram);
				}
			}
			// Play the new program
			await playProgram(program);
		}
	}

	async function playProgram(program: Program) {
		console.log('▶️ Playing program:', program.songName);

		lastPlayedProgramId = program.id;
		playbackProgress[program.id] = 0;

		playProgramService(program, 0);
	}

	async function stopProgram(program: Program) {
		console.log('⏹ Stopping program:', program.songName);

		manualStop = true;
		playbackProgress[program.id] = 0;

		stopPlaybackService();
	}

	// Drag-and-drop functions (desktop only - using pointer + HTML5 drag API)
	function handlePointerDown(event: PointerEvent, programId: string, index: number) {
		// Don't allow dragging if a program is currently playing
		if (currentPlayingId !== null) return;

		const clientX = event.clientX;
		const clientY = event.clientY;

		// Store initial position
		dragState.initialX = clientX;
		dragState.initialY = clientY;
		dragState.programIdForLongPress = programId;

		// Start long-press timer (500ms) for drag activation
		dragState.longPressTimer = setTimeout(() => {
			dragState.longPressActive = true;
		}, 500);
	}

	function handlePointerMove(event: PointerEvent) {
		// Cancel long-press if pointer moves too much (more than 10px)
		if (dragState.longPressTimer && !dragState.isDragging) {
			const clientX = event.clientX;
			const clientY = event.clientY;
			const deltaX = Math.abs(clientX - dragState.initialX);
			const deltaY = Math.abs(clientY - dragState.initialY);

			if (deltaX > 10 || deltaY > 10) {
				clearTimeout(dragState.longPressTimer);
				dragState.longPressTimer = null;
				dragState.longPressActive = false;
			}
		}
	}

	function handlePointerUp() {
		// Clear long-press timer if still pending
		if (dragState.longPressTimer) {
			clearTimeout(dragState.longPressTimer);
			dragState.longPressTimer = null;
		}
		dragState.longPressActive = false;
	}

	function handleDragStart(event: DragEvent, index: number) {
		// Only allow drag if long-press was activated and no program is playing
		if (!dragState.longPressActive || currentPlayingId !== null) {
			event.preventDefault();
			return;
		}

		dragState.isDragging = true;
		dragState.draggedIndex = index;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', index.toString());
		}
	}

	function handleDragOver(event: DragEvent, index: number) {
		if (!dragState.isDragging) return;
		event.preventDefault();
		dragState.dragOverIndex = index;
	}

	function handleDragEnd() {
		dragState.isDragging = false;
		dragState.draggedIndex = null;
		dragState.dragOverIndex = null;
		dragState.longPressActive = false;
	}

	async function handleDrop(event: DragEvent, dropIndex: number) {
		event.preventDefault();

		if (dragState.draggedIndex === null || dragState.draggedIndex === dropIndex) {
			handleDragEnd();
			return;
		}

		const reordered = [...filteredPrograms];
		const [movedItem] = reordered.splice(dragState.draggedIndex, 1);
		reordered.splice(dropIndex, 0, movedItem);

		// Persist the new order
		try {
			await reorderPrograms(reordered);
		} catch (error) {
			console.error('Failed to reorder programs:', error);
		}

		handleDragEnd();
	}

	// Transition implementations
	async function applyTransition(program: Program) {
		console.log(`🔄 Applying ${program.transitionType} transition (${program.transitionDuration}ms)`);

		if (program.transitionType === 'blackout') {
			// Blackout: set all program boards to 0 brightness
			const boardIds = [...new Set(program.cues.flatMap(c => c.boards))];
			await Promise.all(boardIds.map(boardId => setBoardBrightness(boardId, 0)));

			// Wait for transition duration
			if (program.transitionDuration > 0) {
				await new Promise(resolve => setTimeout(resolve, program.transitionDuration));
			}
		} else if (program.transitionType === 'hold') {
			// Hold: keep current lighting state, just wait
			if (program.transitionDuration > 0) {
				await new Promise(resolve => setTimeout(resolve, program.transitionDuration));
			}
		}
		// Immediate: no delay, do nothing
	}

	// Context menu functions
	function showContextMenu(event: MouseEvent, programId: string) {
		event.preventDefault();
		event.stopPropagation();

		contextMenu.visible = true;
		contextMenu.x = event.clientX;
		contextMenu.y = event.clientY;
		contextMenu.programId = programId;
		contextMenu.showChainSubmenu = false;
		contextMenu.showTransitionSubmenu = false;
	}

	// Get submenu position
	function getSubmenuPosition(parentElement: HTMLElement | null) {
		if (!parentElement) return { left: 0, top: 0 };
		const rect = parentElement.getBoundingClientRect();
		return {
			left: rect.right - 8,
			top: rect.top - 8
		};
	}

	function hideContextMenu() {
		contextMenu.visible = false;
		contextMenu.showChainSubmenu = false;
		contextMenu.showTransitionSubmenu = false;
	}

	async function setNextProgram(targetProgramId: string | undefined) {
		const program = $programs.find(p => p.id === contextMenu.programId);
		if (!program) return;

		// Toggle: if clicking the currently selected program, deselect it
		if (program.nextProgramId === targetProgramId) {
			program.nextProgramId = undefined;
		} else {
			program.nextProgramId = targetProgramId;
		}

		await updateProgram(program);
		// Don't hide menu - let user see the selection and make more changes
	}

	async function setTransitionType(type: TransitionType) {
		const program = $programs.find(p => p.id === contextMenu.programId);
		if (!program) return;

		program.transitionType = type;
		await updateProgram(program);
	}

	async function setTransitionDuration(duration: number) {
		const program = $programs.find(p => p.id === contextMenu.programId);
		if (!program) return;

		program.transitionDuration = duration;
		await updateProgram(program);
	}

	async function clearChain() {
		const program = $programs.find(p => p.id === contextMenu.programId);
		if (!program) return;

		program.nextProgramId = undefined;
		await updateProgram(program);
		// Don't hide menu - let user see the change
	}

	// Close context menu on click outside
	function handleClickOutside(event: MouseEvent) {
		if (contextMenu.visible) {
			hideContextMenu();
		}
	}
</script>

<div class="performance-page" onclick={handleClickOutside}>
	{#if $programsLoading}
		<div class="empty-state">
			<p class="empty-text">Loading programs...</p>
		</div>
	{:else if $programsError}
		<div class="empty-state">
			<p class="empty-text error">{$programsError}</p>
		</div>
	{:else if filteredPrograms.length === 0}
		<div class="empty-state">
			<p class="empty-text">No programs in this set</p>
			<p class="empty-hint">Create programs in the Programming page first</p>
		</div>
	{:else}
		<div class="programs-grid" data-count={filteredPrograms.length}>
			{#each filteredPrograms as program, index (program.id)}
				<button
					class="program-button"
					class:playing={currentPlayingId === program.id}
					class:long-press-active={dragState.longPressActive && dragState.programIdForLongPress === program.id}
					class:dragging={dragState.isDragging && dragState.draggedIndex === index}
					class:drag-over={dragState.dragOverIndex === index && dragState.draggedIndex !== index}
					class:drag-disabled={currentPlayingId !== null}
					draggable={currentPlayingId === null}
					onclick={() => !dragState.isDragging && toggleProgram(program)}
					oncontextmenu={(e) => showContextMenu(e, program.id)}
					onpointerdown={(e) => handlePointerDown(e, program.id, index)}
					onpointermove={handlePointerMove}
					onpointerup={handlePointerUp}
					ondragstart={(e) => handleDragStart(e, index)}
					ondragover={(e) => handleDragOver(e, index)}
					ondragend={handleDragEnd}
					ondrop={(e) => handleDrop(e, index)}
					animate:flip={{ duration: 300 }}
				>

					<!-- Progress bar (background) -->
					{#if currentPlayingId === program.id}
						<div class="progress-bar" style="width: {playbackProgress[program.id] || 0}%"></div>
					{/if}

					<!-- Program info (foreground) -->
					<div class="program-content">
						<div class="song-name">{program.displayName || program.songName || 'Untitled'}</div>
					</div>
				</button>
			{/each}
		</div>
	{/if}

	<!-- Context Menu -->
	{#if contextMenu.visible}
		{@const currentProgram = $programs.find(p => p.id === contextMenu.programId)}
		<div
			class="context-menu"
			style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
			onclick={(e) => e.stopPropagation()}
		>
			<!-- Chain to Program -->
			<div
				class="menu-item menu-item-parent"
				onmouseenter={(e) => {
					contextMenu.showChainSubmenu = true;
					const rect = e.currentTarget.getBoundingClientRect();
					contextMenu.submenuX = rect.right - 8;
					contextMenu.submenuY = rect.top - 8;
				}}
				onmouseleave={() => contextMenu.showChainSubmenu = false}
			>
				<span>Chain to Program</span>
			</div>

			<!-- Transition Settings -->
			<div
				class="menu-item menu-item-parent"
				onmouseenter={(e) => {
					contextMenu.showTransitionSubmenu = true;
					const rect = e.currentTarget.getBoundingClientRect();
					contextMenu.submenuX = rect.right - 8;
					contextMenu.submenuY = rect.top - 8;
				}}
				onmouseleave={() => contextMenu.showTransitionSubmenu = false}
			>
				<span>Transition</span>
			</div>

			<!-- Clear Chain -->
			{#if currentProgram?.nextProgramId}
				<div class="menu-divider"></div>
				<div class="menu-item" onclick={clearChain}>
					<span>Clear Chain</span>
				</div>
			{/if}

			<div class="menu-divider"></div>

			<!-- Cancel -->
			<div class="menu-item menu-item-cancel" onclick={hideContextMenu}>
				<span>Cancel</span>
			</div>
		</div>

		<!-- Chain Submenu (fixed position) -->
		{#if contextMenu.showChainSubmenu}
			<div
				class="submenu"
				style="left: {contextMenu.submenuX}px; top: {contextMenu.submenuY}px;"
				onmouseenter={() => contextMenu.showChainSubmenu = true}
				onmouseleave={() => contextMenu.showChainSubmenu = false}
				onclick={(e) => e.stopPropagation()}
			>
				<div class="menu-item" onclick={() => setNextProgram(undefined)}>
					<span>○ None</span>
				</div>
				{#each $programs.filter(p => p.id !== contextMenu.programId) as prog}
					<div class="menu-item" onclick={() => setNextProgram(prog.id)}>
						<span>{currentProgram?.nextProgramId === prog.id ? '●' : '○'} {prog.displayName || prog.songName}</span>
					</div>
				{/each}
			</div>
		{/if}

		<!-- Transition Submenu (fixed position) -->
		{#if contextMenu.showTransitionSubmenu}
			<div
				class="submenu"
				style="left: {contextMenu.submenuX}px; top: {contextMenu.submenuY}px;"
				onmouseenter={() => contextMenu.showTransitionSubmenu = true}
				onmouseleave={() => contextMenu.showTransitionSubmenu = false}
				onclick={(e) => e.stopPropagation()}
			>
				<div class="menu-item" onclick={() => setTransitionType('immediate')}>
					<span>{currentProgram?.transitionType === 'immediate' ? '●' : '○'} Immediate</span>
				</div>
				<div class="menu-item" onclick={() => setTransitionType('blackout')}>
					<span>{currentProgram?.transitionType === 'blackout' ? '●' : '○'} Blackout</span>
				</div>
				<div class="menu-item" onclick={() => setTransitionType('hold')}>
					<span>{currentProgram?.transitionType === 'hold' ? '●' : '○'} Hold</span>
				</div>
				<div class="menu-item">
					<label style="display: flex; align-items: center; gap: 0.5rem; width: 100%;">
						<span style="flex-shrink: 0;">Duration:</span>
						<input
							type="range"
							min="0"
							max="5000"
							step="100"
							value={currentProgram?.transitionDuration || 0}
							oninput={(e) => setTransitionDuration(parseInt((e.target as HTMLInputElement).value))}
							style="flex: 1;"
						/>
						<span style="flex-shrink: 0; min-width: 3rem;">{currentProgram?.transitionDuration || 0}ms</span>
					</label>
				</div>
			</div>
		{/if}
	{/if}
</div>

<style>
	:global(body) {
		background-color: #0a0a0a;
		color: #e5e5e5;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
	}

	.performance-page {
		width: 100%;
		min-height: 100vh;
		padding: 0;
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		outline: none;
	}

	.empty-state {
		text-align: center;
		padding: 4rem 2rem;
	}

	.empty-text {
		font-size: 1.5rem;
		color: #6b7280;
		margin: 0 0 0.5rem 0;
	}

	.empty-text.error {
		color: #ef4444;
	}

	.empty-hint {
		font-size: 1rem;
		color: #4b5563;
		margin: 0;
	}

	.programs-grid {
		display: grid;
		gap: 0.5rem;
		width: 100%;
		flex: 1;
		padding: 0.5rem;
		box-sizing: border-box;
		/* Default: 3 columns x 3 rows for 8 programs */
		grid-template-columns: 1fr 1fr 1fr;
		grid-template-rows: 1fr 1fr 1fr;
	}

	/* Fallback grid layout using data attribute (more compatible) */
	.programs-grid[data-count="1"] {
		grid-template-columns: 1fr;
		grid-template-rows: 1fr;
	}

	.programs-grid[data-count="2"] {
		grid-template-columns: 1fr 1fr;
		grid-template-rows: 1fr;
	}

	.programs-grid[data-count="3"],
	.programs-grid[data-count="4"] {
		grid-template-columns: 1fr 1fr;
		grid-template-rows: 1fr 1fr;
	}

	.programs-grid[data-count="5"],
	.programs-grid[data-count="6"] {
		grid-template-columns: 1fr 1fr 1fr;
		grid-template-rows: 1fr 1fr;
	}

	.programs-grid[data-count="7"],
	.programs-grid[data-count="8"],
	.programs-grid[data-count="9"] {
		grid-template-columns: 1fr 1fr 1fr;
		grid-template-rows: 1fr 1fr 1fr;
	}

	/* For 10+ programs, use 4 columns and auto rows */
	.programs-grid[data-count="10"],
	.programs-grid[data-count="11"],
	.programs-grid[data-count="12"],
	.programs-grid[data-count="13"],
	.programs-grid[data-count="14"],
	.programs-grid[data-count="15"],
	.programs-grid[data-count="16"],
	.programs-grid[data-count="17"],
	.programs-grid[data-count="18"],
	.programs-grid[data-count="19"],
	.programs-grid[data-count="20"] {
		grid-template-columns: 1fr 1fr 1fr 1fr;
		grid-auto-rows: 1fr;
		overflow-y: auto;
	}

	@media (max-width: 768px) {
		.programs-grid {
			gap: 0.75rem;
		}

		/* Mobile: max 2 columns */
		.programs-grid[data-count="3"],
		.programs-grid[data-count="4"],
		.programs-grid[data-count="5"],
		.programs-grid[data-count="6"] {
			grid-template-columns: 1fr 1fr;
			grid-template-rows: auto;
			grid-auto-rows: 1fr;
		}

		.programs-grid[data-count="7"],
		.programs-grid[data-count="8"],
		.programs-grid[data-count="9"] {
			grid-template-columns: 1fr 1fr;
			grid-auto-rows: 1fr;
			overflow-y: auto;
		}
	}


	.program-button {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		cursor: grab;
		transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
		padding: 0;
		position: relative;
		overflow: hidden;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 100%;
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
	}

	.program-button:hover {
		border-color: rgba(255, 255, 255, 0.05);
		background: #0e0e0e;
	}

	.program-button:active {
		background: #111111;
	}

	/* Drag-and-drop states */
	.program-button.drag-disabled {
		cursor: not-allowed;
		opacity: 0.7;
	}

	.program-button.long-press-active {
		animation: long-press-pulse 0.3s ease-out;
		cursor: grabbing;
	}

	@keyframes long-press-pulse {
		0% {
			transform: scale(1);
		}
		50% {
			transform: scale(1.05);
		}
		100% {
			transform: scale(1);
		}
	}

	.program-button.dragging {
		opacity: 0.5;
		cursor: grabbing;
		transform: scale(1.05);
		z-index: 1000;
	}

	.program-button.drag-over {
		border: 2px dashed rgba(255, 255, 255, 0.1);
		background: rgba(255, 255, 255, 0.02);
	}

	/* Progress bar (fills from left to right) */
	.progress-bar {
		position: absolute;
		top: 0;
		left: 0;
		bottom: 0;
		background: linear-gradient(90deg,
			rgba(139, 92, 246, 0.1) 0%,
			rgba(139, 92, 246, 0.2) 100%
		);
		border-radius: 12px 0 0 12px;
		z-index: 1;
		pointer-events: none;
	}

	.program-button.playing {
		background: #0c0c0c;
		border: 1px solid rgba(139, 92, 246, 0.5);
		position: relative;
		animation: none;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02), 0 0 20px rgba(139, 92, 246, 0.2);
	}

	.program-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: clamp(0.75rem, 3vw, 2rem);
		width: 100%;
		height: 100%;
		text-align: center;
		position: relative;
		z-index: 10;
	}

	.song-name {
		font-size: clamp(1rem, 4vw, 1.75rem);
		font-weight: 700;
		color: #e5e5e5;
		word-break: break-word;
		line-height: 1.3;
		max-width: 100%;
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 3;
		-webkit-box-orient: vertical;
	}

	.program-button.playing .song-name {
		color: #ffffff;
		text-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
	}

	/* Context Menu */
	.context-menu {
		position: fixed;
		background: #1a1a1a;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		min-width: 200px;
		z-index: 1000;
		padding: 0.25rem 0;
		overflow: hidden;
	}

	.menu-item {
		padding: 0.5rem 1rem;
		cursor: pointer;
		transition: background 0.2s;
		color: #e5e5e5;
		position: relative;
		user-select: none;
	}

	.menu-item:hover {
		background: #2a2a2a;
	}

	.menu-divider {
		height: 1px;
		background: #2a2a2a;
		margin: 0.25rem 0;
	}

	/* Cancel button - no extra padding */
	.menu-item-cancel {
		margin-bottom: 0;
	}

	/* Submenu */
	.submenu {
		position: fixed;
		background: #1a1a1a;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		min-width: 200px;
		max-height: 80vh;
		overflow-y: auto;
		padding: 0.5rem 0;
		z-index: 1001;
	}

	/* Parent menu item with submenu - extend clickable area */
	.menu-item-parent {
		position: relative;
	}

	.menu-item-parent::after {
		content: '';
		position: absolute;
		right: -0.5rem;
		top: 0;
		bottom: 0;
		width: 0.5rem;
	}

	.submenu .menu-item {
		padding: 0.5rem 1rem;
		font-size: 0.9rem;
	}

	/* Slider in menu */
	.menu-item input[type="range"] {
		height: 4px;
		border-radius: 2px;
		background: #2a2a2a;
		outline: none;
		-webkit-appearance: none;
		cursor: pointer;
	}

	.menu-item input[type="range"]::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #a855f7;
		cursor: pointer;
	}

	.menu-item input[type="range"]::-moz-range-thumb {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #a855f7;
		cursor: pointer;
		border: none;
	}
</style>
