<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import { page } from '$app/stores';
	import WaveSurfer from 'wavesurfer.js';
	import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js';
	import { API_URL } from '$lib/api';
	import { saveProgram as saveProgramToStore, deleteProgram as deleteProgramFromStore } from '$lib/programs-db';
	import { playProgram as playProgramService, stopPlayback as stopPlaybackService, pausePlayback as pausePlaybackService } from '$lib/playback-db';
	import { loadAudioForProgram, getCachedPeaks, loadGuideAudioForProgram, removeGuideAudioForProgram } from '$lib/audio-db';
	import { audioBlobUrls, audioLoading, loopyProSettings, guideBlobUrls, guideCachedPeaks } from '$lib/store';
	import { Program as ProgramModel } from '$lib/models/Program';
	import { programs as programsStore, boards, performancePresets, patternPresets, currentlyPlayingProgram, lastActiveProgramId, gridMultiplier } from '$lib/store';
	import { WLED_EFFECTS } from '$lib/wled-effects';
	import PresetPicker from '$lib/PresetPicker.svelte';
	import CueEditor from '$lib/CueEditor.svelte';
	import RoutingModal from '$lib/RoutingModal.svelte';
	import SlotTrack from '$lib/SlotTrack.svelte';
	import { getSlot } from '$lib/slots';
	import { onResamplingProgress, type ResamplingProgress } from '$lib/sse';

	interface Marker {
		id: string;
		time: number;
		label?: string;
		boards?: string[];
		presetName?: string;
		preset?: number;
		effect?: number;
		color?: [number, number, number];
		brightness?: number;
		syncRate?: number;
	}

	const backingSlot = getSlot('backing')!;
	const guideSlot = getSlot('guide')!;

	// Props
	let {
		program = null
	} = $props();

	// Internal state derived from prop
	let programId = $state(program?.id || null);
	// Sanitized version of programId for use in HTML IDs and CSS selectors (no spaces or special chars)
	let sanitizedProgramId = $derived(programId ? programId.replace(/[^a-zA-Z0-9-_]/g, '-') : null);

	let wavesurfer: WaveSurfer | null = $state(null);
	let regions: ReturnType<typeof RegionsPlugin.create> | null = $state(null);
	let markers: Marker[] = $state([]);
	let fileName = $state('');
	let isLoaded = $state(false);
	let isPlaying = $state(false);
	let audioToUpload = $state(null);
	let wavesurferInitialized = $state(false);

	// Resampling state
	let isTrackReady = $state(true);
	let resamplingProgress: ResamplingProgress | null = $state(null);
	let guideResamplingProgress: ResamplingProgress | null = $state(null);

	// Guide track state - uses SlotTrack component
	// Show guide section when: guideAudioId exists, OR resampling in progress (during upload)
	let hasGuide = $derived(!!program?.guideAudioId || !!guideResamplingProgress);
	let guideBlobUrl = $derived(program?.id ? $guideBlobUrls[program.id] : null);
	let guidePeaks = $derived(program?.id ? $guideCachedPeaks[program.id] : null);
	let unsubscribeResampling: (() => void) | null = null;

	// Program metadata
	let songName = $state('');
	let loopyProTrack = $state('');
	let audioDuration: number | null = $state(null); // Duration in seconds (extracted from audio)
	let bpm: number | null = $state(null); // BPM for speed-synced effects
	let gridOffset = $state(0); // Downbeat position - where beat 1 of bar 1 starts

	// Preset picker modal state
	let presetPickerOpen = $state(false);
	let presetPickerMarkerId: string | null = $state(null);

	// Routing modal state
	let routingModalOpen = $state(false);

	function openPresetPicker(markerId: string) {
		presetPickerMarkerId = markerId;
		presetPickerOpen = true;
	}

	function handlePresetSelect(presetName: string) {
		if (presetPickerMarkerId) {
			updateMarkerPreset(presetPickerMarkerId, presetName);
		}
		presetPickerOpen = false;
		presetPickerMarkerId = null;
	}

	function closePresetPicker() {
		presetPickerOpen = false;
		presetPickerMarkerId = null;
	}

	function openRoutingModal() {
		routingModalOpen = true;
	}

	// Snap time to nearest grid line based on BPM and offset (only if within 10px)
	function snapToGrid(time: number): number {
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
	function getScrollContainer(): HTMLElement | null {
		const wrapper = wavesurfer?.getWrapper();
		if (!wrapper) return null;
		const findScrollable = (el: Element): HTMLElement | null => {
			const style = window.getComputedStyle(el);
			if (style.overflowX === 'auto' || style.overflowX === 'scroll') return el as HTMLElement;
			for (const child of el.children) {
				const found = findScrollable(child);
				if (found) return found;
			}
			return null;
		};
		return findScrollable(wrapper) || wrapper.querySelector('div');
	}

	// Store grid region IDs so we can remove them on update
	let gridRegionIds: string[] = [];

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


	// Currently selected marker
	let currentlySelectedMarker: string | null = $state(null);

	// Default target board for new cues
	let defaultTargetBoard: string | null = $state(null);
	let defaultBoardDropdownOpen = $state(false);
	let actionMenuOpen = $state(false);

	// Zoom state (0 = fit to container, >0 = pixels per second)
	let zoomLevel = $state(0);

	// Seeking state for debouncing
	let seekDebounceTimeout: ReturnType<typeof setTimeout> | null = null;
	let lastSeekTime = 0;

	// Pending cues to restore after audio loads (component-scoped, not global)
	let pendingCues: Marker[] = [];

	function stripAudioExtension(id: string): string {
		return id.replace(/\.(mp3|wav)$/i, '');
	}

	async function checkTrackReadiness() {
		if (!program?.audioId) {
			isTrackReady = true;
			return;
		}

		try {
			const response = await fetch(`${API_URL}/audio/engine/readiness`);
			if (response.ok) {
				const data = await response.json();
				const audioIdBase = stripAudioExtension(program.audioId);
				const trackInfo = data.tracks.find((t: { id: string }) =>
					t.id === program.audioId || t.id === audioIdBase
				);
				isTrackReady = trackInfo?.ready ?? true;
			}
		} catch (err) {
			isTrackReady = true;
		}
	}

	onMount(async () => {
		unsubscribeResampling = onResamplingProgress((progress) => {
			if (program?.audioId) {
				const audioIdBase = stripAudioExtension(program.audioId);
				if (progress.trackName === audioIdBase || progress.trackName === program.audioId) {
					if (progress.active) {
						isTrackReady = false;
						resamplingProgress = progress;
					} else {
						isTrackReady = true;
						resamplingProgress = null;
					}
				}
			}

			const expectedGuideId = programId ? `${programId}_guide` : null;
			const guideIdBase = program?.guideAudioId ? stripAudioExtension(program.guideAudioId) : null;
			const trackNameWithoutPrefix = progress.trackName.replace(/^guide:/, '');
			if (trackNameWithoutPrefix === guideIdBase ||
				trackNameWithoutPrefix === program?.guideAudioId ||
				trackNameWithoutPrefix === expectedGuideId ||
				progress.trackName === `guide:${expectedGuideId}`) {
				if (progress.active) {
					guideResamplingProgress = progress;
				} else {
					guideResamplingProgress = null;
				}
			}
		});

		if (program?.audioId) {
			checkTrackReadiness();
		}

		if (program?.id) {
			programId = program.id;
		}

		if (program) {
			console.log(`[Program.svelte] onMount - program for ${program.id}:`, {
				hasAudioId: !!program.audioId,
				hasAudioData: !!program.audioData,
				audioId: program.audioId
			});

			loadProgramData(program);

			if (!program.audioId && program.audioData) {
				console.log(`[Program.svelte] Loading legacy embedded audio`);
				setTimeout(() => {
					loadCompressedAudio(program.audioData);
				}, 50);
			}
		}

		// Keyboard handler for play/pause (Space) and add cue (C)
		function handleKeyPress(event: KeyboardEvent) {
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
			document.removeEventListener('keydown', handleKeyPress);
		};
	});

	function loadProgramData(data: { songName?: string; loopyProTrack?: string; fileName?: string; defaultTargetBoard?: string | null; bpm?: number | null; gridOffset?: number; cues?: Marker[] }) {
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

	function initializeWaveSurfer(audioUrl: string) {
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

	async function handleGuideUpload(file) {
		if (!programId) return;
		if (program?.guideAudioId) {
			console.warn('[handleGuideUpload] Guide already exists, ignoring upload');
			return;
		}

		const blobUrl = URL.createObjectURL(file);

		const reader = new FileReader();
		reader.onload = async (e) => {
			const dataUrl = e.target.result;
			const guideId = `${programId}_guide`;

			try {
				const response = await fetch(`${API_URL}/audio/${guideId}`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ data_url: dataUrl })
				});

				if (response.ok) {
					const result = await response.json();
					program.guideAudioId = result.audio_file;
					guideBlobUrls.update(urls => ({ ...urls, [programId]: blobUrl }));
					await saveProgram();
				} else {
					console.error('Failed to upload guide track:', response.status, response.statusText);
					URL.revokeObjectURL(blobUrl);
				}
			} catch (err) {
				console.error('Failed to upload guide track:', err);
				URL.revokeObjectURL(blobUrl);
			}
		};
		reader.onerror = () => {
			console.error('Failed to read guide audio file:', reader.error);
			URL.revokeObjectURL(blobUrl);
		};
		reader.readAsDataURL(file);
	}

	async function removeGuide() {
		if (!program?.guideAudioId) return;

		try {
			const response = await fetch(`${API_URL}/audio/${program.guideAudioId}`, {
				method: 'DELETE'
			});

			if (response.ok) {
				removeGuideAudioForProgram(program.id);
				program.guideAudioId = undefined;
				await saveProgram();
			}
		} catch (err) {
			console.error('Failed to remove guide track:', err);
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

	function deleteMarker(markerId) {
		const allRegions = regions.getRegions();
		const region = allRegions.find(r => r.id === markerId);
		if (region) {
			region.remove();
		}
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
		lastActiveProgramId.set(programId);

		let currentProgram = null;
		const unsubscribe = programsStore.subscribe(programs => {
			currentProgram = programs.find(p => p.id === programId);
		});
		unsubscribe();

		if (!currentProgram) return;

		const currentTime = wavesurfer ? wavesurfer.getCurrentTime() : 0;
		console.log('▶️ PLAY pressed - starting from position:', currentTime);

		const audioSource = get(loopyProSettings).audio_source;
		if (wavesurfer) {
			wavesurfer.setTime(currentTime);
			if (audioSource === 'audio_engine') {
				wavesurfer.setVolume(0);
			} else {
				wavesurfer.setVolume(1);
			}
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
			audioId: existingProgram?.audioId || program?.audioId || programId || newProgramId,
			guideAudioId: program?.guideAudioId || existingProgram?.guideAudioId,
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

	// Trigger guide audio loading when needed - SlotTrack handles wavesurfer init
	$effect(() => {
		if (!program?.guideAudioId || !isLoaded) return;

		const blobUrl = $guideBlobUrls[program.id];
		if (!blobUrl && !$audioLoading) {
			loadGuideAudioForProgram(program.id, program.guideAudioId);
		}
	});

	onDestroy(() => {
		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}
		if (unsubscribeResampling) {
			unsubscribeResampling();
			unsubscribeResampling = null;
		}
	});
</script>

<div class="program-editor" onclick={() => { actionMenuOpen = false; defaultBoardDropdownOpen = false; }}>
	<div class="waveform-container">
		<div class="waveform-header">
			{#if isPlaying}
				<button class="btn-program-pause" onclick={stopFullProgram}>
					⏸
				</button>
			{:else}
				<button class="btn-program-play" onclick={playFullProgram} disabled={!isTrackReady}>
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
			<div class="action-menu-wrapper">
				<button
					class="btn-action-menu"
					onclick={(e) => {
						e.stopPropagation();
						actionMenuOpen = !actionMenuOpen;
					}}
					title="Actions"
				>
					⋮
				</button>
				{#if actionMenuOpen}
					<div class="action-menu-dropdown">
						<button class="action-menu-item" onclick={() => { saveProgram(); actionMenuOpen = false; }}>Save</button>
						{#if programId}
							<button class="action-menu-item" onclick={() => { downloadProgram(); actionMenuOpen = false; }}>Download</button>
							<button class="action-menu-item action-menu-item-danger" onclick={() => { deleteProgram(); actionMenuOpen = false; }}>Delete</button>
						{/if}
					</div>
				{/if}
			</div>
		</div>
		<div class="slot-label" style="--slot-color: {backingSlot.color}">
			<span>{backingSlot.label}</span>
			{#if resamplingProgress}
				<span class="resampling-inline">
					{resamplingProgress.trackName} • {(resamplingProgress.fromRate / 1000).toFixed(1)}kHz → {(resamplingProgress.toRate / 1000).toFixed(1)}kHz • {Math.round((resamplingProgress.current / resamplingProgress.total) * 100)}%
				</span>
			{/if}
			<button class="routing-btn" onclick={openRoutingModal} title="Channel routing">
				<svg width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
					<path d="M2 4h4M10 4h4M2 8h4M10 8h4M2 12h4M10 12h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
					<circle cx="8" cy="4" r="1.5" stroke="currentColor" stroke-width="1.5"/>
					<circle cx="8" cy="8" r="1.5" stroke="currentColor" stroke-width="1.5"/>
					<circle cx="8" cy="12" r="1.5" stroke="currentColor" stroke-width="1.5"/>
				</svg>
			</button>
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

		{#if isLoaded}
			<div class="guide-section">
				{#if hasGuide}
					<SlotTrack
						slot={guideSlot}
						programId={programId}
						blobUrl={guideBlobUrl}
						cachedPeaks={guidePeaks}
						resamplingProgress={guideResamplingProgress}
						mainWavesurfer={wavesurfer}
						onRemove={removeGuide}
						onRoutingClick={openRoutingModal}
					/>
				{:else}
					<div
						class="guide-dropzone"
						onclick={() => document.getElementById(`guide-input-${sanitizedProgramId}`)?.click()}
						ondrop={(e) => {
							e.preventDefault();
							const file = e.dataTransfer?.files[0];
							if (file && (file.type.startsWith('audio/') || file.name.endsWith('.wav') || file.name.endsWith('.mp3'))) {
								handleGuideUpload(file);
							}
						}}
						ondragover={(e) => e.preventDefault()}
					>
						<span>Drop or click to add guide track</span>
						<input
							id="guide-input-{sanitizedProgramId}"
							type="file"
							accept="audio/*"
							style="display: none"
							onchange={(e) => {
								const file = e.target?.files?.[0];
								if (file) handleGuideUpload(file);
							}}
						/>
					</div>
				{/if}
			</div>
		{/if}

		{#if !isLoaded && !program?.audioId && !program?.audioData}
			<div class="audio-missing">
				<p>⚠️ Audio file missing</p>
				<p class="audio-missing-hint">This program was saved without audio. Please re-upload the file.</p>
			</div>
		{/if}

		{#if markers.length > 0 && currentlySelectedMarker}
			{@const marker = markers.find(m => m.id === currentlySelectedMarker)}
			{#if marker}
				<CueEditor
					{marker}
					onToggleBoardSelection={toggleBoardSelection}
					onOpenPresetPicker={openPresetPicker}
					onUpdateSyncRate={(markerId, rate) => updateMarkerProperty(markerId, 'syncRate', rate)}
					onDelete={deleteMarker}
				/>
			{/if}
		{/if}
	</div>
</div>

<PresetPicker
	open={presetPickerOpen}
	onSelect={handlePresetSelect}
	onClose={closePresetPicker}
/>

<RoutingModal
	open={routingModalOpen}
	onClose={() => routingModalOpen = false}
/>

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

	.btn-program-play:hover:not(:disabled) {
		background-color: #111;
		color: #22c55e;
		border-color: #222;
	}

	.btn-program-play:disabled {
		color: #333;
		cursor: not-allowed;
		opacity: 0.5;
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


	.action-menu-wrapper {
		position: relative;
	}

	.btn-action-menu {
		background-color: transparent;
		color: #555;
		border: 1px solid #1a1a1a;
		padding: 0.5rem;
		border-radius: 8px;
		font-size: 1.1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		box-sizing: border-box;
	}

	.btn-action-menu:hover {
		background-color: #111;
		color: #888;
		border-color: #222;
	}

	.action-menu-dropdown {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 8px;
		min-width: 140px;
		z-index: 100;
		padding: 0.25rem;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.action-menu-item {
		width: 100%;
		padding: 0.5rem 0.75rem;
		background: transparent;
		border: none;
		color: #888;
		font-size: 0.8rem;
		cursor: pointer;
		border-radius: 6px;
		transition: all 0.15s;
		text-align: left;
	}

	.action-menu-item:hover {
		background: rgba(255, 255, 255, 0.05);
		color: #ccc;
	}

	.action-menu-item-danger:hover {
		background: rgba(239, 68, 68, 0.1);
		color: #ef4444;
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

	.guide-section {
		margin-top: 0.5rem;
		padding-top: 0.5rem;
	}

	.slot-label {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.25rem 1rem;
		font-size: 0.75rem;
		color: var(--slot-color, #888);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.resampling-inline {
		color: #666;
		font-size: 0.7rem;
		text-transform: none;
		letter-spacing: normal;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.routing-btn {
		margin-left: auto;
		background: transparent;
		border: none;
		color: color-mix(in srgb, var(--slot-color, #888) 50%, transparent);
		cursor: pointer;
		padding: 0.25rem;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 4px;
		transition: all 0.15s;
	}

	.routing-btn:hover {
		color: var(--slot-color, #888);
		background: color-mix(in srgb, var(--slot-color, #888) 10%, transparent);
	}

	.guide-dropzone {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 50px;
		margin: 0.5rem 1rem;
		border: 1px dashed rgba(34, 197, 94, 0.3);
		border-radius: 4px;
		color: rgba(34, 197, 94, 0.6);
		font-size: 0.8rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	.guide-dropzone:hover {
		border-color: rgba(34, 197, 94, 0.6);
		background: rgba(34, 197, 94, 0.05);
	}

	div[id^="waveform-"] {
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

</style>
