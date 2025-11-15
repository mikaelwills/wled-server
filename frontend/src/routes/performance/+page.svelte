<script>
	import { onMount } from 'svelte';
	import { programs, programsLoading, programsError, currentlyPlayingProgram } from '$lib/store';
	import { playProgram as playProgramService, stopPlayback as stopPlaybackService, dimProgramBoards } from '$lib/playback-db';
	import { updateProgram } from '$lib/programs-db';
	import { setBoardBrightness } from '$lib/boards-db';
	import { API_URL } from '$lib/api';

	// Track which program is playing locally (for audio sync)
	let audioElements = {};

	// Track playback progress for each program (0-100)
	let playbackProgress = $state({});

	// Track animation frame ID for smooth progress updates
	let animationFrameId = null;

	// Flag to indicate manual stop (breaks chain)
	let manualStop = false;

	// Subscribe to currently playing program to sync state
	let currentPlayingId = $derived($currentlyPlayingProgram?.id || null);

	// Context menu state
	let contextMenu = $state({
		visible: false,
		x: 0,
		y: 0,
		programId: null,
		showChainSubmenu: false,
		showTransitionSubmenu: false,
		submenuX: 0,
		submenuY: 0
	});

	onMount(() => {
		// Cleanup audio elements on unmount
		return () => {
			Object.values(audioElements).forEach(audio => {
				if (audio) {
					audio.pause();
					audio.src = '';
				}
			});
			if (animationFrameId) {
				cancelAnimationFrame(animationFrameId);
			}
		};
	});

	async function toggleProgram(program) {
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

	async function playProgram(program) {
		console.log('▶️ Playing program:', program.songName);

		// Create or get audio element for this program
		let audio = audioElements[program.id];

		if (!audio) {
			audio = new Audio();
			audioElements[program.id] = audio;

			// Load audio file
			if (program.audioId) {
				try {
					const response = await fetch(`${API_URL}/audio/${program.audioId}`);
					if (response.ok) {
						const blob = await response.blob();
						audio.src = URL.createObjectURL(blob);
					} else {
						console.error(`Failed to fetch audio: ${response.statusText}`);
						return;
					}
				} catch (err) {
					console.error('Error loading audio from API:', err);
					return;
				}
			} else {
				console.error('No audio available for this program');
				return;
			}
		}

		// Set up audio ended event
		audio.onended = async () => {
			console.log('🏁 Program ended:', program.songName);

			// Cleanup current program
			currentPlayingId = null;
			playbackProgress[program.id] = 0;
			if (animationFrameId) {
				cancelAnimationFrame(animationFrameId);
				animationFrameId = null;
			}
			stopPlaybackService();

			// Check for manual stop (breaks chain)
			if (manualStop) {
				console.log('⛔ Manual stop - chain broken');
				manualStop = false;
				return;
			}

			// Auto-play chain logic
			if (program.nextProgramId) {
				console.log(`⛓️  Chain detected - next program: ${program.nextProgramId}`);
				const nextProgram = $programs.find(p => p.id === program.nextProgramId);

				if (nextProgram) {
					// Apply transition before playing next program
					await applyTransition(program);

					// Play next program in chain
					await playProgram(nextProgram);
				} else {
					console.warn(`⚠️  Next program "${program.nextProgramId}" not found`);
				}
			}
		};

		// Capture the EXACT moment audio starts
		const audioStartTime = performance.now();

		// Play audio from the beginning
		audio.currentTime = 0;
		await audio.play();

		// Update state
		currentPlayingId = program.id;
		playbackProgress[program.id] = 0;

		// Start smooth progress animation using requestAnimationFrame
		const updateProgress = () => {
			if (audio && audio.duration && currentPlayingId === program.id) {
				playbackProgress[program.id] = (audio.currentTime / audio.duration) * 100;
				animationFrameId = requestAnimationFrame(updateProgress);
			}
		};
		animationFrameId = requestAnimationFrame(updateProgress);

		// Schedule LED cues with audio start timestamp
		playProgramService(program, 0, audioStartTime);
	}

	async function stopProgram(program) {
		console.log('⏹ Stopping program:', program.songName);

		// Set manual stop flag to break chain
		manualStop = true;

		// Stop audio
		const audio = audioElements[program.id];
		if (audio) {
			audio.pause();
			audio.currentTime = 0;
		}

		// Stop animation frame
		if (animationFrameId) {
			cancelAnimationFrame(animationFrameId);
			animationFrameId = null;
		}

		// Reset progress
		playbackProgress[program.id] = 0;

		// Stop global playback
		stopPlaybackService();

		// Dim the program's boards
		try {
			await dimProgramBoards(program);
		} catch (err) {
			console.error('Failed to dim boards:', err);
		}

		// Clear playing state
		currentPlayingId = null;
	}

	// Transition implementations
	async function applyTransition(program) {
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
	function showContextMenu(event, programId) {
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
	function getSubmenuPosition(parentElement) {
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

	async function setNextProgram(targetProgramId) {
		const program = $programs.find(p => p.id === contextMenu.programId);
		if (!program) return;

		program.nextProgramId = targetProgramId;
		await updateProgram(program);
		hideContextMenu();
	}

	async function setTransitionType(type) {
		const program = $programs.find(p => p.id === contextMenu.programId);
		if (!program) return;

		program.transitionType = type;
		await updateProgram(program);
	}

	async function setTransitionDuration(duration) {
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
		hideContextMenu();
	}

	// Close context menu on click outside
	function handleClickOutside(event) {
		if (contextMenu.visible) {
			hideContextMenu();
		}
	}
</script>

<svg style="position: absolute; width: 0; height: 0;">
	<defs>
		<filter id="turbulent-displace" x="-50%" y="-50%" width="200%" height="200%">
			<feTurbulence type="fractalNoise" baseFrequency="0.02 0.02" numOctaves="3" result="turbulence" seed="1">
				<animate attributeName="seed" from="1" to="300" dur="6s" repeatCount="indefinite" />
			</feTurbulence>
			<feDisplacementMap in="SourceGraphic" in2="turbulence" scale="4" xChannelSelector="R" yChannelSelector="G" />
		</filter>
	</defs>
</svg>

<div class="performance-page" onclick={handleClickOutside}>
	{#if $programsLoading}
		<div class="empty-state">
			<p class="empty-text">Loading programs...</p>
		</div>
	{:else if $programsError}
		<div class="empty-state">
			<p class="empty-text error">{$programsError}</p>
		</div>
	{:else if $programs.length === 0}
		<div class="empty-state">
			<p class="empty-text">No programs available</p>
			<p class="empty-hint">Create programs in the Sequencer page first</p>
		</div>
	{:else}
		<div class="programs-grid">
			{#each $programs as program (program.id)}
				<button
					class="program-button"
					class:playing={currentPlayingId === program.id}
					onclick={() => toggleProgram(program)}
					oncontextmenu={(e) => showContextMenu(e, program.id)}
				>

					<!-- Progress bar (background) -->
					{#if currentPlayingId === program.id}
						<div class="progress-bar" style="width: {playbackProgress[program.id] || 0}%"></div>
					{/if}

					<!-- Program info (foreground) -->
					<div class="program-content">
						<div class="song-name">{program.songName || 'Untitled'}</div>
						{#if program.loopyProTrack}
							<div class="track-number">Loopy {program.loopyProTrack}</div>
						{/if}
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
			>
				<div class="menu-item" onclick={() => setNextProgram(undefined)}>
					<span>○ None</span>
				</div>
				{#each $programs.filter(p => p.id !== contextMenu.programId) as prog}
					<div class="menu-item" onclick={() => setNextProgram(prog.id)}>
						<span>{currentProgram?.nextProgramId === prog.id ? '●' : '○'} {prog.songName}</span>
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
							oninput={(e) => setTransitionDuration(parseInt(e.target.value))}
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
		background-color: #0f0f0f;
		color: #e5e5e5;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
	}

	.performance-page {
		width: 100%;
		height: 100vh;
		padding: 1rem;
		box-sizing: border-box;
		overflow: visible;
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
		gap: 1rem;
		width: 100%;
		height: 100%;
		padding: 10px;
		box-sizing: border-box;
		overflow: visible;
	}

	/* Smart grid layout based on number of programs */
	.programs-grid:has(:nth-child(1):last-child) {
		grid-template-columns: 1fr;
		grid-template-rows: 1fr;
	}

	.programs-grid:has(:nth-child(2):last-child) {
		grid-template-columns: 1fr 1fr;
		grid-template-rows: 1fr;
	}

	.programs-grid:has(:nth-child(3):last-child),
	.programs-grid:has(:nth-child(4):last-child) {
		grid-template-columns: 1fr 1fr;
		grid-template-rows: 1fr 1fr;
	}

	.programs-grid:has(:nth-child(5):last-child),
	.programs-grid:has(:nth-child(6):last-child) {
		grid-template-columns: 1fr 1fr 1fr;
		grid-template-rows: 1fr 1fr;
	}

	.programs-grid:has(:nth-child(7):last-child),
	.programs-grid:has(:nth-child(8):last-child),
	.programs-grid:has(:nth-child(9):last-child) {
		grid-template-columns: 1fr 1fr 1fr;
		grid-template-rows: 1fr 1fr 1fr;
	}

	/* For 10+ programs, use 4 columns and auto rows */
	.programs-grid:has(:nth-child(10)) {
		grid-template-columns: 1fr 1fr 1fr 1fr;
		grid-auto-rows: 1fr;
		overflow-y: auto;
	}

	@media (max-width: 768px) {
		.programs-grid {
			gap: 0.75rem;
		}

		/* Mobile: max 2 columns */
		.programs-grid:has(:nth-child(3):last-child),
		.programs-grid:has(:nth-child(4):last-child),
		.programs-grid:has(:nth-child(5):last-child),
		.programs-grid:has(:nth-child(6):last-child) {
			grid-template-columns: 1fr 1fr;
			grid-template-rows: auto;
			grid-auto-rows: 1fr;
		}

		.programs-grid:has(:nth-child(7):last-child),
		.programs-grid:has(:nth-child(8):last-child),
		.programs-grid:has(:nth-child(9):last-child),
		.programs-grid:has(:nth-child(10)) {
			grid-template-columns: 1fr 1fr;
			grid-auto-rows: 1fr;
			overflow-y: auto;
		}
	}


	.program-button {
		background: linear-gradient(135deg, #1a1a1a 0%, #0f0f0f 100%);
		border: 2px solid #2a2a2a;
		border-radius: 12px;
		cursor: pointer;
		transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
		padding: 0;
		position: relative;
		overflow: visible;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 100%;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
	}

	.program-button::before {
		content: '';
		position: absolute;
		inset: 0;
		background: radial-gradient(circle at center, rgba(168, 85, 247, 0.1) 0%, transparent 70%);
		opacity: 0;
		transition: opacity 0.3s;
	}

	.program-button:hover {
		border-color: #3a3a3a;
		transform: scale(1.02);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
	}

	.program-button:hover::before {
		opacity: 1;
	}

	.program-button:active {
		transform: scale(1.01);
	}

	/* Progress bar (fills from left to right) */
	.progress-bar {
		position: absolute;
		top: 0;
		left: 0;
		bottom: 0;
		background: linear-gradient(90deg,
			rgba(168, 85, 247, 0.15) 0%,
			rgba(168, 85, 247, 0.25) 100%
		);
		border-radius: 12px 0 0 12px;
		z-index: 1;
		pointer-events: none;
	}

	.program-button.playing {
		background: rgba(0, 0, 0, 0.6);
		border: none;
		position: relative;
		animation: none;
		box-shadow: none;
	}

	/* Purple inner border */
	.program-button.playing::before {
		content: '';
		position: absolute;
		inset: -2px;
		border-radius: 14px;
		border: 1px solid rgba(168, 85, 247, 0.5);
		filter: url(#turbulent-displace) drop-shadow(0 0 3px rgba(168, 85, 247, 0.3));
		pointer-events: none;
		z-index: 3;
		opacity: 1 !important;
		background: none !important;
	}

	/* Purple outer glow */
	.program-button.playing::after {
		content: '';
		position: absolute;
		inset: -2px;
		border-radius: 14px;
		border: 1px solid #a855f7;
		filter: url(#turbulent-displace) drop-shadow(0 0 6px rgba(168, 85, 247, 0.3));
		opacity: 0.7;
		pointer-events: none;
		z-index: 2;
	}

	.program-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: 2rem;
		width: 100%;
		height: 100%;
		text-align: center;
		position: relative;
		z-index: 10;
	}

	.song-name {
		font-size: 1.5rem;
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

	.track-number {
		font-size: 0.875rem;
		font-weight: 600;
		color: #9ca3af;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 0.25rem 0.75rem;
		background-color: rgba(0, 0, 0, 0.2);
		border-radius: 12px;
	}

	.program-button.playing .track-number {
		color: #e9d5ff;
		background-color: rgba(255, 255, 255, 0.2);
	}

	.playing-indicator {
		position: absolute;
		bottom: 1.5rem;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 4px;
		align-items: flex-end;
		height: 24px;
	}

	.wave-bar {
		width: 4px;
		background-color: white;
		border-radius: 2px;
		animation: wave 1s ease-in-out infinite;
	}

	.wave-bar:nth-child(1) {
		animation-delay: 0s;
	}

	.wave-bar:nth-child(2) {
		animation-delay: 0.2s;
	}

	.wave-bar:nth-child(3) {
		animation-delay: 0.4s;
	}

	@keyframes wave {
		0%, 100% {
			height: 8px;
		}
		50% {
			height: 24px;
		}
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
