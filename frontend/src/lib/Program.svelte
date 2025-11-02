<script>
	import { onMount, createEventDispatcher } from 'svelte';
	import WaveSurfer from 'wavesurfer.js';
	import RegionsPlugin from 'wavesurfer.js/dist/plugins/regions.esm.js';
	import { API_URL } from '$lib/api';

	const dispatch = createEventDispatcher();

	// Props
	let {
		programId = null,
		initialData = null,
		playingProgramId = null,
		onplay = null,
		onstop = null
	} = $props();

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

	// Boards and groups
	let boards = $state([]);
	let groups = $state([]);
	let presets = $state([]);

	// Dropdown state
	let openDropdownId = $state(null);

	// WLED effects list
	const effects = [
		'Solid', 'Blink', 'Breathe', 'Wipe', 'Wipe Random', 'Random Colors', 'Sweep', 'Dynamic', 'Colorloop', 'Rainbow',
		'Scan', 'Dual Scan', 'Fade', 'Theater Chase', 'Theater Chase Rainbow', 'Running', 'Saw', 'Twinkle', 'Dissolve', 'Dissolve Rnd',
		'Sparkle', 'Sparkle Dark', 'Sparkle+', 'Strobe', 'Strobe Rainbow', 'Strobe Mega', 'Blink Rainbow', 'Android', 'Chase', 'Chase Random',
		'Chase Rainbow', 'Chase Flash', 'Chase Flash Rnd', 'Rainbow Runner', 'Colorful', 'Traffic Light', 'Sweep Random', 'Running 2', 'Red & Blue', 'Stream',
		'Scanner', 'Lighthouse', 'Fireworks', 'Rain', 'Merry Christmas', 'Fire Flicker', 'Gradient', 'Loading', 'Police', 'Police All',
		'Two Dots', 'Two Areas', 'Circus', 'Halloween', 'Tri Chase', 'Tri Wipe', 'Tri Fade', 'Lightning', 'ICU', 'Multi Comet',
		'Scanner Dual', 'Stream 2', 'Oscillate', 'Pride 2015', 'Juggle', 'Palette', 'Fire 2012', 'Colorwaves', 'Bpm', 'Fill Noise',
		'Noise 1', 'Noise 2', 'Noise 3', 'Noise 4', 'Colortwinkles', 'Lake', 'Meteor', 'Meteor Smooth', 'Railway', 'Ripple',
		'Twinklefox', 'Twinklecat', 'Halloween Eyes', 'Solid Pattern', 'Solid Pattern Tri', 'Spots', 'Spots Fade', 'Glitter', 'Candle', 'Fireworks Starburst',
		'Fireworks 1D', 'Bouncing Balls', 'Sinelon', 'Sinelon Dual', 'Sinelon Rainbow', 'Popcorn', 'Drip', 'Plasma', 'Percent', 'Ripple Rainbow',
		'Heartbeat', 'Pacifica', 'Candle Multi', 'Solid Glitter', 'Sunrise', 'Phased', 'Twinkleup', 'Noise Pal', 'Sine', 'Phased Noise',
		'Flow', 'Chunchun', 'Dancing Shadows', 'Washing Machine', 'Candy Cane', 'Blends', 'TV Simulator', 'Dynamic Smooth'
	];

	onMount(async () => {
		// Fetch boards and groups
		try {
			const res = await fetch(`${API_URL}/boards`);
			const data = await res.json();

			// Backend returns array of boards directly
			boards = Array.isArray(data) ? data : [];

			// Groups are stored in localStorage (frontend-only)
			const storedGroups = JSON.parse(localStorage.getItem('board-groups') || '[]');
			groups = storedGroups;

			// Fetch presets (0-16 for WLED)
			presets = Array.from({ length: 17 }, (_, i) => ({ id: i, name: i === 0 ? 'None' : `Preset ${i}` }));
		} catch (err) {
			console.error('Failed to fetch boards:', err);
		}

		// Load initial data if provided
		if (initialData) {
			loadProgramData(initialData);

			// Auto-load audio if compressed audio data is present
			if (initialData.audioData) {
				setTimeout(() => {
					loadCompressedAudio(initialData.audioData);
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
					container: '#waveform',
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
							const markerRegion = regions.addRegion({
								start: cue.time,
								content: cue.label,
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

				// Click on waveform to add marker
				wavesurfer.on('click', (relativeX) => {
					if (!isLoaded) return;

					const duration = wavesurfer.getDuration();
					const clickTime = relativeX * duration;

					addMarker(clickTime);
				});

				// Update marker list when regions change
				regions.on('region-updated', (region) => {
					const markerIndex = markers.findIndex(m => m.id === region.id);
					if (markerIndex !== -1) {
						markers[markerIndex].time = region.start;
						markers = [...markers]; // Trigger reactivity
					}
				});

				regions.on('region-removed', (region) => {
					markers = markers.filter(m => m.id !== region.id);
				});

				const url = URL.createObjectURL(file);
				console.log('Loading URL:', url);
				window._currentAudioUrl = url; // Store for later reload
				wavesurfer.load(url);

				// Clear existing markers if not loading program
				if (!initialData) {
					markers = [];
				}
			}, 100);
		} else {
			alert('Please select an audio file (WAV, MP3, etc.)');
		}
	}

	function loadCompressedAudio(audioDataURL) {
		console.log('Loading compressed audio from localStorage');

		// Convert base64 data URL back to blob
		fetch(audioDataURL)
			.then(res => res.blob())
			.then(blob => {
				const url = URL.createObjectURL(blob);

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
							const markerRegion = regions.addRegion({
								start: cue.time,
								content: cue.label,
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

				// Click on waveform to add marker
				wavesurfer.on('click', (relativeX) => {
					if (!isLoaded) return;

					const duration = wavesurfer.getDuration();
					const clickTime = relativeX * duration;

					addMarker(clickTime);
				});

				// Update marker list when regions change
				regions.on('region-updated', (region) => {
					const markerIndex = markers.findIndex(m => m.id === region.id);
					if (markerIndex !== -1) {
						markers[markerIndex].time = region.start;
						markers = [...markers]; // Trigger reactivity
					}
				});

				regions.on('region-removed', (region) => {
					markers = markers.filter(m => m.id !== region.id);
				});

				// Load the compressed audio
				wavesurfer.load(url);
			})
			.catch(err => {
				console.error('Failed to load compressed audio:', err);
			});
	}

	function addMarker(time) {
		const currentCount = markers.length;
		const markerRegion = regions.addRegion({
			start: time,
			content: `Cue ${currentCount + 1}`,
			color: 'rgba(168, 85, 247, 0.3)',
			drag: true,
			resize: false
		});

		const newMarker = {
			id: markerRegion.id,
			time: time,
			label: `Cue ${currentCount + 1}`,
			boards: [],
			preset: 0,
			effect: 0,
			color: '#ff0000',
			brightness: 255,
			transition: 0
		};

		markers = [...markers, newMarker];
	}

	function updateMarkerEffect(markerId, effectIndex) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.effect = effectIndex;
			markers = [...markers];
		}
	}

	function updateMarkerColor(markerId, color) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.color = color;
			markers = [...markers];
		}
	}

	function updateMarkerPreset(markerId, presetId) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.preset = presetId;
			markers = [...markers];
		}
	}

	function updateMarkerBrightness(markerId, brightness) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.brightness = brightness;
			markers = [...markers];
		}
	}

	function updateMarkerBoards(markerId, selectedBoards) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.boards = selectedBoards;
			markers = [...markers];
		}
	}

	function updateMarkerLabel(markerId, newLabel) {
		const marker = markers.find(m => m.id === markerId);
		if (marker) {
			marker.label = newLabel;
			markers = [...markers];
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
			const board = [...boards, ...groups].find(b => b.id === selectedBoards[0]);
			return board ? board.name : '1 selected';
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
		// Clear LED cue timeouts
		if (onstop) {
			onstop();
		}
		// Stop and reset to beginning, then play
		if (wavesurfer) {
			// Store current URL to force reload
			const currentUrl = wavesurfer.getMediaElement?.()?.src || 
							 (window._currentAudioUrl || '');
			
			if (currentUrl) {
				// Force complete reload to reset everything
				window._currentAudioUrl = currentUrl;
				wavesurfer.load(currentUrl);
				
				// Wait for decode, then play from beginning
				wavesurfer.once('decode', () => {
					wavesurfer.play();
				});
			} else {
				// Fallback
				wavesurfer.stop();
				wavesurfer.play();
			}
		}
		// Schedule fresh LED cues
		if (onplay) {
			onplay();
		}
	}

	function stopFullProgram() {
		if (wavesurfer) {
			wavesurfer.pause();
		}
		if (onstop) {
			onstop();
		}
	}

	function returnToStart() {
		// Stop playback and clear LED cue timeouts
		if (onstop) {
			onstop();
		}
		// Pause and reset to start
		if (wavesurfer) {
			wavesurfer.pause();
			wavesurfer.setTime(0);
		}
	}

	function saveProgram() {
		// Validation
		if (!songName.trim()) {
			alert('Please enter a song name');
			return;
		}

		if (markers.length === 0) {
			alert('Please add at least one cue marker');
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

		// Create program object
		const program = {
			id: newProgramId,
			songName: songName.trim(),
			loopyProTrack: loopyProTrack.trim(),
			fileName: fileName,
			audioData: initialData?.audioData, // Preserve audio data
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
			createdAt: new Date().toISOString()
		};

		// Save to localStorage
		const existingPrograms = JSON.parse(localStorage.getItem('light-programs') || '[]');

		if (programId) {
			// Update existing program - preserve audioData from existing
			const index = existingPrograms.findIndex(p => p.id === programId);
			if (index !== -1) {
				program.audioData = existingPrograms[index].audioData; // Keep existing audio
				existingPrograms[index] = program;
			}
		} else {
			// Add new program
			existingPrograms.push(program);
			programId = newProgramId;
		}

		localStorage.setItem('light-programs', JSON.stringify(existingPrograms));

		alert(`Program saved: ${newProgramId}.json`);
		dispatch('save', program);
	}

	function clearCues() {
		if (markers.length === 0) return;

		const confirmed = confirm(
			`Are you sure you want to clear all ${markers.length} cue(s)? This cannot be undone.`
		);

		if (confirmed) {
			// Remove all regions from waveform
			const allRegions = regions.getRegions();
			allRegions.forEach(region => region.remove());

			// Clear markers array
			markers = [];
		}
	}

	function deleteProgram() {
		if (!programId) return;

		const confirmed = confirm(
			`Are you sure you want to delete "${songName}"? This cannot be undone.`
		);

		if (confirmed) {
			const existingPrograms = JSON.parse(localStorage.getItem('light-programs') || '[]');
			const filtered = existingPrograms.filter(p => p.id !== programId);
			localStorage.setItem('light-programs', JSON.stringify(filtered));

			dispatch('delete', programId);
		}
	}
</script>

<div class="program-editor">
	<div class="waveform-container">
		<div class="waveform-header">
			{#if playingProgramId === programId}
				<button class="btn-program-stop" onclick={stopFullProgram}>
					⏸
				</button>
			{:else}
				<button class="btn-program-play" onclick={playFullProgram}>
					▶
				</button>
			{/if}
			<button class="btn-return-start" onclick={returnToStart} title="Return to start">
				⏮
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
			<button class="btn-save" onclick={saveProgram}>
				Save
			</button>
			<button class="btn-clear" onclick={clearCues}>
				Clear Cues
			</button>
			{#if programId}
				<button class="btn-delete-program" onclick={deleteProgram}>
					Delete
				</button>
			{/if}
		</div>
		<div id="waveform-{programId}"></div>
		{#if isLoaded && markers.length > 0}
			<div class="waveform-footer">
				<button class="btn-collapse" onclick={() => cuesExpanded = !cuesExpanded}>
					{cuesExpanded ? '▼' : '▶'} Cues ({markers.length})
				</button>
			</div>
		{/if}
		{#if !isLoaded && !initialData?.audioData}
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
										<div class="boards-dropdown-menu">
											{#if boards.length > 0}
												<div class="dropdown-section">
													<div class="dropdown-section-label">Boards</div>
													{#each boards as board}
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
															<span>{group.name}</span>
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
									{#each presets as preset}
										<option value={preset.id}>{preset.name}</option>
									{/each}
								</select>

								<input
									type="color"
									value={marker.color}
									onchange={(e) => updateMarkerColor(marker.id, e.target.value)}
									class="color-picker"
									disabled={marker.preset > 0}
								/>

								<select
									value={marker.effect}
									onchange={(e) => updateMarkerEffect(marker.id, parseInt(e.target.value))}
									class="effect-select"
									disabled={marker.preset > 0}
								>
									{#each effects as effect, i}
										<option value={i}>{effect}</option>
									{/each}
								</select>

								<input
									type="range"
									min="0"
									max="255"
									value={marker.brightness}
									oninput={(e) => updateMarkerBrightness(marker.id, parseInt(e.target.value))}
									class="brightness-slider"
								/>

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
		border-bottom: 1px solid #2a2a2a;
	}

	.spacer {
		flex: 1;
	}

	.btn-program-play,
	.btn-program-stop {
		padding: 0.5rem 1rem;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		font-size: 1.25rem;
		cursor: pointer;
		transition: all 0.2s;
		background-color: #1a1a1a;
	}

	.btn-program-play {
		color: #10b981;
	}

	.btn-program-play:hover {
		background-color: #2a2a2a;
		border-color: #10b981;
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

	.btn-return-start {
		padding: 0.5rem 1rem;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		font-size: 1.25rem;
		cursor: pointer;
		transition: all 0.2s;
		background-color: #1a1a1a;
		color: #9ca3af;
	}

	.btn-return-start:hover {
		background-color: #2a2a2a;
		border-color: #9ca3af;
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
		background-color: transparent;
		color: #e5e5e5;
		border: none;
		padding: 0.5rem;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.btn-collapse:hover {
		color: #a855f7;
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
	}

	.btn-delete-program:hover {
		background-color: #2a2a2a;
		border-color: #ef4444;
		transform: translateY(-1px);
	}

	.btn-delete-program:active {
		transform: translateY(0);
	}

	div[id^="waveform-"] {
		padding: 1.5rem 2rem 1.5rem 2rem;
	}

	.waveform-footer {
		padding: 0.5rem 1rem;
		background-color: #1a1a1a;
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
		border-top: 1px solid #2a2a2a;
		padding: 0.5rem 1rem;
		margin-top: 0.5rem;
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
		min-width: 140px;
		transition: border-color 0.2s;
		display: flex;
		align-items: center;
		justify-content: space-between;
		position: relative;
		white-space: nowrap;
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
		min-width: 150px;
		flex: 1;
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
