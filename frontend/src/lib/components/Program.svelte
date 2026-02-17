<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import { page } from '$app/stores';
	import WaveSurfer from 'wavesurfer.js';
	import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js';
	import { API_URL } from '$lib/api';
	import { saveProgram as saveProgramToStore, deleteProgram as deleteProgramFromStore, duplicateProgram as duplicateProgramService } from '$lib/db/programs-db';
	import { playProgram as playProgramService, stopPlayback as stopPlaybackService, pausePlayback as pausePlaybackService, resumePlayback as resumePlaybackService } from '$lib/db/playback-db';
	import { loadAudioForProgram, getCachedPeaks, loadGuideAudioForProgram, removeGuideAudioForProgram, setSlotVolume, clearAudioCacheForProgram, computePeaksFromBuffer } from '$lib/db/audio-db';
	import { audioLoading, loopyProSettings, guideBlobUrls, guideCachedPeaks, cachedPeaks as cachedPeaksStore } from '$lib/stores/store';
	import { Program as ProgramModel } from '$lib/models/Program';
	import { programs as programsStore, boards, performancePresets, patternPresets, currentlyPlayingProgram, lastActiveProgramId, gridMultiplier, playbackPosition } from '$lib/stores/store';
	import { WLED_EFFECTS } from '$lib/wled-effects';
	import PresetPicker from '$lib/components/PresetPicker.svelte';
	import CueEditor from '$lib/components/CueEditor.svelte';
	import Track from '$lib/components/Track.svelte';
	import { getSlot } from '$lib/slots';
	import { toggleSlotMute } from '$lib/db/audio-db';
	import { slotMuted, resamplingProgress as resamplingProgressStore, resamplingComplete as resamplingCompleteStore, type SlotResamplingProgress } from '$lib/stores/store';
	import type { MarkerType } from '$lib/models/Cue';

	interface Marker {
		id: string;
		time: number;
		type: MarkerType;
		label: string;
		boards: string[];
		presetName?: string;
		preset?: number;
		effect?: number;
		color?: string;
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
	let isPaused = $state(false);
	let audioToUpload: string | ArrayBuffer | null = $state(null);
	let wavesurferInitialized = $state(false);

	let backingProgress = $derived.by(() => {
		const p = $resamplingProgressStore.backing;
		if (!p || p.programId !== programId) return null;
		if (p.total > 0 && p.current >= p.total) return null;
		return p;
	});
	let guideProgress = $derived.by(() => {
		const p = $resamplingProgressStore.guide;
		if (!p || p.programId !== programId) return null;
		if (p.total > 0 && p.current >= p.total) return null;
		return p;
	});
	let resamplingModalOpen = $state(false);
	let resamplingModalMessage = $state('');

	// Guide track state - uses Track component
	// Show guide section when: guideAudioId exists, OR resampling in progress (during upload)
	let hasGuide = $derived(!!program?.guideAudioId || !!guideProgress);
	let guideBlobUrl = $derived(program?.id ? $guideBlobUrls[program.id] : null);
	let guidePeaks = $derived(program?.id ? $guideCachedPeaks[program.id] : null);

	// Program metadata
	let songName = $state('');
	let loopyProTrack = $state('');
	let audioDuration: number | null = $state(null); // Duration in seconds (extracted from audio)
	let bpm: number | null = $state(null); // BPM for speed-synced effects
	let gridOffset = $state(0); // Downbeat position - where beat 1 of bar 1 starts
	let clickRate = $state(1); // Click track rate (0.5 = half, 1 = normal, 2 = double)
	let guideVolume = $state(1.0);

	// Preset picker modal state
	let presetPickerOpen = $state(false);
	let presetPickerMarkerId: string | null = $state(null);

	// Metadata modal state
	let metadataModalOpen = $state(false);

	// Resampled versions dialog state
	let resampledDialogOpen = $state(false);
	let resampledInfoRaw: { backing: any; guide: any } | null = $state(null);
	let resampledLoading = $state(false);

	let resampledInfo = $derived.by(() => {
		if (!resampledInfoRaw) return null;
		const complete = $resamplingCompleteStore;
		if (!complete || complete.programId !== programId) return resampledInfoRaw;

		const trackKey = complete.slot === 'guide' ? 'guide' : 'backing';
		const trackInfo = resampledInfoRaw[trackKey];
		if (!trackInfo) return resampledInfoRaw;

		const idx = trackInfo.versions.findIndex((v: any) => v.target_rate === complete.targetRate);
		if (idx < 0) return resampledInfoRaw;

		const updatedVersions = [...trackInfo.versions];
		updatedVersions[idx] = { ...updatedVersions[idx], quality: complete.quality };
		return { ...resampledInfoRaw, [trackKey]: { ...trackInfo, versions: updatedVersions } };
	});

	async function fetchResampledInfo(showLoading = true) {
		if (!programId) return;
		if (showLoading) resampledLoading = true;
		try {
			const trackId = programId;
			const res = await fetch(`${API_URL}/audio/resampled-info/${encodeURIComponent(trackId)}`);
			if (res.ok) {
				resampledInfoRaw = await res.json();
			}
		} catch (e) {
			console.error('Failed to fetch resampled info:', e);
		}
		if (showLoading) resampledLoading = false;
	}

	async function triggerResample(trackId: string, targetRate: number, quality: string) {
		try {
			await fetch(`${API_URL}/audio/resample`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ track_id: trackId, target_rate: targetRate, quality })
			});
		} catch (e) {
			console.error('Failed to trigger resample:', e);
		}
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	// Edit mode (lighting vs midi markers)
	let editMode: MarkerType = $state('lighting');
	let visibleMarkers = $derived(markers.filter(m => m.type === editMode));
	let shiftAmount: number = $state(1);

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

			const region = regions!.addRegion({
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
	// Track the current playhead time ourselves (wavesurfer.getCurrentTime unreliable with peaks-only mode)
	let playheadTimeSecs = $state(0);

	// SSE-driven playhead (backend broadcasts position at 10Hz)
	$effect(() => {
		const pos = $playbackPosition;
		if (!wavesurfer || !isLoaded) return;
		if (pos && pos.programId === programId) {
			playheadTimeSecs = pos.positionSecs;
			const duration = wavesurfer.getDuration();
			if (duration > 0) {
				wavesurfer.seekTo(Math.min(pos.positionSecs / duration, 1));
			}
			if (!isPaused) {
				isPlaying = true;
			}
		} else if (!pos && (isPlaying || isPaused)) {
			isPlaying = false;
			isPaused = false;
		}
	});

	function stopPlayhead() {
		isPlaying = false;
	}

	function getPlayheadTime(): number {
		return playheadTimeSecs;
	}

	// Auto-save debounce
	let saveTimeout: ReturnType<typeof setTimeout> | null = null;

	function debouncedSave() {
		if (saveTimeout) clearTimeout(saveTimeout);
		saveTimeout = setTimeout(() => {
			if (isLoaded && program) {
				saveProgram();
			}
		}, 500);
	}

	// Pending cues to restore after audio loads (component-scoped, not global)
	let pendingCues: Marker[] = [];

	function stripAudioExtension(id: string): string {
		return id.replace(/\.(mp3|wav)$/i, '');
	}

	async function checkTrackReadiness(): Promise<{ ready: boolean; message: string }> {
		if (!program?.audioId) {
			return { ready: true, message: '' };
		}

		try {
			const response = await fetch(`${API_URL}/audio/engine/readiness`);
			if (response.ok) {
				const data = await response.json();
				const audioIdBase = stripAudioExtension(program.audioId);
				const trackInfo = data.tracks.find((t: { id: string }) =>
					t.id === program.audioId || t.id === audioIdBase
				);
				console.log('[checkTrackReadiness]', { audioId: program.audioId, audioIdBase, trackIds: data.tracks.map((t: { id: string }) => t.id), match: trackInfo });
				if (!trackInfo) {
					return { ready: true, message: '' };
				}
				if (trackInfo.ready) {
					return { ready: true, message: '' };
				}
				const fromRate = trackInfo.original_rate || 0;
				const toRate = data.device_sample_rate || 0;
				return {
					ready: false,
					message: `Resampling audio from ${(fromRate/1000).toFixed(1)}kHz to ${(toRate/1000).toFixed(1)}kHz...`
				};
			}
		} catch (err) {
			console.error('Failed to check track readiness:', err);
		}
		return { ready: true, message: '' };
	}

	onMount(() => {
		if (program?.id) {
			programId = program.id;
		}

		if (program) {
			loadProgramData(program);

			if (!program.audioId && program.audioData) {
				setTimeout(() => {
					loadCompressedAudio(program.audioData);
				}, 50);
			}
		}

		function handleKeyPress(event: KeyboardEvent) {
			const currentPath = get(page).url.pathname;
			if (currentPath !== '/programming') return;

			const lastActiveId = get(lastActiveProgramId);
			if (lastActiveId !== programId) return;

			if (!wavesurfer || !isLoaded) return;

			if (event.code === 'Space') {
				event.preventDefault();
				if (isPlaying) {
					stopFullProgram();
				} else {
					playFullProgram();
				}
			} else if (event.code === 'KeyC') {
				event.preventDefault();
				const currentTime = getPlayheadTime();
				addMarker(currentTime);
				const newMarker = markers[markers.length - 1];
				if (newMarker) {
					currentlySelectedMarker = newMarker.id;
				}
			}
		}

		document.addEventListener('keydown', handleKeyPress);

		return () => {
			if (wavesurfer) {
				wavesurfer.destroy();
			}
			document.removeEventListener('keydown', handleKeyPress);
		};
	});

	function loadProgramData(data: { songName?: string; loopyProTrack?: string; fileName?: string; defaultTargetBoard?: string | null; bpm?: number | null; gridOffset?: number; clickRate?: number; guideVolume?: number; cues?: Marker[] }) {
		songName = data.songName || '';
		loopyProTrack = data.loopyProTrack || '';
		fileName = data.fileName || '';
		defaultTargetBoard = data.defaultTargetBoard || null;
		bpm = data.bpm || null;
		gridOffset = data.gridOffset || 0;
		clickRate = data.clickRate ?? 1;
		guideVolume = data.guideVolume ?? 1.0;
		// Note: cues will need to be restored after audio file is loaded
		// Store them temporarily in component-scoped variable
		pendingCues = data.cues || [];
	}

	function initializeWaveSurfer(audioUrl?: string) {
		regions = RegionsPlugin.create();

		const cached = programId ? getCachedPeaks(programId) : null;
		const peaksOnly = cached && !audioUrl;

		const createOptions: any = {
			container: `#waveform-${sanitizedProgramId}`,
			waveColor: 'rgba(139, 92, 246, 0.5)',
			progressColor: 'rgba(139, 92, 246, 0.8)',
			cursorColor: 'rgba(167, 139, 250, 0.9)',
			barWidth: 2,
			barRadius: 3,
			height: 120,
			plugins: [regions]
		};

		if (peaksOnly) {
			const media = new Audio();
			media.preload = 'none';
			createOptions.media = media;
			createOptions.peaks = cached.peaks;
			createOptions.duration = cached.duration;
		}

		wavesurfer = WaveSurfer.create(createOptions);

		wavesurfer.on('ready', () => {
			isLoaded = true;

			const duration = wavesurfer!.getDuration();
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

					const markerRegion = regions!.addRegion({
						start: cue.time,
						content: document.createElement('div'),
						color: 'rgba(168, 85, 247, 0.3)',
						drag: true,
						resize: false
					});

					const labelElement = createRegionLabel(cue.label || '', cue.time, markerRegion.id);
					markerRegion.element!.replaceChildren(labelElement);

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
						type: cue.type || 'lighting',
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

		// Handle seeking during playback - reschedule cues from new position (debounced)
		wavesurfer.on('seeking', (currentTime) => {
			let currentProgram: ProgramModel | null = null;
			const unsub = currentlyPlayingProgram.subscribe(p => {
				currentProgram = p;
			});
			unsub();

			if (currentProgram && (currentProgram as ProgramModel).id === programId && isPlaying) {
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

					let seekProgram: ProgramModel | undefined;
					const unsubPrograms = programsStore.subscribe(programs => {
						seekProgram = programs.find(p => p.id === programId);
					});
					unsubPrograms();

					if (seekProgram) {
						playProgramService(seekProgram, currentTime);
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
			const duration = wavesurfer!.getDuration();
			const clickTime = relativeX * duration;

			if (event.button === 0) {
				playheadTimeSecs = clickTime;
				wavesurfer!.seekTo(relativeX);
			} else if (event.button === 2 && event.shiftKey) {
				gridOffset = clickTime;
				console.log('🎵 Downbeat set at:', clickTime.toFixed(3) + 's');
				updateBeatGrid();
				debouncedSave();
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

		if (audioUrl) {
			wavesurfer.load(audioUrl, cached?.peaks, cached?.duration);
		}

		// Clear existing markers if not loading program
		if (!program) {
			markers = [];
		}
	}

	async function handleGuideUpload(file: File) {
		if (!programId) return;
		if (program?.guideAudioId) {
			console.warn('[handleGuideUpload] Guide already exists, ignoring upload');
			return;
		}

		const blobUrl = URL.createObjectURL(file);

		const reader = new FileReader();
		reader.onload = async (e) => {
			const dataUrl = e.target!.result;
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

	export function loadAudioFile(file: File) {
		console.log('Loading file:', file.name, file.type);

		// Check if it's an audio file
		if (file.type.startsWith('audio/') || file.name.endsWith('.wav') || file.name.endsWith('.mp3')) {
			fileName = file.name;

			// Store for upload
			const reader = new FileReader();
			reader.onload = (e) => {
				audioToUpload = e.target!.result;
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

	function loadCompressedAudio(audioDataURL: string) {
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
	function createRegionLabel(text: string, time: number, markerId: string) {
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

		programsStore.update(programs => {
			const programIndex = programs.findIndex(p => p.id === programId);
			if (programIndex !== -1) {
				const updatedProgram = programs[programIndex];
				updatedProgram.cues = markers as any;
				programs[programIndex] = updatedProgram;
			}
			return [...programs];
		});

		debouncedSave();
	}

	function addMarker(time: number) {
		const labelText = 'No Preset';

		const markerRegion = regions!.addRegion({
			start: time,
			content: document.createElement('div'), // Temporary placeholder
			color: 'rgba(168, 85, 247, 0.3)',
			drag: true,
			resize: false
		});

		const labelElement = createRegionLabel(labelText, time, markerRegion.id);
		markerRegion.element!.replaceChildren(labelElement);

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

		const newMarker: Marker = {
			id: markerRegion.id,
			time: time,
			type: editMode,
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
	function updateMarkerProperty(markerId: string, property: keyof Marker, value: unknown) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			(marker as any)[property] = value;
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	/**
	 * Regenerate a marker's label in the waveform
	 * @param {string} markerId - Region ID
	 * @param {string} newLabel - New label text
	 */
	function regenerateMarkerLabel(markerId: string, newLabel: string) {
		if (regions) {
			const allRegions = regions.getRegions();
			const region = allRegions.find(r => r.id === markerId);
			if (region) {
				const newLabelElement = createRegionLabel(newLabel, region.start, markerId);
				region.element!.replaceChildren(newLabelElement);

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

	function updateMarkerPreset(markerId: string, presetName: string) {
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

	function toggleBoardSelection(markerId: string, boardId: string) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			if (marker.boards.includes(boardId)) {
				marker.boards = marker.boards.filter((id: string) => id !== boardId);
			} else {
				marker.boards = [...marker.boards, boardId];
			}
			markers = [...markers];
			syncMarkersToStore();
		}
	}

	function deleteMarker(markerId: string) {
		const allRegions = regions!.getRegions();
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

async function playFullProgram() {
		lastActiveProgramId.set(programId);

		if (isPaused) {
			isPaused = false;
			isPlaying = true;
			resumePlaybackService();
			return;
		}

		let currentProgram: ProgramModel | undefined;
		const unsubscribe = programsStore.subscribe(programs => {
			currentProgram = programs.find(p => p.id === programId);
		});
		unsubscribe();

		if (!currentProgram) return;

		const { ready, message } = await checkTrackReadiness();
		if (!ready) {
			resamplingModalMessage = message;
			resamplingModalOpen = true;
			return;
		}

		const currentTime = getPlayheadTime();

		isPlaying = true;
		playProgramService(currentProgram, currentTime);
	}

	function stopFullProgram() {
		lastActiveProgramId.set(programId);

		isPaused = true;
		stopPlayhead();

		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}

		pausePlaybackService();
	}

	function stopAndReset() {
		lastActiveProgramId.set(programId);

		isPaused = false;
		playheadTimeSecs = 0;
		stopPlayhead();

		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}

		stopPlaybackService();

		if (wavesurfer) {
			wavesurfer.seekTo(0);
		}
	}

	function saveProgram() {
		if (!songName.trim()) return;

		// Generate unique ID or use existing
		const timestamp = Date.now();
		const sanitizedSongName = songName.trim().replace(/\s+/g, '-').toLowerCase();
		const trackSuffix = loopyProTrack.trim() ? `-${loopyProTrack.trim()}` : '';
		const newProgramId = programId || `${sanitizedSongName}${trackSuffix}-${timestamp}`;

		// Get existing program data to preserve audioId
		let existingProgram: ProgramModel | undefined;
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
				type: m.type,
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
			clickRate: clickRate,
			guideVolume: guideVolume,
			displayOrder: existingProgram?.displayOrder ?? program?.displayOrder ?? 0
		};

		// Create Program model using factory
		const programInstance = ProgramModel.fromJson(programData);

		if (programInstance) {
			saveProgramToStore(programInstance, audioToUpload as string | null);
			console.log('💾 Auto-saved program:', newProgramId);

			audioToUpload = null;

			if (!programId) {
				programId = newProgramId;
			}
		}
	}

	function clearCues() {
		const toClear = visibleMarkers;
		if (toClear.length === 0) return;

		// Remove regions for current type only
		toClear.forEach(marker => {
			const region = regions!.getRegions().find(r => r.id === marker.id);
			if (region) region.remove();
		});

		// Keep markers of other types
		markers = markers.filter(m => m.type !== editMode);
		syncMarkersToStore();
	}

	function shiftCues(direction: 1 | -1) {
		if (!bpm || bpm <= 0 || visibleMarkers.length === 0) return;
		const shiftSeconds = (60 / bpm) * shiftAmount * direction;
		for (const marker of visibleMarkers) {
			marker.time = Math.max(0, marker.time + shiftSeconds);
			const region = regions?.getRegions().find(r => r.id === marker.id);
			if (region) {
				region.setOptions({ start: marker.time, end: marker.time });
			}
		}
		markers = [...markers];
		syncMarkersToStore();
	}

	async function handleDuplicate() {
		if (!programId) return;
		try {
			await duplicateProgramService(programId);
		} catch (err) {
			console.error('Failed to duplicate program:', err);
		}
	}

	async function handleReplaceAudio() {
		if (!programId) return;

		const input = document.createElement('input');
		input.type = 'file';
		input.accept = 'audio/*,.wav,.mp3,.ogg,.flac';
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) return;

			const savedMarkers = [...markers];

			markers = [];
			if (regions) {
				regions.getRegions().forEach(r => {
					if (!gridRegionIds.includes(r.id)) r.remove();
				});
			}
			if (wavesurfer) {
				wavesurfer.destroy();
				wavesurfer = null;
			}
			isLoaded = false;
			wavesurferInitialized = true;

			const container = document.querySelector(`#waveform-${sanitizedProgramId}`);
			if (container) container.innerHTML = '';

			clearAudioCacheForProgram(programId!);

			try {
				const arrayBuffer = await file.arrayBuffer();

				const dataUrl = await new Promise<string>((resolve, reject) => {
					const reader = new FileReader();
					reader.onload = (e) => resolve(e.target!.result as string);
					reader.onerror = reject;
					reader.readAsDataURL(file);
				});

				const response = await fetch(`${API_URL}/programs/${encodeURIComponent(programId!)}/replace-audio`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ data_url: dataUrl })
				});

				if (!response.ok) {
					const errText = await response.text();
					console.error('Failed to replace audio:', response.status, errText);
					alert('Failed to replace audio: ' + errText);
					return;
				}

				const updated = await response.json();

				if (program) {
					program.audioId = updated.audio_file;
					program.audioFile = updated.audio_file;
					program.audioDuration = null;
				}

				programsStore.update(programs => {
					const idx = programs.findIndex(p => p.id === programId);
					if (idx !== -1) {
						programs[idx].audioId = updated.audio_file;
						programs[idx].audioFile = updated.audio_file;
						programs[idx].audioDuration = undefined;
					}
					return [...programs];
				});

				const audioCtx = new AudioContext();
				const audioBuffer = await audioCtx.decodeAudioData(arrayBuffer);
				audioCtx.close();

				const peaks = computePeaksFromBuffer(audioBuffer);
				const dur = audioBuffer.duration;

				cachedPeaksStore.update(cache => ({
					...cache,
					[programId!]: { peaks, duration: dur }
				}));

				await fetch(`${API_URL}/audio/${encodeURIComponent(updated.audio_file)}/peaks`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ peaks, duration: dur })
				});

				pendingCues = savedMarkers;
				fileName = file.name;

				setTimeout(() => {
					initializeWaveSurfer();
				}, 100);
			} catch (err) {
				console.error('Failed to replace audio:', err);
				alert('Failed to replace audio');
			}
		};
		input.click();
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

		// Apply default board to visible cues only
		markers = markers.map(marker => {
			if (marker.type !== editMode) return marker;
			return { ...marker, boards: [defaultTargetBoard!] };
		});

		syncMarkersToStore();
	}

	function selectDefaultBoard(boardId: string) {
		defaultTargetBoard = boardId;
		defaultBoardDropdownOpen = false;
		debouncedSave();
	}

	function getDefaultBoardLabel() {
		if (!defaultTargetBoard) return 'Default';
		return defaultTargetBoard;
	}

	$effect(() => {
		if (!program?.audioId || wavesurferInitialized) return;

		const peaks = programId ? $cachedPeaksStore[programId] : null;
		if (peaks) {
			wavesurferInitialized = true;
			initializeWaveSurfer();
		} else if (!$audioLoading) {
			loadAudioForProgram(program.id, program.audioId);
		}
	});

	$effect(() => {
		if (!program?.guideAudioId || !isLoaded) return;

		const peaks = program.id ? $guideCachedPeaks[program.id] : null;
		if (!peaks && !$audioLoading) {
			loadGuideAudioForProgram(program.id, program.guideAudioId);
		}
	});

	$effect(() => {
		if (guideBlobUrl && guideVolume !== 1.0) {
			setSlotVolume('guide', guideVolume);
		}
	});

	// Toggle marker region visibility based on edit mode
	$effect(() => {
		const mode = editMode;
		const allMarkers = markers;
		if (!regions) return;

		for (const region of regions.getRegions()) {
			if (gridRegionIds.includes(region.id)) continue;
			if (!region.element) continue;

			const marker = allMarkers.find(m => m.id === region.id);
			if (!marker) continue;

			const visible = marker.type === mode;
			region.element.style.visibility = visible ? 'visible' : 'hidden';
		}
	});

	$effect(() => {
		const complete = $resamplingCompleteStore;
		if (resamplingModalOpen && !backingProgress && !guideProgress) {
			checkTrackReadiness().then(({ ready }) => {
				if (ready) {
					resamplingModalOpen = false;
				}
			});
		}
	});

	onDestroy(() => {
		stopPlayhead();
		if (seekDebounceTimeout) {
			clearTimeout(seekDebounceTimeout);
			seekDebounceTimeout = null;
		}
		if (saveTimeout) {
			clearTimeout(saveTimeout);
			saveTimeout = null;
		}
	});
</script>

<div class="program-editor" onclick={() => { actionMenuOpen = false; defaultBoardDropdownOpen = false; }}>
	<div class="waveform-container">
		<div class="program-transport-controls">
			{#if isPlaying}
				<button class="btn-program-pause" onclick={stopFullProgram}>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="2" width="4" height="12"/><rect x="9" y="2" width="4" height="12"/></svg>
				</button>
			{:else}
				<button class="btn-program-play" onclick={playFullProgram}>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><polygon points="3,1 14,8 3,15"/></svg>
				</button>
			{/if}
			<button class="btn-program-stop" onclick={stopAndReset} title="Stop and reset to start">
				<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><rect x="2" y="2" width="12" height="12" rx="1"/></svg>
			</button>
			<button class="song-name-btn" onclick={() => metadataModalOpen = true}>
				{songName || 'Untitled'}
			</button>
			<div class="spacer"></div>
			<div class="click-group" title="Click track">
				<button
					class="click-mute-btn"
					class:muted={$slotMuted.click}
					onclick={() => toggleSlotMute('click')}
				>Click</button>
				<button
					class="click-rate-btn"
					class:active={clickRate === 0.5}
					onclick={() => { clickRate = 0.5; debouncedSave(); }}
				>½</button>
				<button
					class="click-rate-btn"
					class:active={clickRate === 1}
					onclick={() => { clickRate = 1; debouncedSave(); }}
				>1</button>
				<button
					class="click-rate-btn"
					class:active={clickRate === 2}
					onclick={() => { clickRate = 2; debouncedSave(); }}
				>2</button>
			</div>
			<div class="action-menu-wrapper">
				<button
					class="btn-action-menu"
					onclick={(e) => {
						e.stopPropagation();
						actionMenuOpen = !actionMenuOpen;
					}}
					title="Actions"
				>
					⋯
				</button>
				{#if actionMenuOpen && programId}
					<div class="action-menu-dropdown">
						<button class="action-menu-item" onclick={() => { handleDuplicate(); actionMenuOpen = false; }}>Duplicate</button>
						<button class="action-menu-item" onclick={() => { downloadProgram(); actionMenuOpen = false; }}>Download</button>
						<button class="action-menu-item" onclick={() => { handleReplaceAudio(); actionMenuOpen = false; }}>Replace audio</button>
						<button class="action-menu-item" onclick={() => { resampledDialogOpen = true; fetchResampledInfo(); actionMenuOpen = false; }}>Resampled</button>
						<button class="action-menu-item action-menu-item-danger" onclick={() => { deleteProgram(); actionMenuOpen = false; }}>Delete</button>
					</div>
				{/if}
			</div>
		</div>
		<div class="track-label" style="--track-color: {backingSlot.color}">
			<button class="mute-btn" class:muted={$slotMuted.backing} onclick={() => toggleSlotMute('backing')}>{backingSlot.label}</button>
			{#if backingProgress}
				<span class="resampling-inline">
					{backingProgress.trackName} • {(backingProgress.fromRate / 1000).toFixed(1)}kHz → {(backingProgress.toRate / 1000).toFixed(1)}kHz • {Math.round((backingProgress.current / backingProgress.total) * 100)}%
				</span>
			{/if}
			<div class="track-label-actions">
				{#if editMode === 'lighting' && bpm && bpm > 0 && visibleMarkers.length > 0}
					<div class="mode-btn-group">
						{#each [{ label: '¼', value: 0.25 }, { label: '½', value: 0.5 }, { label: '1', value: 1 }, { label: '2', value: 2 }, { label: '4', value: 4 }] as opt}
							<button
								class="mode-btn"
								class:active={shiftAmount === opt.value}
								onclick={() => shiftAmount = opt.value}
							>{opt.label}</button>
						{/each}
					</div>
					<div class="zoom-btn-group">
						<button class="zoom-btn" onclick={() => shiftCues(-1)} title="Shift cues earlier">−</button>
						<button class="zoom-btn" onclick={() => shiftCues(1)} title="Shift cues later">+</button>
					</div>
				{/if}
				<div class="mode-btn-group">
					<button
						class="mode-btn"
						class:active={editMode === 'lighting'}
						onclick={() => editMode = 'lighting'}
					>Lighting</button>
					<button
						class="mode-btn"
						class:active={editMode === 'midi'}
						onclick={() => editMode = 'midi'}
					>Midi</button>
				</div>
				{#if isLoaded}
					<div class="zoom-btn-group">
						<svg class="zoom-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
							<circle cx="7" cy="7" r="5.5" stroke="currentColor" stroke-width="1.5"/>
							<path d="M11 11L14.5 14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
						</svg>
						<button class="zoom-btn" onclick={zoomOut} title="Zoom Out">−</button>
						<button class="zoom-btn" onclick={zoomIn} title="Zoom In">+</button>
					</div>
					<div class="zoom-btn-group" title="Grid density">
						<svg class="zoom-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
							<path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
						</svg>
						<button class="zoom-btn" onclick={gridSparser} title="Sparser Grid">−</button>
						<button class="zoom-btn" onclick={gridDenser} title="Denser Grid">+</button>
					</div>
				{/if}
			</div>
		</div>
		<div class="backing-waveform">
			{#if !isLoaded && (program?.audioId || program?.audioData)}
				<div class="waveform-skeleton"></div>
			{/if}
			<div id="waveform-{sanitizedProgramId}" class:hidden={!isLoaded && (program?.audioId || program?.audioData)}></div>
		</div>

		{#if editMode === 'lighting'}
		<div class="backing-controls" class:has-cues={isLoaded}>
			{#if isLoaded}
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
						<div class="default-board-dropdown-menu" onclick={(e) => e.stopPropagation()}>
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

				{#if visibleMarkers.length > 0}
					<button class="cue-count-badge-wrapper" onclick={clearCues}>
						<span class="cue-count-badge">{visibleMarkers.length}</span>
						<span class="clear-cues-text">Clear Cues</span>
					</button>
				{:else}
					<div class="cue-count-badge-wrapper-static">
						<span class="cue-count-badge">0</span>
					</div>
				{/if}
			{/if}
		</div>
		{/if}

		{#if editMode === 'lighting' && visibleMarkers.length > 0 && currentlySelectedMarker}
			{@const marker = visibleMarkers.find(m => m.id === currentlySelectedMarker)}
			{#if marker}
				<CueEditor
					{marker}
					onToggleBoardSelection={toggleBoardSelection}
					onOpenPresetPicker={openPresetPicker}
					onUpdateSyncRate={(markerId: string, rate: number) => updateMarkerProperty(markerId, 'syncRate', rate)}
					onDelete={deleteMarker}
				/>
			{/if}
		{/if}

		{#if isLoaded}
			<div class="guide-section">
				{#if hasGuide}
					<Track
						track={guideSlot}
						programId={programId}
						blobUrl={guideBlobUrl}
						cachedPeaks={guidePeaks}
						resamplingProgress={guideProgress}
						mainWavesurfer={wavesurfer}
						onRemove={removeGuide}
						volume={guideVolume}
						onVolumeChange={(vol) => {
							guideVolume = vol;
							setSlotVolume('guide', vol);
							debouncedSave();
						}}
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
								const file = (e.target as HTMLInputElement)?.files?.[0];
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
	</div>
</div>

<PresetPicker
	open={presetPickerOpen}
	onSelect={handlePresetSelect}
	onClose={closePresetPicker}
/>

{#if metadataModalOpen}
	<div class="modal-overlay" onclick={() => metadataModalOpen = false}>
		<div class="metadata-modal" onclick={(e) => e.stopPropagation()}>
			<div class="metadata-modal-header">
				<h3>Program Details</h3>
				<button class="modal-close-btn" onclick={() => metadataModalOpen = false}>×</button>
			</div>
			<div class="metadata-modal-body">
				<div class="metadata-field">
					<label for="meta-name">Name</label>
					<input
						id="meta-name"
						type="text"
						bind:value={songName}
						placeholder="Song name"
						oninput={debouncedSave}
					/>
				</div>
				{#if $loopyProSettings.audio_source === 'loopy_pro'}
					<div class="metadata-field">
						<label for="meta-track">Loopy Pro Track</label>
						<input
							id="meta-track"
							type="text"
							bind:value={loopyProTrack}
							placeholder="Track number"
							maxlength="2"
							oninput={debouncedSave}
						/>
					</div>
				{/if}
				<div class="metadata-field">
					<label for="meta-bpm">BPM</label>
					<input
						id="meta-bpm"
						type="number"
						bind:value={bpm}
						placeholder="BPM"
						min="20"
						max="300"
						oninput={() => { updateBeatGrid(); debouncedSave(); }}
					/>
				</div>
			</div>
			{#if fileName}
				<div class="metadata-modal-footer">
					<span class="metadata-filename">{fileName}</span>
				</div>
			{/if}
		</div>
	</div>
{/if}

{#if resamplingModalOpen}
	<div class="modal-overlay" onclick={() => resamplingModalOpen = false}>
		<div class="resampling-modal" onclick={(e) => e.stopPropagation()}>
			<div class="resampling-modal-content">
				<div class="resampling-spinner"></div>
				<p>{resamplingModalMessage}</p>
				{#if backingProgress}
					<p class="resampling-track-progress">Backing: {(backingProgress.fromRate / 1000).toFixed(1)}kHz → {(backingProgress.toRate / 1000).toFixed(1)}kHz — {Math.round((backingProgress.current / backingProgress.total) * 100)}%</p>
				{/if}
				{#if guideProgress}
					<p class="resampling-track-progress">Guide: {(guideProgress.fromRate / 1000).toFixed(1)}kHz → {(guideProgress.toRate / 1000).toFixed(1)}kHz — {Math.round((guideProgress.current / guideProgress.total) * 100)}%</p>
				{/if}
				{#if !backingProgress && !guideProgress}
					<p class="resampling-hint">Please wait for resampling to complete.</p>
				{/if}
			</div>
			<button class="resampling-modal-close" onclick={() => resamplingModalOpen = false}>OK</button>
		</div>
	</div>
{/if}

{#if resampledDialogOpen}
	<div class="modal-overlay" onclick={() => resampledDialogOpen = false}>
		<div class="resampled-dialog" onclick={(e) => e.stopPropagation()}>
			<div class="resampled-dialog-header">
				<h3>Resampled Versions</h3>
				<button class="modal-close-btn" onclick={() => resampledDialogOpen = false}>×</button>
			</div>
			<div class="resampled-dialog-body">
				{#if resampledLoading}
					<p class="resampled-loading">Loading...</p>
				{:else if resampledInfo}
					{#each [
						{ label: 'Backing', info: resampledInfo.backing, progress: backingProgress },
						{ label: 'Guide', info: resampledInfo.guide, progress: guideProgress }
					] as track}
						{#if track.info}
							<div class="resampled-track-section">
								<div class="resampled-track-header">
									{track.label} — {(track.info.original_rate / 1000).toFixed(1)}kHz {track.info.channels === 2 ? 'stereo' : `${track.info.channels}ch`}
								</div>
								{#if track.info.versions.length === 0}
									<p class="resampled-none">No resampled versions cached</p>
								{:else}
									{#each track.info.versions as version}
									{@const isResampling = track.progress && track.progress.active && track.progress.toRate === version.target_rate}
										<div class="resampled-version-row">
											<div class="resampled-version-info">
												<span class="resampled-rate">{(version.target_rate / 1000).toFixed(1)}kHz</span>
												<span class="resampled-size">{formatBytes(version.size_bytes)}</span>
											</div>
											<div class="resampled-quality-row">
											{#each ['fast', 'balanced', 'high'] as q}
												<button
													class="resampled-quality-btn"
													class:active={version.quality === q}
													disabled={isResampling}
													onclick={() => {
														if (version.quality !== q) {
															triggerResample(track.info.track_id, version.target_rate, q);
														}
													}}
												>{q.charAt(0).toUpperCase() + q.slice(1)}</button>
											{/each}
											{#if isResampling}
												<span class="resampled-percent">{Math.round((track.progress!.current / track.progress!.total) * 100)}%</span>
											{/if}
										</div>
										</div>
									{/each}
								{/if}
							</div>
						{/if}
					{/each}
					{#if !resampledInfo.backing && !resampledInfo.guide}
						<p class="resampled-none">No tracks loaded in audio engine</p>
					{/if}
				{:else}
					<p class="resampled-none">Could not load resampled info</p>
				{/if}
			</div>
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

	.program-transport-controls {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 1rem;
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

	.song-name-btn {
		background-color: transparent;
		border: 1px solid #1a1a1a;
		color: #e5e5e5;
		padding: 0.5rem 1.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		cursor: pointer;
		transition: all 0.2s;
		max-width: 300px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		text-align: left;
	}

	.song-name-btn:hover {
		border-color: #333;
		background-color: #111;
	}

	.metadata-modal {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 12px;
		min-width: 320px;
		max-width: 90vw;
	}

	.metadata-modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem 1.25rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	.metadata-modal-header h3 {
		margin: 0;
		font-size: 1rem;
		font-weight: 500;
		color: #e5e5e5;
	}

	.modal-close-btn {
		background: transparent;
		border: none;
		color: #666;
		font-size: 1.5rem;
		cursor: pointer;
		padding: 0;
		line-height: 1;
	}

	.modal-close-btn:hover {
		color: #999;
	}

	.metadata-modal-body {
		padding: 1.25rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.metadata-field {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.metadata-field label {
		font-size: 0.75rem;
		color: #888;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.metadata-field input {
		background-color: #0a0a0a;
		border: 1px solid rgba(255, 255, 255, 0.1);
		color: #e5e5e5;
		padding: 0.75rem;
		border-radius: 8px;
		font-size: 0.9rem;
		transition: border-color 0.2s;
	}

	.metadata-field input:hover {
		border-color: rgba(255, 255, 255, 0.2);
	}

	.metadata-field input:focus {
		outline: none;
		border-color: rgba(255, 255, 255, 0.3);
	}

	.metadata-field input::placeholder {
		color: #444;
	}

	.metadata-field input[type="number"] {
		-moz-appearance: textfield;
	}

	.metadata-field input[type="number"]::-webkit-outer-spin-button,
	.metadata-field input[type="number"]::-webkit-inner-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}

	.metadata-modal-footer {
		padding: 1rem 1.25rem;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
		text-align: center;
	}

	.metadata-filename {
		font-size: 0.75rem;
		color: #555;
	}

	.click-group {
		display: flex;
		background-color: transparent;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		overflow: hidden;
		height: 36px;
		box-sizing: border-box;
	}

	.click-mute-btn {
		background: transparent;
		border: none;
		color: #fbbf24;
		padding: 0 0.9rem;
		font-size: 0.75rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		cursor: pointer;
		border-right: 1px solid #1a1a1a;
		transition: all 0.15s;
		height: 100%;
	}

	.click-mute-btn:hover {
		background: #111;
	}

	.click-mute-btn.muted {
		color: #444;
	}

	.click-rate-btn {
		background: transparent;
		border: none;
		color: #444;
		padding: 0 0.75rem;
		font-size: 0.875rem;
		cursor: pointer;
		border-right: 1px solid #1a1a1a;
		transition: all 0.15s;
		height: 100%;
	}

	.click-rate-btn:last-child {
		border-right: none;
	}

	.click-rate-btn:hover {
		background: #111;
		color: #888;
	}

	.click-rate-btn.active {
		background: #1a1a1a;
		color: #fff;
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

	.backing-waveform {
		position: relative;
		min-height: 140px;
	}

	.backing-waveform:has(+ .backing-controls:not(.has-cues)) {
		margin-bottom: -40px;
	}

	/* Custom scrollbar for waveform */
	.backing-waveform ::-webkit-scrollbar {
		height: 8px;
	}

	.backing-waveform ::-webkit-scrollbar-track {
		background: transparent;
	}

	.backing-waveform ::-webkit-scrollbar-thumb {
		background: rgba(168, 85, 247, 0.5);
		border-radius: 4px;
	}

	.backing-waveform ::-webkit-scrollbar-thumb:hover {
		background: rgba(168, 85, 247, 0.7);
	}

	/* Firefox scrollbar */
	.backing-waveform * {
		scrollbar-width: thin;
		scrollbar-color: rgba(168, 85, 247, 0.5) transparent;
	}

	.guide-section {
		margin-top: 0;
		padding-top: 0;
		padding-bottom: 0.5rem;
	}

	.track-label {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.25rem 1rem;
		font-size: 0.75rem;
		color: var(--track-color, #888);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.mute-btn {
		background: none;
		border: none;
		padding: 0;
		font-size: inherit;
		font-weight: inherit;
		text-transform: inherit;
		letter-spacing: inherit;
		color: var(--track-color, #888);
		cursor: pointer;
		transition: color 0.15s;
	}

	.mute-btn:hover {
		opacity: 0.7;
	}

	.mute-btn.muted {
		color: #444;
	}

	.track-label-actions {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-left: auto;
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

	.backing-controls {
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

	.backing-controls.has-cues {
		opacity: 1;
	}

	.zoom-btn-group {
		display: flex;
		align-items: center;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		height: 28px;
		box-sizing: border-box;
	}

	.zoom-icon {
		color: #555;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		padding: 0.25rem 0.35rem;
	}

	.zoom-btn {
		background: transparent;
		border: none;
		color: #555;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 4px;
		transition: all 0.15s;
		font-size: 1rem;
		font-weight: 500;
		line-height: 1;
	}

	.zoom-btn:hover {
		color: #888;
		background: #111;
	}

	.mode-btn-group {
		display: flex;
		align-items: center;
		border: 1px solid #1a1a1a;
		border-radius: 6px;
		height: 28px;
		box-sizing: border-box;
		overflow: hidden;
	}

	.mode-btn {
		background: transparent;
		border: none;
		color: #555;
		cursor: pointer;
		padding: 0 0.75rem;
		height: 100%;
		font-size: 0.75rem;
		transition: all 0.15s;
	}

	.mode-btn:hover {
		color: #888;
		background: #111;
	}

	.mode-btn.active {
		background: #1a1a1a;
		color: #fff;
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

	.modal-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 2000;
	}

	.resampling-modal {
		background: #0c0c0c;
		border: 1px solid #1a1a1a;
		border-radius: 12px;
		padding: 1.5rem;
		min-width: 300px;
		text-align: center;
	}

	.resampling-modal-content {
		margin-bottom: 1rem;
	}

	.resampling-modal-content p {
		margin: 0.5rem 0;
		color: #e5e5e5;
	}

	.resampling-hint {
		color: #666 !important;
		font-size: 0.85rem;
	}

	.resampling-track-progress {
		color: #a78bfa !important;
		font-size: 0.85rem;
		margin: 0.25rem 0;
	}

	.resampling-spinner {
		width: 24px;
		height: 24px;
		border: 2px solid #333;
		border-top-color: #a78bfa;
		border-radius: 50%;
		animation: spin 1s linear infinite;
		margin: 0 auto 1rem;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.resampling-modal-close {
		background: #1a1a1a;
		border: 1px solid #333;
		color: #888;
		padding: 0.5rem 1.5rem;
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.15s;
	}

	.resampling-modal-close:hover {
		background: #222;
		color: #ccc;
	}

	.resampled-dialog {
		background: #0c0c0c;
		border: 1px solid #1a1a1a;
		border-radius: 12px;
		min-width: 460px;
		max-width: 560px;
	}

	.resampled-dialog-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 1rem 1.25rem;
		border-bottom: 1px solid #1a1a1a;
	}

	.resampled-dialog-header h3 {
		margin: 0;
		font-size: 1rem;
		color: #e5e5e5;
	}

	.resampled-dialog-body {
		padding: 1rem 1.25rem;
	}

	.resampled-loading {
		color: #666;
		text-align: center;
		margin: 1rem 0;
	}

	.resampled-track-section {
		margin-bottom: 1rem;
	}

	.resampled-track-section:last-child {
		margin-bottom: 0;
	}

	.resampled-track-header {
		font-size: 0.85rem;
		color: #888;
		margin-bottom: 0.5rem;
		font-weight: 500;
	}

	.resampled-none {
		color: #555;
		font-size: 0.85rem;
		margin: 0.25rem 0;
	}

	.resampled-version-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.5rem 0;
		border-top: 1px solid rgba(255, 255, 255, 0.03);
	}

	.resampled-version-info {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.resampled-rate {
		color: #ccc;
		font-size: 0.9rem;
		font-weight: 500;
	}

	.resampled-size {
		color: #555;
		font-size: 0.8rem;
	}

	.resampled-quality-row {
		display: flex;
		gap: 0.35rem;
	}

	.resampled-quality-btn {
		background: transparent;
		border: 1px solid #222;
		color: #555;
		padding: 0.25rem 0.6rem;
		border-radius: 6px;
		font-size: 0.75rem;
		cursor: pointer;
		transition: all 0.15s;
	}

	.resampled-quality-btn:hover {
		background: #111;
		color: #888;
		border-color: rgba(255, 255, 255, 0.08);
	}

	.resampled-quality-btn.active {
		background: rgba(34, 197, 94, 0.1);
		color: #22c55e;
		border-color: rgba(34, 197, 94, 0.3);
	}

	.resampled-quality-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.resampled-percent {
		color: #a78bfa;
		font-size: 0.8rem;
		font-weight: 500;
		min-width: 2.5em;
		text-align: right;
	}

</style>
