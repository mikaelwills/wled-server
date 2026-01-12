<script>
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import { page } from '$app/stores';
	import WaveSurfer from 'wavesurfer.js';
	import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js';
	import { API_URL } from '$lib/api';
	import { saveProgram as saveProgramToStore, deleteProgram as deleteProgramFromStore } from '$lib/programs-db';
	import { playProgram as playProgramService, stopPlayback as stopPlaybackService, pausePlayback as pausePlaybackService } from '$lib/playback-db';
	import { loadAudioForProgram, getCachedPeaks } from '$lib/audio-db';
	import { audioBlobUrls, audioLoading } from '$lib/store';
	import { Program as ProgramModel } from '$lib/models/Program';
	import { programs as programsStore, boards, performancePresets, patternPresets, currentlyPlayingProgram, lastActiveProgramId, gridMultiplier } from '$lib/store';
	import { WLED_EFFECTS } from '$lib/wled-effects';

	// Props
	let {
		program = null
	} = $props();

	// Internal state derived from prop
	let programId = $state(program?.id || null);
	// Sanitized version of programId for use in HTML IDs and CSS selectors (no spaces or special chars)
	let sanitizedProgramId = $derived(programId ? programId.replace(/[^a-zA-Z0-9-_]/g, '-') : null);

	let wavesurfer = $state(null);
	let regions = $state(null);
	let markers = $state([]);
	let fileName = $state('');
	let isLoaded = $state(false);
	let isPlaying = $state(false);
	let audioToUpload = $state(null);
	let wavesurferInitialized = $state(false);

	// Program metadata
	let songName = $state('');
	let loopyProTrack = $state('');
	let audioDuration = $state(null); // Duration in seconds (extracted from audio)
	let bpm = $state(null); // BPM for speed-synced effects
	let gridOffset = $state(0); // Downbeat position - where beat 1 of bar 1 starts

	// Preset picker modal state
	let presetPicker = $state({
		open: false,
		markerId: null,
		step: 'category', // 'category' or 'color'
		selectedCategory: null
	});

	const quickPresets = ['Off', 'Flash'];

	const allCategories = $derived(() => {
		const categoryMap = new Map();
		$performancePresets.forEach(p => {
			if (quickPresets.includes(p.name)) return;
			const parts = p.name.split(' ');
			if (parts.length >= 2) {
				const category = parts.slice(0, -1).join(' ');
				categoryMap.set(category, false);
			}
		});
		$patternPresets.forEach(p => {
			const parts = p.name.split(' ');
			if (parts.length >= 2) {
				const category = parts.slice(0, -1).join(' ');
				categoryMap.set(category, true);
			}
		});
		return Array.from(categoryMap.entries())
			.map(([name, isPattern]) => ({ name, isPattern }))
			.sort((a, b) => a.name.localeCompare(b.name));
	});

	const categoryColors = $derived(() => {
		if (!presetPicker.selectedCategory) return [];
		const categoryInfo = allCategories().find(c => c.name === presetPicker.selectedCategory);
		const isPattern = categoryInfo?.isPattern ?? false;
		const presets = isPattern ? $patternPresets : $performancePresets;
		return presets
			.filter(p => p.name.startsWith(presetPicker.selectedCategory + ' '))
			.map(p => ({
				name: p.name,
				color: p.name.split(' ').pop(),
				isPattern
			}));
	});

	function openPresetPicker(markerId) {
		presetPicker = {
			open: true,
			markerId,
			step: 'category',
			selectedCategory: null
		};
	}

	function selectCategory(category) {
		presetPicker.selectedCategory = category;
		presetPicker.step = 'color';
	}

	function selectPreset(presetName) {
		if (presetPicker.markerId) {
			updateMarkerPreset(presetPicker.markerId, presetName);
		}
		closePresetPicker();
	}

	function closePresetPicker() {
		presetPicker = {
			open: false,
			markerId: null,
			step: 'category',
			selectedCategory: null
		};
	}

	function getColorStyle(colorName) {
		const colorMap = {
			'Red': '#ef4444',
			'Orange': '#f97316',
			'Yellow': '#eab308',
			'Green': '#22c55e',
			'Cyan': '#06b6d4',
			'Blue': '#3b82f6',
			'Purple': '#a855f7',
			'Pink': '#ec4899',
			'White': '#ffffff',
			'Warm': '#fbbf24',
			'Cool': '#93c5fd'
		};
		return colorMap[colorName] || '#e5e5e5';
	}

	// Snap time to nearest grid line based on BPM and offset (only if within 10px)
	function snapToGrid(time) {
		if (!bpm || bpm <= 0 || !wavesurfer || !audioDuration) return time;

		const barInterval = (60 / bpm) * 4;
		const gridInterval = (barInterval * 4) / $gridMultiplier;

		// Calculate snap threshold in time (25px worth)
		// zoomLevel is minPxPerSec - when 0, calculate from container width
		const wrapper = wavesurfer.getWrapper();
		if (!wrapper) return time;
		const pixelsPerSecond = zoomLevel > 0 ? zoomLevel : wrapper.clientWidth / audioDuration;
		const snapThresholdTime = 25 / pixelsPerSecond;

		// Find nearest grid line relative to offset
		const relativeTime = time - gridOffset;
		const nearestGridRelative = Math.round(relativeTime / gridInterval) * gridInterval;
		const nearestGridTime = gridOffset + nearestGridRelative;

		// Only snap if within threshold
		if (Math.abs(time - nearestGridTime) <= snapThresholdTime) {
			return nearestGridTime;
		}
		return time;
	}

	// Find the scroll container inside WaveSurfer's DOM
	function getScrollContainer() {
		const wrapper = wavesurfer?.getWrapper();
		if (!wrapper) return null;
		const findScrollable = (el) => {
			const style = window.getComputedStyle(el);
			if (style.overflowX === 'auto' || style.overflowX === 'scroll') return el;
			for (const child of el.children) {
				const found = findScrollable(child);
				if (found) return found;
			}
			return null;
		};
		return findScrollable(wrapper) || wrapper.querySelector('div');
	}

	// Store grid region IDs so we can remove them on update
	let gridRegionIds = [];

	// Render beat grid using Regions plugin (syncs with zoom/scroll automatically)
	function updateBeatGrid() {
		// Remove existing grid regions first
		gridRegionIds.forEach(id => {
			const allRegions = regions?.getRegions();
			const region = allRegions?.find(r => r.id === id);
			if (region) region.remove();
		});
		gridRegionIds = [];

		// Don't show grid when fully zoomed out or missing required data
		if (!wavesurfer || !regions || !bpm || bpm <= 0 || !audioDuration || audioDuration <= 0 || zoomLevel === 0) {
			return;
		}

		// Calculate grid intervals
		const beatInterval = 60 / bpm;
		const barInterval = beatInterval * 4;
		const gridInterval = (barInterval * 4) / $gridMultiplier;

		// Generate grid positions from offset, going both forward and backward
		const gridPositions = [];

		// Forward from offset
		for (let t = gridOffset; t <= audioDuration; t += gridInterval) {
			if (t >= 0) gridPositions.push(t);
		}

		// Backward from offset (excluding offset itself)
		for (let t = gridOffset - gridInterval; t >= 0; t -= gridInterval) {
			gridPositions.push(t);
		}

		// Create grid line regions
		gridPositions.forEach(t => {
			// Calculate bar number relative to offset
			const relativeTime = t - gridOffset;
			const barNumber = Math.round(relativeTime / barInterval);
			const isDownbeat = barNumber % 4 === 0;

			const region = regions.addRegion({
				start: t,
				end: t,
				color: isDownbeat ? 'rgba(255, 255, 255, 0.15)' : 'rgba(255, 255, 255, 0.06)',
				drag: false,
				resize: false
			});
			gridRegionIds.push(region.id);
		});

		console.log('📐 Grid:', { bpm, offset: gridOffset.toFixed(2), mult: $gridMultiplier, lines: gridRegionIds.length });
	}

	// Dropdown state
	let openDropdownId = $state(null);

	// Currently selected marker
	let currentlySelectedMarker = $state(null);

	// Default target board for new cues
	let defaultTargetBoard = $state(null);
	let defaultBoardDropdownOpen = $state(false);

	// Zoom state (0 = fit to container, >0 = pixels per second)
	let zoomLevel = $state(0);

	// Seeking state for debouncing
	let seekDebounceTimeout = null;
	let lastSeekTime = 0;

	// Pending cues to restore after audio loads (component-scoped, not global)
	let pendingCues = [];

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
				hasAudioId: !!program.audioId,
				hasAudioData: !!program.audioData,
				audioId: program.audioId
			});

			loadProgramData(program);

			// Legacy embedded audio (rare case)
			if (!program.audioId && program.audioData) {
				console.log(`[Program.svelte] Loading legacy embedded audio`);
				setTimeout(() => {
					loadCompressedAudio(program.audioData);
				}, 50);
			}
			// Modern programs: audio loading handled by reactive $effect below
		}

		// Close dropdown when clicking outside
		const handleClickOutside = (e) => {
			if (!e.target.closest('.boards-dropdown-wrapper')) {
				openDropdownId = null;
			}
		};
		document.addEventListener('click', handleClickOutside);

		// Keyboard handler for play/pause (Space) and add cue (C)
		function handleKeyPress(event) {
			// Only respond on the programming page
			const currentPath = get(page).url.pathname;
			if (currentPath !== '/programming') return;

			// Check if this is the last active program
			const lastActiveId = get(lastActiveProgramId);
			if (lastActiveId !== programId) return;

			// Only respond if this program has audio loaded
			if (!wavesurfer || !isLoaded) return;

			// Handle spacebar - play/pause
			if (event.code === 'Space') {
				event.preventDefault();

				// Toggle play/pause
				if (isPlaying) {
					stopFullProgram();
				} else {
					playFullProgram();
				}
			}
			// Handle 'C' key - add cue marker at current playhead position
			else if (event.code === 'KeyC') {
				event.preventDefault();

				// Get current playhead position
				const currentTime = wavesurfer.getCurrentTime();

				// Add marker at this position
				addMarker(currentTime);

				// Select the newly created marker (it's the last one added)
				const newMarker = markers[markers.length - 1];
				if (newMarker) {
					currentlySelectedMarker = newMarker.id;
				}

				console.log(`🎯 Added cue marker at ${currentTime.toFixed(3)}s via 'C' key`);
			}
		}

		// Add keyboard listener
		document.addEventListener('keydown', handleKeyPress);

		return () => {
			if (wavesurfer) {
				wavesurfer.destroy();
			}
			document.removeEventListener('click', handleClickOutside);
			document.removeEventListener('keydown', handleKeyPress);
		};
	});

	function loadProgramData(data) {
		songName = data.songName || '';
		loopyProTrack = data.loopyProTrack || '';
		fileName = data.fileName || '';
		defaultTargetBoard = data.defaultTargetBoard || null;
		bpm = data.bpm || null;
		gridOffset = data.gridOffset || 0;
		// Note: cues will need to be restored after audio file is loaded
		// Store them temporarily in component-scoped variable
		pendingCues = data.cues || [];
	}

	/**
	 * Initialize WaveSurfer instance with all event handlers
	 * @param {string} audioUrl - Blob URL of the audio file
	 */
	function initializeWaveSurfer(audioUrl) {
		regions = RegionsPlugin.create();

		// Check for cached peaks once at the start
		const cached = programId ? getCachedPeaks(programId) : null;

		wavesurfer = WaveSurfer.create({
			container: `#waveform-${sanitizedProgramId}`,
			waveColor: 'rgba(139, 92, 246, 0.5)',
			progressColor: 'rgba(139, 92, 246, 0.8)',
			cursorColor: 'rgba(167, 139, 250, 0.9)',
			barWidth: 2,
			barRadius: 3,
			height: 120,
			plugins: [regions]
		});

		wavesurfer.on('decode', () => {
			isLoaded = true;

			const duration = wavesurfer.getDuration();
			if (duration && duration > 0) {
				audioDuration = duration;
				updateBeatGrid();
			}

			// Restore pending cues if any
			if (pendingCues && pendingCues.length > 0) {
				pendingCues.forEach(cue => {
					// Migrate legacy preset ID to preset name
					let presetName = cue.presetName;
					if (!presetName && cue.preset && cue.preset > 0) {
						const preset = $performancePresets.find(p => p.id === cue.preset);
						if (preset) {
							presetName = preset.name;
							console.log(`📦 Migrated cue preset ID ${cue.preset} → "${presetName}"`);
						}
					}

					// Create region first to get ID
					const markerRegion = regions.addRegion({
						start: cue.time,
						content: document.createElement('div'), // Temporary placeholder
						color: 'rgba(168, 85, 247, 0.3)',
						drag: true,
						resize: false
					});

					// Now create label with the region ID and replace content
					const labelElement = createRegionLabel(cue.label, cue.time, markerRegion.id);
					markerRegion.element.replaceChildren(labelElement);

					// Force style reapplication AFTER WaveSurfer's avoidOverlapping() runs (10ms)
					setTimeout(() => {
						if (labelElement.parentElement) {
							labelElement.style.marginTop = ''; // Remove plugin's marginTop
							labelElement.style.position = 'absolute';
							labelElement.style.top = '50%';
							labelElement.style.transform = 'translateY(-50%)';
						}
					}, 20);

					markers = [...markers, {
						id: markerRegion.id,
						time: cue.time,
						label: cue.label,
						boards: cue.boards,
						presetName: presetName,
						preset: cue.preset,
						effect: cue.effect,
						color: cue.color,
						brightness: cue.brightness,
						syncRate: cue.syncRate ?? 1
					}];
				});
				pendingCues = [];
			}
		});

		// Track play/pause state
		wavesurfer.on('play', () => {
			isPlaying = true;
		});

		wavesurfer.on('pause', () => {
			isPlaying = false;
		});

		// Handle seeking during playback - reschedule cues from new position (debounced)
		wavesurfer.on('seeking', (currentTime) => {
			// Check if this program is currently playing
			let currentProgram = null;
			const unsub = currentlyPlayingProgram.subscribe(p => {
				currentProgram = p;
			});
			unsub();

			// Only reschedule if THIS program is playing
			if (currentProgram && currentProgram.id === programId && isPlaying) {
				// Clear any pending reschedule
				if (seekDebounceTimeout) {
					clearTimeout(seekDebounceTimeout);
				}

				// Only reschedule if seek distance is significant (> 0.5s from last processed seek)
				const seekDistance = Math.abs(currentTime - lastSeekTime);

				// Debounce: wait 150ms after user stops seeking before rescheduling
				seekDebounceTimeout = setTimeout(() => {
					console.log(`⏩ Seeking to ${currentTime.toFixed(2)}s during playback - rescheduling cues`);
					lastSeekTime = currentTime;

					// Get the current program data
					let program = null;
					const unsubPrograms = programsStore.subscribe(programs => {
						program = programs.find(p => p.id === programId);
					});
					unsubPrograms();

					if (program) {
						playProgramService(program, currentTime);
					}
				}, 150);
			}
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
			} else if (event.button === 2 && event.shiftKey) {
				// Shift+Right-click: Set downbeat position (grid offset)
				gridOffset = clickTime;
				console.log('🎵 Downbeat set at:', clickTime.toFixed(3) + 's');
				updateBeatGrid();
			} else if (event.button === 2) {
				// Right-click: Add marker (snapped to grid if BPM set)
				const snappedTime = snapToGrid(clickTime);
				console.log('🖱️ Right-click: Adding marker at', snappedTime, bpm ? '(snapped)' : '');
				addMarker(snappedTime);
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
				// Snap to grid if BPM is set
				const snappedTime = snapToGrid(region.start);

				// If snapped position differs, update the region
				if (snappedTime !== region.start && bpm) {
					region.setOptions({ start: snappedTime, end: snappedTime });
				}

				markers[markerIndex].time = snappedTime;

				// Regenerate label to ensure it's always centered
				const marker = markers[markerIndex];
				regenerateMarkerLabel(region.id, marker.label);

				markers = [...markers]; // Trigger reactivity
				syncMarkersToStore();
			}
		});

		regions.on('region-removed', (region) => {
			markers = markers.filter(m => m.id !== region.id);
			syncMarkersToStore();
		});

		// Track selected marker when clicked on waveform
		regions.on('region-clicked', (region, e) => {
			e.stopPropagation(); // Prevent waveform click from firing
			currentlySelectedMarker = region.id;
			console.log('Selected marker:', region.id);
		});

		// Clear selection when clicking empty waveform area
		wavesurfer.on('click', () => {
			currentlySelectedMarker = null;
			console.log('Cleared selection');
		});

		// Load audio with cached peaks if available
		if (cached) {
			wavesurfer.load(audioUrl, cached.peaks, cached.duration);
		} else {
			wavesurfer.load(audioUrl);
		}

		// Clear existing markers if not loading program
		if (!program) {
			markers = [];
		}
	}

	export function loadAudioFile(file) {
		console.log('Loading file:', file.name, file.type);

		// Check if it's an audio file
		if (file.type.startsWith('audio/') || file.name.endsWith('.wav') || file.name.endsWith('.mp3')) {
			fileName = file.name;

			// Store for upload
			const reader = new FileReader();
			reader.onload = (e) => {
				audioToUpload = e.target.result;
				console.log('[Program.svelte] Audio file stored for upload.');
			};
			reader.readAsDataURL(file);

			// Wait for DOM to update, then initialize WaveSurfer
			setTimeout(() => {
				const url = URL.createObjectURL(file);
				console.log('Loading URL:', url);
				initializeWaveSurfer(url);
			}, 100);
		} else {
			alert('Please select an audio file (WAV, MP3, etc.)');
		}
	}

	function loadCompressedAudio(audioDataURL) {
		console.log('[Program.svelte] Loading compressed audio, data URL length:', audioDataURL?.length);
		console.log('[Program.svelte] programId:', programId);

		// Check if container exists
		const container = document.querySelector(`#waveform-${sanitizedProgramId}`);
		if (!container) {
			console.error('[Program.svelte] Waveform container not found:', `#waveform-${sanitizedProgramId}`);
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

				// Create blob URL and initialize WaveSurfer
				const url = URL.createObjectURL(blob);
				console.log('[Program.svelte] Loading audio URL into WaveSurfer:', url);
				initializeWaveSurfer(url);
			})
			.catch(err => {
				console.error('[Program.svelte] Failed to load compressed audio:', err);
			});
	}

	// Helper function to create styled label elements - always centered
	function createRegionLabel(text, time, markerId) {
		const label = document.createElement('div');
		label.textContent = text;
		label.title = text; // Tooltip for full text

		// Always center labels vertically on the waveform
		label.style.cssText = `
			position: absolute !important;
			top: 50% !important;
			transform: translateY(-50%) !important;
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
			pointer-events: auto;
			cursor: pointer;
			display: block;
		`;

		// Make label clickable to select marker
		label.addEventListener('click', (e) => {
			e.stopPropagation(); // Prevent region click from also firing
			currentlySelectedMarker = markerId;
			console.log('Selected marker via label:', markerId);
		});

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
		const labelText = 'No Preset'; // Default label for new markers with preset action

		// Create region first to get ID
		const markerRegion = regions.addRegion({
			start: time,
			content: document.createElement('div'), // Temporary placeholder
			color: 'rgba(168, 85, 247, 0.3)',
			drag: true,
			resize: false
		});

		// Now create label with the region ID and replace content
		const labelElement = createRegionLabel(labelText, time, markerRegion.id);
		markerRegion.element.replaceChildren(labelElement);

		// Force style reapplication AFTER WaveSurfer's avoidOverlapping() runs (10ms)
		setTimeout(() => {
			if (labelElement.parentElement) {
				labelElement.style.marginTop = ''; // Remove plugin's marginTop
				labelElement.style.position = 'absolute';
				labelElement.style.top = '50%';
				labelElement.style.transform = 'translateY(-50%)';
			}
		}, 20);

		// Inherit default target board if set, otherwise empty
		const initialBoards = defaultTargetBoard ? [defaultTargetBoard] : [];

		const newMarker = {
			id: markerRegion.id,
			time: time,
			label: 'No Preset',
			presetName: undefined,
			boards: initialBoards,
			preset: 0,
			effect: 0,
			color: '#ff0000',
			brightness: 255,
			syncRate: 1
		};

		markers = [...markers, newMarker];
		syncMarkersToStore();
	}

	/**
	 * Generic function to update any marker property
	 * @param {string} markerId - Region ID
	 * @param {string} property - Property name (e.g., 'effect', 'color', 'brightness', etc.)
	 * @param {any} value - New value for the property
	 */
	function updateMarkerProperty(markerId, property, value) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker[property] = value;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	/**
	 * Regenerate a marker's label in the waveform
	 * @param {string} markerId - Region ID
	 * @param {string} newLabel - New label text
	 */
	function regenerateMarkerLabel(markerId, newLabel) {
		if (regions) {
			const allRegions = regions.getRegions();
			const region = allRegions.find(r => r.id === markerId);
			if (region) {
				const newLabelElement = createRegionLabel(newLabel, region.start, markerId);
				region.element.replaceChildren(newLabelElement);

				// Force style reapplication AFTER WaveSurfer's avoidOverlapping() runs (10ms)
				setTimeout(() => {
					if (newLabelElement.parentElement) {
						newLabelElement.style.marginTop = ''; // Remove plugin's marginTop
						newLabelElement.style.position = 'absolute';
						newLabelElement.style.top = '50%';
						newLabelElement.style.transform = 'translateY(-50%)';
					}
				}, 20);
			} else {
				console.warn('Region not found for marker:', markerId);
			}
		} else {
			console.warn('Regions plugin not available');
		}
	}

	function updateMarkerPreset(markerId, presetName) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.presetName = presetName;
			marker.preset = undefined;

			marker.label = presetName;
			regenerateMarkerLabel(markerId, presetName);

			markers = [...markers];
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
			syncMarkersToStore();
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

	function zoomIn() {
		if (!wavesurfer || !isLoaded) return;

		// Zoom levels: 0 (fit), 25, 38, 56, 84, 127, 190, 285, 428, 500 (max)
		if (zoomLevel === 0) {
			zoomLevel = 25;
		} else {
			zoomLevel = Math.min(Math.round(zoomLevel * 1.5), 500);
		}

		wavesurfer.zoom(zoomLevel);
		setTimeout(() => updateBeatGrid(), 10);
	}

	function zoomOut() {
		if (!wavesurfer || !isLoaded) return;

		if (zoomLevel <= 25) {
			zoomLevel = 0;
		} else {
			zoomLevel = Math.round(zoomLevel / 1.5);
		}

		wavesurfer.zoom(zoomLevel);
		setTimeout(() => updateBeatGrid(), 10);
	}

	function gridDenser() {
		const maxMultiplier = 128;
		if ($gridMultiplier < maxMultiplier) {
			gridMultiplier.set($gridMultiplier * 2);
			updateBeatGrid();
		}
	}

	function gridSparser() {
		const minMultiplier = 1;
		if ($gridMultiplier > minMultiplier) {
			gridMultiplier.set($gridMultiplier / 2);
			updateBeatGrid();
		}
	}

function playFullProgram() {
		// Mark this program as the last active (for spacebar control)
		lastActiveProgramId.set(programId);

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

		// Play from current position (or from start if at beginning)
		if (wavesurfer) {
			wavesurfer.setTime(currentTime);
			wavesurfer.play();
		}

		playProgramService(currentProgram, currentTime);
	}

	function stopFullProgram() {
		// Mark this program as the last active (for spacebar control)
		lastActiveProgramId.set(programId);

		const pausePosition = wavesurfer ? wavesurfer.getCurrentTime() : 0;
		console.log('⏸ PAUSE pressed - paused at position:', pausePosition);

		if (wavesurfer) {
			wavesurfer.pause();
		}

		// Clear any pending seek debounce timeout
		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}

		// Pause playback - clears timeouts but keeps lights as-is
		pausePlaybackService();
	}

	function stopAndReset() {
		lastActiveProgramId.set(programId);
		console.log('⏹ STOP pressed');

		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}

		stopPlaybackService();

		if (wavesurfer) {
			wavesurfer.stop();
		}
	}

	function saveProgram() {
		// Validation
		if (!songName.trim()) {
			alert('Please enter a song name');
			return;
		}

		const cuesWithoutPreset = markers.filter(m => !m.presetName);
		if (cuesWithoutPreset.length > 0) {
			const confirmed = confirm(
				`${cuesWithoutPreset.length} cue(s) have no preset selected and will be skipped during playback. Save anyway?`
			);
			if (!confirmed) return;
		}

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

		// Get existing program data to preserve audioId
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
			audioId: existingProgram?.audioId || program?.audioId || programId || newProgramId, // Use audioId reference
			cues: markers.map(m => ({
				time: m.time,
				label: m.label,
				boards: m.boards,
				presetName: m.presetName,
				color: m.color,
				effect: m.effect,
				brightness: m.brightness,
				syncRate: m.syncRate ?? 1
			})),
			createdAt: existingProgram?.createdAt || new Date().toISOString(),
			defaultTargetBoard: defaultTargetBoard,
			audioDuration: audioDuration,
			bpm: bpm ? Number(bpm) : undefined,
			gridOffset: gridOffset || 0,
			displayOrder: existingProgram?.displayOrder ?? program?.displayOrder ?? 0
		};

		// Create Program model using factory
		const programInstance = ProgramModel.fromJson(programData);

		if (programInstance) {
			// Save through service layer - store will update automatically
			saveProgramToStore(programInstance, audioToUpload);

			// Clear audio data after saving
			audioToUpload = null;

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

	async function downloadProgram() {
		if (!programId) {
			alert('Cannot download program without saving first');
			return;
		}

		// Get current program from store
		const programsArray = get(programsStore);
		const currentProgram = programsArray.find(p => p.id === programId);

		if (!currentProgram) {
			alert('Program not found');
			return;
		}

		if (!currentProgram.audioId) {
			alert('No audio file associated with this program');
			console.error('Program has no audioId:', currentProgram);
			return;
		}

		console.log('Downloading program with audioId:', currentProgram.audioId);

		try {
			// Fetch audio file from backend
			const audioUrl = `${API_URL}/audio/${currentProgram.audioId}`;
			console.log('Fetching audio from:', audioUrl);
			const audioResponse = await fetch(audioUrl);
			if (!audioResponse.ok) {
				throw new Error(`Failed to fetch audio file: ${audioResponse.status} ${audioResponse.statusText}`);
			}

			const audioBlob = await audioResponse.blob();

			// Convert to base64 data URL
			const reader = new FileReader();
			reader.readAsDataURL(audioBlob);

			reader.onloadend = () => {
				const base64data = reader.result;

				// Create export JSON with embedded audio, remove audio_file reference
				const exportData = {
					...currentProgram.toJson(),
					audio_data: base64data,
					audio_file: undefined
				};

				// Remove undefined fields from JSON
				const cleanExport = JSON.parse(JSON.stringify(exportData));

				// Trigger download
				const json = JSON.stringify(cleanExport, null, 2);
				const blob = new Blob([json], { type: 'application/json' });
				const url = URL.createObjectURL(blob);
				const a = document.createElement('a');
				a.href = url;
				a.download = `${songName.trim() || 'program'}.json`;
				document.body.appendChild(a);
				a.click();
				document.body.removeChild(a);
				URL.revokeObjectURL(url);
			};

			reader.onerror = () => {
				alert('Failed to encode audio file');
			};
		} catch (error) {
			console.error('Failed to download program:', error);
			alert('Failed to download program. Please try again.');
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

	// Reactive audio loading: initializes WaveSurfer when blob URL becomes available
	// Waits for global audio loading to complete before attempting on-demand load
	$effect(() => {
		if (!program?.audioId || wavesurferInitialized) return;

		const blobUrl = $audioBlobUrls[program.id];
		if (blobUrl) {
			wavesurferInitialized = true;
			initializeWaveSurfer(blobUrl);
		} else if (!$audioLoading) {
			// Only trigger on-demand load after global init completes
			loadAudioForProgram(program.id, program.audioId);
		}
	});

	// Cleanup on component destroy
	onDestroy(() => {
		// Clear seek debounce timeout to prevent memory leaks
		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}
	});
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
			<input
				type="number"
				bind:value={bpm}
				placeholder="BPM"
				class="bpm-input"
				min="20"
				max="300"
				oninput={updateBeatGrid}
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
				<button class="btn-download-program" onclick={downloadProgram} title="Download program with audio">
					<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
						<path d="M8 1v10M8 11l-3-3M8 11l3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
						<path d="M2 11v2c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2v-2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</button>
				<button class="btn-delete-program" onclick={deleteProgram} title="Delete program">
					<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
						<path d="M2 4h12M5.5 4V2.5h5V4M6.5 7.5v4M9.5 7.5v4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
						<path d="M3.5 4l.5 9.5c0 .55.45 1 1 1h6c.55 0 1-.45 1-1L13 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</button>
			{/if}
		</div>
		<div class="waveform-wrapper">
			{#if !isLoaded && (program?.audioId || program?.audioData)}
				<div class="waveform-skeleton"></div>
			{/if}
			<div id="waveform-{sanitizedProgramId}" class:hidden={!isLoaded && (program?.audioId || program?.audioData)}></div>
		</div>
		<div class="waveform-footer" class:has-cues={isLoaded}>
			{#if isLoaded}
				{@const groups = $boards.filter(b => b.isGroup)}
				{@const regularBoards = $boards.filter(b => !b.isGroup)}

				<div class="zoom-btn-group">
					<svg class="zoom-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
						<circle cx="7" cy="7" r="5.5" stroke="currentColor" stroke-width="1.5"/>
						<path d="M11 11L14.5 14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
					</svg>
					<button class="zoom-btn zoom-btn-left" onclick={zoomOut} title="Zoom Out">−</button>
					<button class="zoom-btn zoom-btn-right" onclick={zoomIn} title="Zoom In">+</button>
				</div>

				<div class="zoom-btn-group" title="Grid density">
					<svg class="zoom-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
						<path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
					</svg>
					<button class="zoom-btn zoom-btn-left" onclick={gridSparser} title="Sparser Grid">−</button>
					<button class="zoom-btn zoom-btn-right" onclick={gridDenser} title="Denser Grid">+</button>
				</div>

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

				{#if markers.length > 0}
					<button class="cue-count-badge-wrapper" onclick={clearCues}>
						<span class="cue-count-badge">{markers.length}</span>
						<span class="clear-cues-text">Clear Cues</span>
					</button>
				{:else}
					<div class="cue-count-badge-wrapper-static">
						<span class="cue-count-badge">0</span>
					</div>
				{/if}
			{/if}
		</div>
		{#if !isLoaded && !program?.audioId && !program?.audioData}
			<div class="audio-missing">
				<p>⚠️ Audio file missing</p>
				<p class="audio-missing-hint">This program was saved without audio. Please re-upload the file.</p>
			</div>
		{/if}

		{#if markers.length > 0 && currentlySelectedMarker}
			{@const marker = markers.find(m => m.id === currentlySelectedMarker)}
			{#if marker}
			<div class="markers-section">
				<div class="markers-list">
						<div class="marker-item">
							<div class="marker-info">
								<span class="marker-time">{formatTime(marker.time)}</span>
								<span class="marker-label">{marker.label}</span>
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

								<button
									class="preset-picker-button"
									class:broken-preset={marker.presetName && !$performancePresets.some(p => p.name === marker.presetName)}
									onclick={() => openPresetPicker(marker.id)}
								>
									{marker.presetName || 'Select Preset'}
									<span class="dropdown-arrow">▼</span>
								</button>
								<div class="sync-rate-group" title="BPM sync rate">
									<button
										class="sync-rate-btn"
										class:active={marker.syncRate === 0.25}
										onclick={() => updateMarkerProperty(marker.id, 'syncRate', 0.25)}
									>¼</button>
									<button
										class="sync-rate-btn"
										class:active={marker.syncRate === 0.5}
										onclick={() => updateMarkerProperty(marker.id, 'syncRate', 0.5)}
									>½</button>
									<button
										class="sync-rate-btn"
										class:active={(marker.syncRate ?? 1) === 1}
										onclick={() => updateMarkerProperty(marker.id, 'syncRate', 1)}
									>1</button>
									<button
										class="sync-rate-btn"
										class:active={marker.syncRate === 2}
										onclick={() => updateMarkerProperty(marker.id, 'syncRate', 2)}
									>2</button>
									<button
										class="sync-rate-btn"
										class:active={marker.syncRate === 4}
										onclick={() => updateMarkerProperty(marker.id, 'syncRate', 4)}
									>4</button>
								</div>

								<button class="btn-delete" onclick={() => deleteMarker(marker.id)}>
									✕
								</button>
							</div>
						</div>
				</div>
			</div>
			{/if}
		{/if}
	</div>
</div>

{#if presetPicker.open}
	<div class="preset-picker-overlay" onclick={closePresetPicker}>
		<div class="preset-picker-modal" onclick={(e) => e.stopPropagation()}>
			{#if presetPicker.step === 'category'}
				<div class="preset-picker-header">
					<h3>Select Effect Type</h3>
					<button class="preset-picker-close" onclick={closePresetPicker}>✕</button>
				</div>
				<div class="preset-picker-quick">
					{#each quickPresets as preset}
						<button
							class="preset-quick-btn"
							onclick={() => selectPreset(preset)}
						>
							{preset}
						</button>
					{/each}
				</div>
				<div class="preset-picker-grid">
					{#each allCategories() as category}
						<button
							class="preset-category-btn"
							onclick={() => selectCategory(category.name)}
						>
							{category.name}
						</button>
					{/each}
				</div>
			{:else}
				<div class="preset-picker-header">
					<button class="preset-picker-back" onclick={() => { presetPicker.step = 'category'; presetPicker.selectedCategory = null; }}>
						←
					</button>
					<h3>{presetPicker.selectedCategory}</h3>
					<button class="preset-picker-close" onclick={closePresetPicker}>✕</button>
				</div>
				<div class="preset-picker-grid colors">
					{#each categoryColors() as preset}
						<button
							class="preset-color-btn"
							style="color: {getColorStyle(preset.color)}"
							onclick={() => selectPreset(preset.name)}
						>
							{preset.color}
						</button>
					{/each}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	/* Force all WaveSurfer region labels to be centered - overrides plugin's default CSS */
	:global([data-id^="wavesurfer_"] > div > div) {
		position: absolute !important;
		top: 50% !important;
		transform: translateY(-50%) !important;
	}

	/* Remove WaveSurfer's default padding around waveform */
	:global([id^="waveform-"]) {
		padding: 0 !important;
	}

	:global([id^="waveform-"] > div) {
		padding: 0.5rem 1rem !important;
	}

	.program-editor {
		width: 100%;
		background: #0c0c0c;
		border-radius: 12px;
		border: 1px solid rgba(255, 255, 255, 0.03);
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
	}

	.waveform-container {
		background: transparent;
		border-radius: 0;
		border: none;
		overflow: visible;
		min-height: 252px;
	}

	.waveform-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		background: transparent;
	}

	.spacer {
		flex: 1;
	}

	.btn-program-play,
	.btn-program-pause,
	.btn-program-stop {
		padding: 0.5rem 1rem;
		border: 1px solid #1a1a1a;
		border-radius: 8px;
		font-size: 1rem;
		cursor: pointer;
		transition: all 0.2s;
		background-color: transparent;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		box-sizing: border-box;
	}

	.btn-program-play {
		color: #555;
	}

	.btn-program-play:hover {
		background-color: #111;
		color: #22c55e;
		border-color: #222;
	}

	.btn-program-pause {
		color: #555;
	}

	.btn-program-pause:hover {
		background-color: #111;
		color: #f59e0b;
		border-color: #222;
	}

	.btn-program-stop {
		color: #555;
	}

	.btn-program-stop:hover {
		background-color: #111;
		color: #ef4444;
		border-color: #222;
	}

	.song-name-input {
		flex: 0 0 250px;
		background-color: #0a0a0a;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.9rem;
		transition: border-color 0.2s;
	}

	.song-name-input:hover {
		border-color: #333;
	}

	.song-name-input:focus {
		outline: none;
		border-color: #222;
	}

	.song-name-input::placeholder {
		color: #444;
	}

	.track-input {
		width: 45px;
		background-color: #0a0a0a;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 0.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		text-align: center;
		transition: border-color 0.2s;
	}

	.track-input:hover {
		border-color: #333;
	}

	.track-input:focus {
		outline: none;
		border-color: #222;
	}

	.track-input::placeholder {
		color: #444;
	}

	.bpm-input {
		width: 60px;
		background-color: #0a0a0a;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 0.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		text-align: center;
		transition: border-color 0.2s;
		-moz-appearance: textfield;
	}

	.bpm-input::-webkit-outer-spin-button,
	.bpm-input::-webkit-inner-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}

	.bpm-input:hover {
		border-color: #333;
	}

	.bpm-input:focus {
		outline: none;
		border-color: #222;
	}

	.bpm-input::placeholder {
		color: #444;
	}

	.cue-count-badge-wrapper {
		background-color: transparent;
		color: #888;
		border: 1px solid #1a1a1a;
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
		background-color: #1a1212;
		border-color: #331a1a;
	}

	.cue-count-badge {
		color: #888;
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
		color: #c44;
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

	.cue-count-badge-wrapper-static {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
		border-radius: 16px;
		padding: 0;
		display: flex;
		align-items: center;
		height: 28px;
		min-width: 28px;
	}


	.file-name {
		font-size: 0.9rem;
		color: #555;
		font-weight: 400;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 200px;
		flex-shrink: 1;
	}


	.btn-save {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
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
		background-color: #111;
		color: #22c55e;
		border-color: #222;
	}

	.btn-save:active {
		background-color: #0f0f0f;
	}

	.btn-download-program {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
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

	.btn-download-program:hover {
		background-color: #111;
		color: #888;
		border-color: #222;
	}

	.btn-download-program:active {
		background-color: #0f0f0f;
	}

	.btn-delete-program {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
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
		background-color: #1a1212;
		color: #c44;
		border-color: #331a1a;
	}

	.btn-delete-program:active {
		background-color: #150f0f;
	}

	.waveform-wrapper {
		position: relative;
		min-height: 140px;
	}

	.waveform-wrapper:has(+ .waveform-footer:not(.has-cues)) {
		margin-bottom: -40px;
	}

	/* Custom scrollbar for waveform */
	.waveform-wrapper ::-webkit-scrollbar {
		height: 8px;
	}

	.waveform-wrapper ::-webkit-scrollbar-track {
		background: transparent;
	}

	.waveform-wrapper ::-webkit-scrollbar-thumb {
		background: rgba(168, 85, 247, 0.5);
		border-radius: 4px;
	}

	.waveform-wrapper ::-webkit-scrollbar-thumb:hover {
		background: rgba(168, 85, 247, 0.7);
	}

	/* Firefox scrollbar */
	.waveform-wrapper * {
		scrollbar-width: thin;
		scrollbar-color: rgba(168, 85, 247, 0.5) transparent;
	}

	div[id^="waveform-"] {
		padding: 0 2rem;
		min-height: 130px;
	}

	div[id^="waveform-"].hidden {
		opacity: 0;
		position: absolute;
		pointer-events: none;
	}

	.waveform-footer {
		padding: 0.5rem 1rem 0.75rem 1rem;
		background: transparent;
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

	.zoom-btn-group {
		display: flex;
		align-items: center;
		border: 1px solid #1a1a1a;
		border-radius: 4px;
		overflow: hidden;
		height: 28px;
		box-sizing: border-box;
		background: transparent;
	}

	.zoom-icon {
		color: #555;
		flex-shrink: 0;
		padding: 0 6px;
		display: flex;
		align-items: center;
	}

	.zoom-btn {
		width: 32px;
		height: 100%;
		background: transparent;
		border: none;
		color: #555;
		border-radius: 0;
		font-size: 1rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s ease;
		display: flex;
		align-items: center;
		justify-content: center;
		box-sizing: border-box;
		padding: 0;
		line-height: 1;
	}

	.zoom-btn-left {
		border-right: 1px solid #1a1a1a;
	}

	.zoom-btn:hover {
		background: #111;
		color: #888;
	}

	.zoom-btn:active {
		background: #0a0a0a;
	}

	.waveform-skeleton {
		position: absolute;
		top: 0;
		left: 2rem;
		right: 2rem;
		bottom: 0;
		background: linear-gradient(90deg,
			transparent 25%,
			rgba(255, 255, 255, 0.02) 50%,
			transparent 75%);
		background-size: 200% 100%;
		animation: shimmer 2.5s infinite ease-in-out;
		border-radius: 8px;
	}

	@keyframes shimmer {
		0% { background-position: 200% 0; }
		100% { background-position: -200% 0; }
	}

	.audio-missing {
		padding: 2rem;
		text-align: center;
		background-color: #0f0f0f;
	}

	.audio-missing p {
		color: #c44;
		font-size: 0.875rem;
		margin: 0.5rem 0;
	}

	.audio-missing-hint {
		color: #444 !important;
		font-size: 0.8rem !important;
	}

	.markers-section {
		padding: 0.5rem 1rem;
		overflow: visible;
	}

	.default-board-dropdown-wrapper {
		position: relative;
	}

	.default-board-select-button {
		background-color: transparent;
		border: 1px solid #1a1a1a;
		color: #888;
		padding: 0 2rem 0 0.75rem;
		border-radius: 4px;
		font-size: 0.875rem;
		cursor: pointer;
		width: 140px;
		height: 28px;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		position: relative;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		box-sizing: border-box;
	}

	.default-board-select-button:hover {
		border-color: #333;
		background: #111;
	}

	.default-board-select-button .dropdown-arrow {
		position: absolute;
		right: 0.75rem;
		font-size: 0.7rem;
		color: #555;
	}

	.default-board-dropdown-menu {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		background-color: #0f0f0f;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		min-width: 200px;
		max-height: 300px;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.default-board-dropdown-menu::-webkit-scrollbar {
		display: none;
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
		color: #444;
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
		background-color: #111;
	}

	.default-board-dropdown-menu .dropdown-option input[type="checkbox"] {
		cursor: pointer;
	}

	.default-board-dropdown-menu .dropdown-option span {
		color: #888;
		font-size: 0.875rem;
		flex: 1;
	}

	.btn-apply-default {
		padding: 0 1rem;
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
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
		text-align: center;
	}

	.btn-apply-default:hover:not(:disabled) {
		background-color: #111;
		color: #888;
		border-color: #222;
	}

	.btn-apply-default:active:not(:disabled) {
		background: #0f0f0f;
	}

	.btn-apply-default:disabled {
		background-color: transparent;
		color: #333;
		cursor: not-allowed;
	}

	.markers-list {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		overflow: visible;
	}

	.marker-item {
		padding: 0.35rem 0;
		display: flex;
		justify-content: space-between;
		align-items: center;
		overflow: visible;
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
		overflow: visible;
	}

	.boards-dropdown-wrapper {
		position: relative;
		overflow: visible;
	}

	.boards-select-button {
		background-color: transparent;
		border: 1px solid #1a1a1a;
		color: #888;
		padding: 0.5rem 2rem 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
		width: 140px;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: space-between;
		position: relative;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.boards-select-button:hover {
		border-color: #333;
		background: #111;
	}

	.dropdown-arrow {
		position: absolute;
		right: 0.75rem;
		font-size: 0.7rem;
		color: #555;
	}

	.boards-dropdown-menu {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		background-color: #0f0f0f;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		min-width: 200px;
		max-height: 300px;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.boards-dropdown-menu::-webkit-scrollbar {
		display: none;
	}

	.dropdown-section {
		padding: 0.5rem 0;
	}

	.dropdown-section:not(:last-child) {
		border-bottom: 1px solid #1a1a1a;
	}

	.dropdown-section-label {
		padding: 0.5rem 0.75rem;
		font-size: 0.75rem;
		font-weight: 600;
		color: #444;
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
		background-color: #111;
	}

	.dropdown-option input[type="checkbox"] {
		cursor: pointer;
	}

	.dropdown-option span {
		font-size: 0.875rem;
		color: #888;
	}

	.sync-rate-group {
		display: flex;
		background-color: transparent;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		overflow: hidden;
	}

	.sync-rate-btn {
		background: transparent;
		border: none;
		color: #444;
		padding: 0.5rem 0.6rem;
		font-size: 0.875rem;
		cursor: pointer;
		border-right: 1px solid #1a1a1a;
		transition: all 0.15s;
	}

	.sync-rate-btn:last-child {
		border-right: none;
	}

	.sync-rate-btn:hover {
		background-color: #111;
		color: #888;
	}

	.sync-rate-btn.active {
		background-color: #1a1a1a;
		color: #fff;
	}

	.marker-time {
		font-family: 'Courier New', monospace;
		font-size: 1.1rem;
		color: #888;
		font-weight: bold;
		min-width: 80px;
	}

	.marker-label {
		color: #888;
		font-size: 1rem;
		min-width: 120px;
	}

	.btn-delete {
		background-color: transparent;
		border: 1px solid #1a1a1a;
		color: #555;
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
		background-color: #1a1212;
		color: #c44;
		border-color: #331a1a;
	}

	.preset-picker-button {
		background-color: transparent;
		border: 1px solid #1a1a1a;
		color: #888;
		padding: 0.5rem 2rem 0.5rem 0.75rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
		min-width: 140px;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: space-between;
		position: relative;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.preset-picker-button:hover {
		border-color: #333;
		background: #111;
	}

	.preset-picker-button.broken-preset {
		border-color: #331a1a;
		background-color: #1a1212;
	}

	.preset-picker-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background-color: rgba(0, 0, 0, 0.8);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 10000;
	}

	.preset-picker-modal {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		min-width: 320px;
		max-width: 400px;
		max-height: 80vh;
		overflow: hidden;
	}

	.preset-picker-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem;
		background: transparent;
	}

	.preset-picker-header h3 {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		color: #fff;
		flex: 1;
		text-align: center;
	}

	.preset-picker-back {
		background: transparent;
		border: none;
		color: #888;
		font-size: 0.875rem;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		transition: all 0.2s;
	}

	.preset-picker-back:hover {
		background-color: #1a1a1a;
		color: #fff;
	}

	.preset-picker-close {
		background: transparent;
		border: none;
		color: #444;
		font-size: 1rem;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		transition: all 0.2s;
	}

	.preset-picker-close:hover {
		background-color: #1a1212;
		color: #c44;
	}

	.preset-picker-quick {
		display: flex;
		gap: 0.75rem;
		padding: 1rem 1rem 0 1rem;
	}

	.preset-quick-btn {
		flex: 1;
		background-color: transparent;
		border: 1px solid rgba(255, 255, 255, 0.03);
		color: #888;
		padding: 0.75rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		text-align: center;
	}

	.preset-quick-btn:hover {
		background-color: rgba(255, 255, 255, 0.02);
		border-color: rgba(255, 255, 255, 0.05);
		color: #fff;
	}

	.preset-picker-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.75rem;
		padding: 1rem;
		max-height: 60vh;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
	}

	.preset-picker-grid::-webkit-scrollbar {
		display: none;
	}

	.preset-picker-grid.colors {
		grid-template-columns: repeat(3, 1fr);
	}

	.preset-category-btn {
		background-color: transparent;
		border: 1px solid rgba(255, 255, 255, 0.03);
		color: #888;
		padding: 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		text-align: center;
	}

	.preset-category-btn:hover {
		background-color: rgba(255, 255, 255, 0.02);
		border-color: rgba(255, 255, 255, 0.05);
		color: #fff;
	}

	.preset-color-btn {
		background-color: transparent;
		border: 1px solid rgba(255, 255, 255, 0.03);
		padding: 0.75rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		text-align: center;
	}

	.preset-color-btn:hover {
		background-color: rgba(255, 255, 255, 0.02);
		border-color: rgba(255, 255, 255, 0.05);
	}
</style>
