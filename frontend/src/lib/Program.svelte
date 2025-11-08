<script>
	import { onMount } from 'svelte';
	import WaveSurfer from 'wavesurfer.js';
	import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js';
	import { API_URL } from '$lib/api';
	import { saveProgram as saveProgramToStore, deleteProgram as deleteProgramFromStore } from '$lib/programs-db';
	import { playProgram as playProgramService, stopPlayback as stopPlaybackService, dimProgramBoards } from '$lib/playback-db';
	import { Program as ProgramModel } from '$lib/models/Program';
	import { programs as programsStore, boards, presets, currentlyPlayingProgram } from '$lib/store';
	import { WLED_EFFECTS } from '$lib/wled-effects';

	// Props
	let {
		program = null
	} = $props();

	// Internal state derived from prop
	let programId = $state(program?.id || null);

	let wavesurfer = $state(null);
	let regions = $state(null);
	let markers = $state([]);
	let fileName = $state('');
	let isLoaded = $state(false);
	let isPlaying = $state(false);
	let cuesExpanded = $state(true); // Start expanded

	// Program metadata
	let songName = $state('');
	let loopyProTrack = $state('');

	// Dropdown state
	let openDropdownId = $state(null);

	// Default target board for new cues
	let defaultTargetBoard = $state(null);
	let defaultBoardDropdownOpen = $state(false);

	onMount(async () => {
		// Boards, groups, and presets are now loaded via stores in parent component
		// No need to fetch here

		// Initialize programId from program prop
		if (program?.id) {
			programId = program.id;
		}

		// Load initial data if provided
		if (program) {
			console.log(`[Program.svelte] onMount - program for ${program.id}:`, {
				hasAudioData: !!program.audioData,
				audioDataLength: program.audioData?.length || 0,
				audioDataPrefix: program.audioData?.substring(0, 50)
			});

			loadProgramData(program);

			// Auto-load audio if compressed audio data is present
			if (program.audioData) {
				setTimeout(() => {
					loadCompressedAudio(program.audioData);
				}, 100);
			}
		}

		// Close dropdown when clicking outside
		const handleClickOutside = (e) => {
			if (!e.target.closest('.boards-dropdown-wrapper')) {
				openDropdownId = null;
			}
		};
		document.addEventListener('click', handleClickOutside);

		return () => {
			if (wavesurfer) {
				wavesurfer.destroy();
			}
			document.removeEventListener('click', handleClickOutside);
		};
	});

	function loadProgramData(data) {
		songName = data.songName || '';
		loopyProTrack = data.loopyProTrack || '';
		fileName = data.fileName || '';
		// Note: cues will need to be restored after audio file is loaded
		// Store them temporarily
		window._pendingCues = data.cues || [];
	}

	export function loadAudioFile(file) {
		console.log('Loading file:', file.name, file.type);

		// Check if it's an audio file
		if (file.type.startsWith('audio/') || file.name.endsWith('.wav') || file.name.endsWith('.mp3')) {
			fileName = file.name;

			// Wait for DOM to update, then initialize WaveSurfer
			setTimeout(() => {
				// Initialize Regions plugin
				regions = RegionsPlugin.create();

				// Create WaveSurfer instance
				wavesurfer = WaveSurfer.create({
					container: `#waveform-${programId}`,
					waveColor: 'rgb(147, 51, 234)',
					progressColor: 'rgb(168, 85, 247)',
					cursorColor: 'rgb(192, 132, 252)',
					barWidth: 2,
					barRadius: 3,
					height: 128,
					plugins: [regions]
				});

				// When waveform is loaded and decoded
				wavesurfer.on('decode', () => {
					isLoaded = true;

					// Restore pending cues if any
					if (window._pendingCues && window._pendingCues.length > 0) {
						window._pendingCues.forEach(cue => {
							const labelElement = createRegionLabel(cue.label, cue.time);
							const markerRegion = regions.addRegion({
								start: cue.time,
								content: labelElement,
								color: 'rgba(168, 85, 247, 0.3)',
								drag: true,
								resize: false
							});

							markers = [...markers, {
								id: markerRegion.id,
								time: cue.time,
								label: cue.label,
								boards: cue.boards,
								preset: cue.preset,
								effect: cue.effect,
								color: cue.color,
								brightness: cue.brightness,
								transition: cue.transition
							}];
						});
						window._pendingCues = null;
					}
				});

				// Track play/pause state
				wavesurfer.on('play', () => {
					isPlaying = true;
				});

				wavesurfer.on('pause', () => {
					isPlaying = false;
				});

				// Handle left-click (seek) and right-click (add marker) on waveform
				const waveformContainer = wavesurfer.getWrapper();
				waveformContainer.addEventListener('mousedown', (event) => {
					if (!isLoaded) return;

					// Prevent default context menu on right-click
					if (event.button === 2) {
						event.preventDefault();
					}

					const bounds = waveformContainer.getBoundingClientRect();
					const relativeX = (event.clientX - bounds.left) / bounds.width;
					const duration = wavesurfer.getDuration();
					const clickTime = relativeX * duration;

					if (event.button === 0) {
						// Left-click: Seek to position
						console.log('🖱️ Left-click: Seeking to', clickTime);
						wavesurfer.seekTo(relativeX);
					} else if (event.button === 2) {
						// Right-click: Add marker
						console.log('🖱️ Right-click: Adding marker at', clickTime);
						addMarker(clickTime);
					}
				});

				// Prevent context menu on right-click
				waveformContainer.addEventListener('contextmenu', (event) => {
					event.preventDefault();
				});

				// Update marker list when regions change
				regions.on('region-updated', (region) => {
					const markerIndex = markers.findIndex(m => m.id === region.id);
					if (markerIndex !== -1) {
						markers[markerIndex].time = region.start;
						markers = [...markers]; // Trigger reactivity
						syncMarkersToStore();
					}
				});

				regions.on('region-removed', (region) => {
					markers = markers.filter(m => m.id !== region.id);
					syncMarkersToStore();
				});

				const url = URL.createObjectURL(file);
				console.log('Loading URL:', url);
				window._currentAudioUrl = url; // Store for later reload
				wavesurfer.load(url);

				// Clear existing markers if not loading program
				if (!program) {
					markers = [];
				}
			}, 100);
		} else {
			alert('Please select an audio file (WAV, MP3, etc.)');
		}
	}

	function loadCompressedAudio(audioDataURL) {
		console.log('[Program.svelte] Loading compressed audio, data URL length:', audioDataURL?.length);
		console.log('[Program.svelte] programId:', programId);

		// Check if container exists
		const container = document.querySelector(`#waveform-${programId}`);
		if (!container) {
			console.error('[Program.svelte] Waveform container not found:', `#waveform-${programId}`);
			return;
		}
		console.log('[Program.svelte] Container found:', container);

		// Convert base64 data URL back to blob
		fetch(audioDataURL)
			.then(res => {
				console.log('[Program.svelte] fetch() response:', res.status, res.statusText);
				return res.blob();
			})
			.then(blob => {
				console.log('[Program.svelte] Blob created:', blob.size, 'bytes, type:', blob.type);

				// Create blob URL
				const url = URL.createObjectURL(blob);
				window._currentAudioUrl = url; // Store for later reload

				// Initialize Regions plugin
				regions = RegionsPlugin.create();

				try {

				// Create WaveSurfer instance
				wavesurfer = WaveSurfer.create({
					container: `#waveform-${programId}`,
					waveColor: 'rgb(147, 51, 234)',
					progressColor: 'rgb(168, 85, 247)',
					cursorColor: 'rgb(192, 132, 252)',
					barWidth: 2,
					barRadius: 3,
					height: 128,
					plugins: [regions]
				});

				// When waveform is loaded and decoded
				wavesurfer.on('decode', () => {
					isLoaded = true;

					// Restore pending cues if any
					if (window._pendingCues && window._pendingCues.length > 0) {
						window._pendingCues.forEach(cue => {
							const labelElement = createRegionLabel(cue.label, cue.time);
							const markerRegion = regions.addRegion({
								start: cue.time,
								content: labelElement,
								color: 'rgba(168, 85, 247, 0.3)',
								drag: true,
								resize: false
							});

							markers = [...markers, {
								id: markerRegion.id,
								time: cue.time,
								label: cue.label,
								boards: cue.boards,
								preset: cue.preset,
								effect: cue.effect,
								color: cue.color,
								brightness: cue.brightness,
								transition: cue.transition
							}];
						});
						window._pendingCues = null;
					}
				});

				// Track play/pause state
				wavesurfer.on('play', () => {
					isPlaying = true;
				});

				wavesurfer.on('pause', () => {
					isPlaying = false;
				});

				// Handle left-click (seek) and right-click (add marker) on waveform
				const waveformContainer = wavesurfer.getWrapper();
				waveformContainer.addEventListener('mousedown', (event) => {
					if (!isLoaded) return;

					// Prevent default context menu on right-click
					if (event.button === 2) {
						event.preventDefault();
					}

					const bounds = waveformContainer.getBoundingClientRect();
					const relativeX = (event.clientX - bounds.left) / bounds.width;
					const duration = wavesurfer.getDuration();
					const clickTime = relativeX * duration;

					if (event.button === 0) {
						// Left-click: Seek to position
						console.log('🖱️ Left-click: Seeking to', clickTime);
						wavesurfer.seekTo(relativeX);
					} else if (event.button === 2) {
						// Right-click: Add marker
						console.log('🖱️ Right-click: Adding marker at', clickTime);
						addMarker(clickTime);
					}
				});

				// Prevent context menu on right-click
				waveformContainer.addEventListener('contextmenu', (event) => {
					event.preventDefault();
				});

				// Update marker list when regions change
				regions.on('region-updated', (region) => {
					const markerIndex = markers.findIndex(m => m.id === region.id);
					if (markerIndex !== -1) {
						markers[markerIndex].time = region.start;
						markers = [...markers]; // Trigger reactivity
						syncMarkersToStore();
					}
				});

				regions.on('region-removed', (region) => {
					markers = markers.filter(m => m.id !== region.id);
					syncMarkersToStore();
				});

					// Load the compressed audio (WaveSurfer will decode it)
					console.log('[Program.svelte] Loading audio URL into WaveSurfer:', url);
					wavesurfer.load(url);
				} catch (err) {
					console.error('[Program.svelte] Failed to create WaveSurfer instance:', err);
				}
			})
			.catch(err => {
				console.error('[Program.svelte] Failed to load compressed audio:', err);
			});
	}

	// Helper function to create styled label elements with vertical staggering
	function createRegionLabel(text, time) {
		const label = document.createElement('div');
		label.textContent = text;
		label.title = text; // Tooltip for full text

		// Calculate vertical offset based on nearby markers
		// No nearby markers: center the label (0px)
		// Nearby markers exist: use staggering (5px / 25px below)
		const overlapping = markers.filter(m => Math.abs(m.time - time) < 2.0);
		let verticalOffset;
		if (overlapping.length === 0) {
			// No nearby markers - center the label
			verticalOffset = 0;
		} else {
			// Nearby markers exist - use staggering
			verticalOffset = (overlapping.length % 2 === 0) ? 5 : 25;
		}

		label.style.cssText = `
			position: absolute;
			transform: translateY(${verticalOffset}px);
			background-color: rgba(20, 20, 20, 0.95);
			color: #e5e5e5 !important;
			padding: 3px 8px;
			border-radius: 4px;
			font-size: 11px;
			font-weight: 500;
			font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
			white-space: nowrap;
			overflow: hidden;
			text-overflow: ellipsis;
			max-width: 120px;
			min-height: 18px;
			line-height: 1.2;
			border: 1px solid rgba(168, 85, 247, 0.5);
			box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
			pointer-events: none;
			display: block;
		`;

		return label;
	}

	/**
	 * Sync markers to the program in the store immediately.
	 * This ensures that:
	 * 1. Play button uses the latest cues (reads from store)
	 * 2. Cues are available for playback without clicking Save first
	 * 3. Save button only needs to persist store data to backend API
	 */
	function syncMarkersToStore() {
		if (!programId) return;

		// Update the program in the store with current markers
		programsStore.update(programs => {
			const programIndex = programs.findIndex(p => p.id === programId);
			if (programIndex !== -1) {
				const updatedProgram = programs[programIndex];
				updatedProgram.cues = markers;
				programs[programIndex] = updatedProgram;
			}
			return [...programs];
		});
	}

	function addMarker(time) {
		console.log('📍 addMarker called with time:', time);
		const currentCount = markers.length;
		const labelText = `Cue ${currentCount + 1}`;
		const labelElement = createRegionLabel(labelText, time);

		const markerRegion = regions.addRegion({
			start: time,
			content: labelElement,
			color: 'rgba(168, 85, 247, 0.3)',
			drag: true,
			resize: false
		});

		// Inherit default target board if set, otherwise empty
		const initialBoards = defaultTargetBoard ? [defaultTargetBoard] : [];

		const newMarker = {
			id: markerRegion.id,
			time: time,
			label: `Cue ${currentCount + 1}`,
			boards: initialBoards,
			preset: 0,
			effect: 0,
			color: '#ff0000',
			brightness: 255,
			transition: 0
		};

		markers = [...markers, newMarker];
		syncMarkersToStore();
	}

	function updateMarkerEffect(markerId, effectIndex) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.effect = effectIndex;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function updateMarkerColor(markerId, color) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.color = color;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function updateMarkerPreset(markerId, presetSlot) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.preset = presetSlot;

			// Update label to preset name (find by wled_slot)
			const preset = $presets.find(p => p.id === presetSlot);
			if (preset) {
				console.log('Updating marker with preset slot:', markerId, presetSlot, preset.name);
				marker.label = preset.name;

				// Update WaveSurfer region label (HTML element)
				if (regions) {
					const allRegions = regions.getRegions();
					console.log('All regions:', allRegions.map(r => ({ id: r.id, content: r.content })));
					const region = allRegions.find(r => r.id === markerId);
					if (region && region.content) {
						console.log('Updating region content to:', preset.name);
						// Update the text content of the HTML element
						if (region.content.textContent !== undefined) {
							region.content.textContent = preset.name;
							region.content.title = preset.name; // Update tooltip too
						}
					} else {
						console.warn('Region not found for marker:', markerId);
					}
				} else {
					console.warn('Regions plugin not available');
				}
			}

			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function updateMarkerBrightness(markerId, brightness) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.brightness = brightness;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function updateMarkerTransition(markerId, transition) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.transition = transition;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function updateMarkerBoards(markerId, selectedBoards) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.boards = selectedBoards;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function updateMarkerLabel(markerId, newLabel) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.label = newLabel;
			markers = [...markers];

			// Update WaveSurfer region label (HTML element)
			if (regions) {
				const allRegions = regions.getRegions();
				const region = allRegions.find(r => r.id === markerId);
				if (region && region.content) {
					console.log('Updating region label:', markerId, newLabel);
					// Update the text content of the HTML element
					if (region.content.textContent !== undefined) {
						region.content.textContent = newLabel;
						region.content.title = newLabel; // Update tooltip too
					}
				} else {
					console.warn('Region not found:', markerId);
				}
			} else {
				console.warn('Regions plugin not available');
			}
			syncMarkersToStore();
		}
	}

	function toggleBoardSelection(markerId, boardId) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			if (marker.boards.includes(boardId)) {
				marker.boards = marker.boards.filter(id => id !== boardId);
			} else {
				marker.boards = [...marker.boards, boardId];
			}
			markers = [...markers];
		}
	}

	function toggleDropdown(markerId) {
		openDropdownId = openDropdownId === markerId ? null : markerId;
	}

	function getBoardsLabel(selectedBoards) {
		if (selectedBoards.length === 0) return 'Select boards...';
		if (selectedBoards.length === 1) {
			const board = $boards.find(b => b.id === selectedBoards[0]);
			return board ? board.id : '1 selected';
		}
		return `${selectedBoards.length} selected`;
	}

	function deleteMarker(markerId) {
		const allRegions = regions.getRegions();
		const region = allRegions.find(r => r.id === markerId);
		if (region) {
			region.remove();
		}
	}

	function formatTime(seconds) {
		const mins = Math.floor(seconds / 60);
		const secs = (seconds % 60).toFixed(2);
		return `${mins}:${secs.padStart(5, '0')}`;
	}


function playFullProgram() {
		// Get the current program data from store
		let currentProgram = null;
		const unsubscribe = programsStore.subscribe(programs => {
			currentProgram = programs.find(p => p.id === programId);
		});
		unsubscribe();

		if (!currentProgram) return;

		// Get current playback position
		const currentTime = wavesurfer ? wavesurfer.getCurrentTime() : 0;
		console.log('▶️ PLAY pressed - starting from position:', currentTime);

		// Capture the EXACT moment audio starts
		const audioStartTime = performance.now();

		// Play from current position (or from start if at beginning)
		if (wavesurfer) {
			wavesurfer.play();
		}

		// IMMEDIATELY schedule LED cues with audio start timestamp
		playProgramService(currentProgram, currentTime, audioStartTime);
	}

	function stopFullProgram() {
		const pausePosition = wavesurfer ? wavesurfer.getCurrentTime() : 0;
		console.log('⏸ PAUSE pressed - paused at position:', pausePosition);

		if (wavesurfer) {
			wavesurfer.pause();
		}

		// Stop global playback
		stopPlaybackService();

		// Dim THIS program's boards (fire-and-forget, don't block UI)
		let currentProgram = null;
		const unsubscribe = programsStore.subscribe(programs => {
			currentProgram = programs.find(p => p.id === programId);
		});
		unsubscribe();

		if (currentProgram) {
			dimProgramBoards(currentProgram).catch(err => {
				console.error('Failed to dim boards:', err);
			});
		}
	}

	function stopAndReset() {
		const beforePosition = wavesurfer ? wavesurfer.getCurrentTime() : 0;
		console.log('⏹ STOP pressed - position before:', beforePosition);

		// Stop global playback
		stopPlaybackService();

		// Dim THIS program's boards (fire-and-forget, don't block UI)
		let currentProgram = null;
		const unsubscribe = programsStore.subscribe(programs => {
			currentProgram = programs.find(p => p.id === programId);
		});
		unsubscribe();

		if (currentProgram) {
			dimProgramBoards(currentProgram).catch(err => {
				console.error('Failed to dim boards:', err);
			});
		}

		// Stop and reset to start IMMEDIATELY (don't wait for dimming)
		if (wavesurfer) {
			wavesurfer.stop();  // Should stop playback and reset to 0
			setTimeout(() => {
				const afterPosition = wavesurfer.getCurrentTime();
				console.log('⏹ Position after stop():', afterPosition);
			}, 50);
		}
	}

	function saveProgram() {
		// Validation
		if (!songName.trim()) {
			alert('Please enter a song name');
			return;
		}

		// Check if any cue has no boards selected
		const cuesWithoutBoards = markers.filter(m => m.boards.length === 0);
		if (cuesWithoutBoards.length > 0) {
			const confirmed = confirm(
				`${cuesWithoutBoards.length} cue(s) have no boards selected. Save anyway?`
			);
			if (!confirmed) return;
		}

		// Generate unique ID or use existing
		const timestamp = Date.now();
		const sanitizedSongName = songName.trim().replace(/\s+/g, '-').toLowerCase();
		const trackSuffix = loopyProTrack.trim() ? `-${loopyProTrack.trim()}` : '';
		const newProgramId = programId || `${sanitizedSongName}${trackSuffix}-${timestamp}`;

		// Get existing program data to preserve audioData
		let existingProgram = null;
		if (programId) {
			programsStore.subscribe(programs => {
				existingProgram = programs.find(p => p.id === programId);
			})();
		}

		// Create program data
		const programData = {
			id: newProgramId,
			songName: songName.trim(),
			loopyProTrack: loopyProTrack.trim(),
			fileName: fileName,
			audioData: existingProgram?.audioData || program?.audioData || '', // Preserve audio data
			cues: markers.map(m => ({
				time: m.time,
				label: m.label,
				boards: m.boards,
				preset: m.preset,
				color: m.color,
				effect: m.effect,
				brightness: m.brightness,
				transition: m.transition
			})),
			createdAt: existingProgram?.createdAt || new Date().toISOString()
		};

		// Create Program model using factory
		const program = ProgramModel.fromJson(programData);

		if (program) {
			// Save through service layer - store will update automatically
			saveProgramToStore(program);

			// Update local programId if new
			if (!programId) {
				programId = newProgramId;
			}
		}
	}

	function clearCues() {
		if (markers.length === 0) return;

		// Remove all regions from waveform
		const allRegions = regions.getRegions();
		allRegions.forEach(region => region.remove());

		// Clear markers array
		markers = [];
		syncMarkersToStore();
	}

	function deleteProgram() {
		if (!programId) return;

		const confirmed = confirm(
			`Are you sure you want to delete "${songName}"? This cannot be undone.`
		);

		if (confirmed) {
			// Delete through service layer - store will update automatically
			deleteProgramFromStore(programId);
		}
	}

	function applyDefaultBoardToAll() {
		if (!defaultTargetBoard) {
			alert('Please select a default target board first');
			return;
		}

		// Apply default board to ALL cues
		markers = markers.map(marker => ({
			...marker,
			boards: [defaultTargetBoard]
		}));

		syncMarkersToStore();
	}

	function selectDefaultBoard(boardId) {
		defaultTargetBoard = boardId;
		defaultBoardDropdownOpen = false;
	}

	function getDefaultBoardLabel() {
		if (!defaultTargetBoard) return 'Default';
		return defaultTargetBoard;
	}
</script>

<div class="program-editor">
	<div class="waveform-container">
		<div class="waveform-header">
			{#if isPlaying}
				<button class="btn-program-pause" onclick={stopFullProgram}>
					⏸
				</button>
			{:else}
				<button class="btn-program-play" onclick={playFullProgram}>
					▶
				</button>
			{/if}
			<button class="btn-program-stop" onclick={stopAndReset} title="Stop and reset to start">
				⏹
			</button>
			<input
				type="text"
				bind:value={songName}
				placeholder="Song name"
				class="song-name-input"
			/>
			<input
				type="text"
				bind:value={loopyProTrack}
				placeholder="Track"
				class="track-input"
				maxlength="2"
			/>
			<span class="file-name">{fileName}</span>
			<div class="spacer"></div>
			<button class="btn-save" onclick={saveProgram} title="Save program">
				<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
					<path d="M12.5 14.5h-9c-.55 0-1-.45-1-1v-11c0-.55.45-1 1-1h6.88l3.62 3.62v8.38c0 .55-.45 1-1 1z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
					<path d="M5.5 1.5v4h5v-4M10.5 14.5v-5h-5v5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
				</svg>
			</button>
			{#if programId}
				<button class="btn-delete-program" onclick={deleteProgram} title="Delete program">
					<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
						<path d="M2 4h12M5.5 4V2.5h5V4M6.5 7.5v4M9.5 7.5v4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
						<path d="M3.5 4l.5 9.5c0 .55.45 1 1 1h6c.55 0 1-.45 1-1L13 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</button>
			{/if}
		</div>
		<div class="waveform-wrapper">
			{#if !isLoaded && program?.audioData}
				<div class="waveform-skeleton"></div>
			{/if}
			<div id="waveform-{programId}" class:hidden={!isLoaded && program?.audioData}></div>
		</div>
		<div class="waveform-footer" class:has-cues={isLoaded && markers.length > 0}>
			{#if isLoaded && markers.length > 0}
				{@const groups = $boards.filter(b => b.isGroup)}
				{@const regularBoards = $boards.filter(b => !b.isGroup)}

				<div class="default-board-dropdown-wrapper">
					<button
						class="default-board-select-button"
						onclick={(e) => {
							e.stopPropagation();
							defaultBoardDropdownOpen = !defaultBoardDropdownOpen;
						}}
					>
						{getDefaultBoardLabel()}
						<span class="dropdown-arrow">▼</span>
					</button>
					{#if defaultBoardDropdownOpen}
						<div class="default-board-dropdown-menu">
							{#if groups.length > 0}
								<div class="dropdown-section">
									<div class="dropdown-section-label">GROUPS</div>
									{#each groups as group}
										<label class="dropdown-option">
											<input
												type="checkbox"
												checked={defaultTargetBoard === group.id}
												onchange={() => selectDefaultBoard(group.id)}
											/>
											<span>{group.id}</span>
										</label>
									{/each}
								</div>
							{/if}

							{#if regularBoards.length > 0}
								<div class="dropdown-section">
									<div class="dropdown-section-label">BOARDS</div>
									{#each regularBoards as board}
										<label class="dropdown-option">
											<input
												type="checkbox"
												checked={defaultTargetBoard === board.id}
												onchange={() => selectDefaultBoard(board.id)}
											/>
											<span>{board.id}</span>
										</label>
									{/each}
								</div>
							{/if}
						</div>
					{/if}
				</div>
				<button
					class="btn-apply-default"
					onclick={applyDefaultBoardToAll}
					disabled={!defaultTargetBoard}
				>
					Apply to All Cues
				</button>

				<button class="btn-collapse" onclick={() => cuesExpanded = !cuesExpanded}>
					<span>{cuesExpanded ? '▼' : '▶'} Cues</span>
				</button>
				<button class="cue-count-badge-wrapper" onclick={clearCues}>
					<span class="cue-count-badge">{markers.length}</span>
					<span class="clear-cues-text">Clear Cues</span>
				</button>
			{/if}
		</div>
		{#if !isLoaded && !program?.audioData}
			<div class="audio-missing">
				<p>⚠️ Audio file missing</p>
				<p class="audio-missing-hint">This program was saved without audio. Please re-upload the file.</p>
			</div>
		{/if}

		{#if markers.length > 0}
			{#if cuesExpanded}
			<div class="markers-section">
				<div class="markers-list">
					{#each [...markers].sort((a, b) => a.time - b.time) as marker (marker.id)}
						<div class="marker-item">
							<div class="marker-info">
								<span class="marker-time">{formatTime(marker.time)}</span>
								<input
									type="text"
									value={marker.label}
									oninput={(e) => updateMarkerLabel(marker.id, e.target.value)}
									class="marker-label-input"
									placeholder="Cue label"
								/>
							</div>
							<div class="marker-controls">
								<div class="boards-dropdown-wrapper">
									<button
										class="boards-select-button"
										onclick={(e) => {
											e.stopPropagation();
											toggleDropdown(marker.id);
										}}
									>
										{getBoardsLabel(marker.boards)}
										<span class="dropdown-arrow">▼</span>
									</button>
									{#if openDropdownId === marker.id}
										{@const regularBoards = $boards.filter(b => !b.isGroup)}
										{@const groups = $boards.filter(b => b.isGroup)}

										<div class="boards-dropdown-menu">
											{#if groups.length > 0}
												<div class="dropdown-section">
													<div class="dropdown-section-label">Groups</div>
													{#each groups as group}
														<label class="dropdown-option">
															<input
																type="checkbox"
																checked={marker.boards.includes(group.id)}
																onchange={() => toggleBoardSelection(marker.id, group.id)}
															/>
															<span>{group.id}</span>
														</label>
													{/each}
												</div>
											{/if}

											{#if regularBoards.length > 0}
												<div class="dropdown-section">
													<div class="dropdown-section-label">Boards</div>
													{#each regularBoards as board}
														<label class="dropdown-option">
															<input
																type="checkbox"
																checked={marker.boards.includes(board.id)}
																onchange={() => toggleBoardSelection(marker.id, board.id)}
															/>
															<span>{board.id}</span>
														</label>
													{/each}
												</div>
											{/if}
										</div>
									{/if}
								</div>

								<select
									value={marker.preset}
									onchange={(e) => updateMarkerPreset(marker.id, parseInt(e.target.value))}
									class="preset-select"
								>
									{#each $presets as preset}
										<option value={preset.id}>{preset.name}</option>
									{/each}
								</select>

								<div class="transition-input-wrapper">
									<input
										type="text"
										inputmode="numeric"
										pattern="[0-9]*"
										min="0"
										max="100"
										value={marker.transition + ' ms'}
										oninput={(e) => {
											const val = e.target.value.replace(/\D/g, '');
											updateMarkerTransition(marker.id, parseInt(val) || 0);
											e.target.value = val + ' ms';
										}}
										class="transition-input"
									/>
								</div>

								<button class="btn-delete" onclick={() => deleteMarker(marker.id)}>
									✕
								</button>
							</div>
						</div>
					{/each}
				</div>
			</div>
			{/if}
		{/if}
	</div>
</div>

<style>
	.program-editor {
		width: 100%;
	}

	.waveform-container {
		background-color: #1a1a1a;
		border-radius: 12px;
		border: 1px solid #2a2a2a;
		overflow: visible;
	}

	.waveform-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 1rem 1.5rem;
		background: linear-gradient(to bottom, #1f1f1f, #1a1a1a);
	}

	.spacer {
		flex: 1;
	}

	.btn-program-play,
	.btn-program-pause,
	.btn-program-stop {
		padding: 0.5rem 1rem;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		font-size: 1rem;
		cursor: pointer;
		transition: all 0.2s;
		background-color: #1a1a1a;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		box-sizing: border-box;
	}

	.btn-program-play {
		color: #10b981;
	}

	.btn-program-play:hover {
		background-color: #2a2a2a;
		border-color: #10b981;
		transform: translateY(-1px);
	}

	.btn-program-pause {
		color: #f59e0b;
	}

	.btn-program-pause:hover {
		background-color: #2a2a2a;
		border-color: #f59e0b;
		transform: translateY(-1px);
	}

	.btn-program-stop {
		color: #ef4444;
	}

	.btn-program-stop:hover {
		background-color: #2a2a2a;
		border-color: #ef4444;
		transform: translateY(-1px);
	}

	.song-name-input {
		flex: 0 0 250px;
		background-color: #0f0f0f;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.9rem;
		transition: border-color 0.2s;
	}

	.song-name-input:hover {
		border-color: #a855f7;
	}

	.song-name-input:focus {
		outline: none;
		border-color: #a855f7;
	}

	.song-name-input::placeholder {
		color: #6b7280;
	}

	.track-input {
		width: 45px;
		background-color: #0f0f0f;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.5rem 0.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		text-align: center;
		transition: border-color 0.2s;
	}

	.track-input:hover {
		border-color: #a855f7;
	}

	.track-input:focus {
		outline: none;
		border-color: #a855f7;
	}

	.track-input::placeholder {
		color: #6b7280;
	}

	.btn-collapse {
		background-color: #2a2a2a;
		color: #a0a0a0;
		border: 1px solid #3a3a3a;
		padding: 0.4rem 0.75rem;
		font-size: 0.8rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s ease;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		border-radius: 6px;
		height: 28px;
		box-sizing: border-box;
	}

	.btn-collapse:hover {
		background-color: #333;
		border-color: #555;
		color: #e5e5e5;
	}

	.cue-count-badge-wrapper {
		background-color: #2a2a2a;
		color: #e5e5e5;
		border: 1px solid #3a3a3a;
		border-radius: 16px;
		padding: 0;
		cursor: pointer;
		transition: all 0.3s ease;
		display: flex;
		align-items: center;
		overflow: hidden;
		height: 28px;
		min-width: 28px;
	}

	.cue-count-badge-wrapper:hover {
		padding-right: 0.75rem;
		background-color: #333;
		border-color: #e57373;
	}

	.cue-count-badge {
		color: #e5e5e5;
		width: 32px;
		height: 28px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: 0.75rem;
		font-weight: 600;
		flex-shrink: 0;
	}

	.clear-cues-text {
		color: #e57373;
		font-size: 0.75rem;
		font-weight: 500;
		white-space: nowrap;
		opacity: 0;
		max-width: 0;
		transition: all 0.3s ease;
		margin-left: 0;
	}

	.cue-count-badge-wrapper:hover .clear-cues-text {
		opacity: 1;
		max-width: 100px;
		margin-left: 0.5rem;
	}


	.file-name {
		font-size: 0.9rem;
		color: #9ca3af;
		font-weight: 400;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 200px;
		flex-shrink: 1;
	}


	.btn-save {
		background-color: #1a1a1a;
		color: #10b981;
		border: 1px solid #2a2a2a;
		padding: 0.5rem 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		height: 36px;
		box-sizing: border-box;
	}

	.btn-save:hover {
		background-color: #2a2a2a;
		border-color: #10b981;
		transform: translateY(-1px);
	}

	.btn-save:active {
		transform: translateY(0);
	}

	.btn-clear {
		background-color: #1a1a1a;
		color: #f59e0b;
		border: 1px solid #2a2a2a;
		padding: 0.5rem 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
	}

	.btn-clear:hover {
		background-color: #2a2a2a;
		border-color: #f59e0b;
		transform: translateY(-1px);
	}

	.btn-clear:active {
		transform: translateY(0);
	}

	.btn-delete-program {
		background-color: #1a1a1a;
		color: #ef4444;
		border: 1px solid #2a2a2a;
		padding: 0.5rem 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		height: 36px;
		box-sizing: border-box;
	}

	.btn-delete-program:hover {
		background-color: #2a2a2a;
		border-color: #ef4444;
		transform: translateY(-1px);
	}

	.btn-delete-program:active {
		transform: translateY(0);
	}

	.waveform-wrapper {
		position: relative;
		min-height: 176px;
	}

	.waveform-wrapper:has(+ .waveform-footer:not(.has-cues)) {
		margin-bottom: -40px;
	}

	div[id^="waveform-"] {
		padding: 1.5rem 2rem 1.5rem 2rem;
		min-height: 128px;
	}

	div[id^="waveform-"].hidden {
		opacity: 0;
		position: absolute;
		pointer-events: none;
	}

	.waveform-footer {
		padding: 0.5rem 1rem;
		background-color: #1a1a1a;
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 0.75rem;
		min-height: 40px;
		opacity: 0;
		transition: opacity 0.3s ease;
	}

	.waveform-footer.has-cues {
		opacity: 1;
	}

	.waveform-skeleton {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		margin: 1.5rem 2rem;
		height: 128px;
		background: linear-gradient(90deg, #1a1a1a 25%, #2a2a2a 50%, #1a1a1a 75%);
		background-size: 200% 100%;
		animation: shimmer 1.5s infinite;
		border-radius: 8px;
	}

	@keyframes shimmer {
		0% { background-position: 200% 0; }
		100% { background-position: -200% 0; }
	}

	.audio-missing {
		padding: 2rem;
		text-align: center;
		background-color: #1a1a1a;
	}

	.audio-missing p {
		color: #ef4444;
		font-size: 0.875rem;
		margin: 0.5rem 0;
	}

	.audio-missing-hint {
		color: #6b7280 !important;
		font-size: 0.8rem !important;
	}

	.markers-section {
		padding: 0.5rem 1rem;
		margin-top: 0.5rem;
	}

	.default-board-controls {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.default-board-label {
		font-size: 0.875rem;
		font-weight: 600;
		color: #a8a29e;
		white-space: nowrap;
	}

	.default-board-dropdown-wrapper {
		position: relative;
	}

	.default-board-select-button {
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0 2rem 0 0.75rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
		width: 140px;
		height: 28px;
		transition: border-color 0.2s;
		display: flex;
		align-items: center;
		position: relative;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		box-sizing: border-box;
	}

	.default-board-select-button:hover {
		border-color: #a855f7;
	}

	.default-board-select-button .dropdown-arrow {
		position: absolute;
		right: 0.75rem;
		font-size: 0.7rem;
		color: #9ca3af;
	}

	.default-board-dropdown-menu {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		border-radius: 6px;
		min-width: 200px;
		max-height: 300px;
		overflow-y: auto;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.default-board-dropdown-menu .dropdown-section {
		padding: 0.5rem 0;
	}

	.default-board-dropdown-menu .dropdown-section:last-child {
		padding-bottom: 0.5rem;
	}

	.default-board-dropdown-menu .dropdown-section-label {
		font-size: 0.75rem;
		font-weight: 600;
		color: #6b7280;
		margin-bottom: 0.25rem;
		padding: 0.25rem 0.5rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.default-board-dropdown-menu .dropdown-option {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		cursor: pointer;
		border-radius: 4px;
		transition: background-color 0.15s;
	}

	.default-board-dropdown-menu .dropdown-option:hover {
		background-color: #2a2a2a;
	}

	.default-board-dropdown-menu .dropdown-option input[type="checkbox"] {
		cursor: pointer;
	}

	.default-board-dropdown-menu .dropdown-option span {
		color: #e5e5e5;
		font-size: 0.875rem;
		flex: 1;
	}

	.btn-apply-default {
		padding: 0 1.4rem 0 0.75rem;
		background-color: #a855f7;
		color: white;
		border: none;
		border-radius: 6px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		white-space: nowrap;
		height: 28px;
		min-width: 105px;
		display: flex;
		align-items: center;
		justify-content: center;
		box-sizing: border-box;
	}

	.btn-apply-default:hover:not(:disabled) {
		background-color: #9333ea;
		transform: translateY(-1px);
	}

	.btn-apply-default:active:not(:disabled) {
		transform: translateY(0);
	}

	.btn-apply-default:disabled {
		background-color: #2a2a2a;
		color: #666;
		cursor: not-allowed;
	}

	.markers-list {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.marker-item {
		padding: 0.35rem 0;
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.marker-info {
		display: flex;
		gap: 1rem;
		align-items: center;
		flex: 1;
	}

	.marker-controls {
		display: flex;
		gap: 0.75rem;
		align-items: center;
	}

	.color-picker {
		width: 40px;
		height: 32px;
		border: 1px solid #2a2a2a;
		border-radius: 6px;
		cursor: pointer;
		background-color: transparent;
		padding: 0;
		overflow: hidden;
	}

	.color-picker::-webkit-color-swatch-wrapper {
		padding: 0;
		border: none;
	}

	.color-picker::-webkit-color-swatch {
		border: none;
		border-radius: 6px;
	}

	.color-picker::-moz-color-swatch {
		border: none;
		border-radius: 6px;
	}

	.color-picker:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.boards-dropdown-wrapper {
		position: relative;
	}

	.boards-select-button {
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.5rem 2rem 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
		width: 140px;
		transition: border-color 0.2s;
		display: flex;
		align-items: center;
		justify-content: space-between;
		position: relative;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.boards-select-button:hover {
		border-color: #a855f7;
	}

	.dropdown-arrow {
		position: absolute;
		right: 0.75rem;
		font-size: 0.7rem;
		color: #9ca3af;
	}

	.boards-dropdown-menu {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		border-radius: 6px;
		min-width: 200px;
		max-height: 300px;
		overflow-y: auto;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.dropdown-section {
		padding: 0.5rem 0;
	}

	.dropdown-section:not(:last-child) {
		border-bottom: 1px solid #2a2a2a;
	}

	.dropdown-section-label {
		padding: 0.5rem 0.75rem;
		font-size: 0.75rem;
		font-weight: 600;
		color: #6b7280;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.dropdown-option {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		cursor: pointer;
		transition: background-color 0.2s;
		user-select: none;
	}

	.dropdown-option:hover {
		background-color: #2a2a2a;
	}

	.dropdown-option input[type="checkbox"] {
		cursor: pointer;
	}

	.dropdown-option span {
		font-size: 0.875rem;
		color: #e5e5e5;
	}

	.preset-select {
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.5rem 2rem 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
		min-width: 100px;
		transition: border-color 0.2s;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg width='10' height='6' viewBox='0 0 10 6' fill='none' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M1 1L5 5L9 1' stroke='%23e5e5e5' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
	}

	.preset-select:hover {
		border-color: #a855f7;
	}

	.preset-select:focus {
		outline: none;
		border-color: #a855f7;
	}

	.brightness-slider {
		width: 80px;
		height: 4px;
		border-radius: 2px;
		background: linear-gradient(to right, #2a2a2a, #a855f7);
		outline: none;
		cursor: pointer;
		-webkit-appearance: none;
		appearance: none;
	}

	.brightness-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #a855f7;
		cursor: pointer;
		border: 2px solid #1a1a1a;
	}

	.brightness-slider::-moz-range-thumb {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: #a855f7;
		cursor: pointer;
		border: 2px solid #1a1a1a;
	}

	.transition-input-wrapper {
		display: inline-block;
	}

	.transition-input {
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.375rem 0.5rem;
		border-radius: 6px;
		font-size: 0.875rem;
		width: 52px;
		text-align: center;
		transition: border-color 0.2s;
	}

	.transition-input:hover {
		border-color: #a855f7;
	}

	.transition-input:focus {
		outline: none;
		border-color: #a855f7;
		background-color: #2a2a2a;
	}

	.effect-select {
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.5rem 2rem 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
		min-width: 150px;
		transition: border-color 0.2s;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg width='10' height='6' viewBox='0 0 10 6' fill='none' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M1 1L5 5L9 1' stroke='%23e5e5e5' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
	}

	.effect-select:hover {
		border-color: #a855f7;
	}

	.effect-select:focus {
		outline: none;
		border-color: #a855f7;
	}

	.effect-select:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.marker-time {
		font-family: 'Courier New', monospace;
		font-size: 1.1rem;
		color: #a855f7;
		font-weight: bold;
		min-width: 80px;
	}

	.marker-label-input {
		background-color: #0f0f0f;
		border: 1px solid #2a2a2a;
		color: #e5e5e5;
		padding: 0.375rem 0.625rem;
		border-radius: 6px;
		font-size: 1rem;
		transition: border-color 0.2s;
		width: 200px;
		max-width: 200px;
	}

	.marker-label-input:hover {
		border-color: #a855f7;
	}

	.marker-label-input:focus {
		outline: none;
		border-color: #a855f7;
		background-color: #1a1a1a;
	}

	.marker-label-input::placeholder {
		color: #6b7280;
	}

	.btn-delete {
		background-color: #1a1a1a;
		border: 1px solid #2a2a2a;
		color: #ef4444;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
		padding: 0.375rem 0.625rem;
		border-radius: 6px;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 32px;
	}

	.btn-delete:hover {
		background-color: #ef4444;
		color: white;
		border-color: #ef4444;
	}
</style>
