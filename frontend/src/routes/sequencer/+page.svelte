<script>
	import { onMount } from 'svelte';
	import Program from '$lib/Program.svelte';
	import { API_URL } from '$lib/api';

	let programs = $state([]);
	let isDragging = $state(false);
	let playingProgramId = $state(null);
	let activeTimeouts = $state([]);
	let isCompressing = $state(false);

	onMount(() => {
		loadPrograms();
	});

	function loadPrograms() {
		const storedPrograms = JSON.parse(localStorage.getItem('light-programs') || '[]');
		programs = storedPrograms;
	}

	function handleDragOver(event) {
		event.preventDefault();
		isDragging = true;
	}

	function handleDragLeave() {
		isDragging = false;
	}

	function handleDrop(event) {
		event.preventDefault();
		isDragging = false;

		const files = event.dataTransfer.files;
		if (files.length > 0) {
			createNewProgram(files[0]);
		}
	}

	function handleFileSelect(event) {
		const files = event.target.files;
		if (files.length > 0) {
			createNewProgram(files[0]);
		}
	}

	async function compressAudio(file) {
		console.log('Compressing audio file...');

		try {
			// Read file as ArrayBuffer
			const arrayBuffer = await file.arrayBuffer();

			// Decode audio using Web Audio API
			const audioContext = new AudioContext();
			const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

			// Create MediaStreamSource from AudioBuffer
			const offlineContext = new OfflineAudioContext(
				audioBuffer.numberOfChannels,
				audioBuffer.length,
				audioBuffer.sampleRate
			);

			const source = offlineContext.createBufferSource();
			source.buffer = audioBuffer;
			source.connect(offlineContext.destination);
			source.start();

			// Render to get the audio data
			const renderedBuffer = await offlineContext.startRendering();

			// Create MediaStream from rendered audio
			const mediaStreamDestination = audioContext.createMediaStreamDestination();
			const mediaSource = audioContext.createBufferSource();
			mediaSource.buffer = renderedBuffer;
			mediaSource.connect(mediaStreamDestination);
			mediaSource.start();

			// Use MediaRecorder to compress to WebM/Opus
			const mediaRecorder = new MediaRecorder(mediaStreamDestination.stream, {
				mimeType: 'audio/webm;codecs=opus',
				audioBitsPerSecond: 64000 // 64kbps
			});

			const chunks = [];

			return new Promise((resolve, reject) => {
				mediaRecorder.ondataavailable = (e) => {
					if (e.data.size > 0) {
						chunks.push(e.data);
					}
				};

				mediaRecorder.onstop = async () => {
					const blob = new Blob(chunks, { type: 'audio/webm;codecs=opus' });
					console.log(`Compressed: ${(file.size / 1024 / 1024).toFixed(2)}MB -> ${(blob.size / 1024 / 1024).toFixed(2)}MB`);

					// Convert to base64
					const reader = new FileReader();
					reader.onloadend = () => {
						resolve(reader.result);
					};
					reader.onerror = reject;
					reader.readAsDataURL(blob);
				};

				mediaRecorder.onerror = reject;

				mediaRecorder.start();

				// Stop recording after the audio duration
				setTimeout(() => {
					mediaRecorder.stop();
					audioContext.close();
				}, (renderedBuffer.duration * 1000) + 100);
			});
		} catch (err) {
			console.error('Compression failed:', err);
			throw err;
		}
	}

	async function createNewProgram(file) {
		console.log('Creating new program with file:', file.name);

		const timestamp = Date.now();
		const fileName = file.name;

		try {
			isCompressing = true;

			// Compress audio file
			const compressedAudio = await compressAudio(file);

			const newProgram = {
				id: `new-program-${timestamp}`,
				songName: fileName.replace(/\.[^/.]+$/, ''), // Remove extension
				loopyProTrack: '',
				fileName: fileName,
				audioData: compressedAudio, // Compressed audio as base64
				cues: [],
				createdAt: new Date().toISOString()
			};

			const existingPrograms = JSON.parse(localStorage.getItem('light-programs') || '[]');
			existingPrograms.push(newProgram);
			localStorage.setItem('light-programs', JSON.stringify(existingPrograms));

			console.log('Program saved with compressed audio');
			programs = existingPrograms;
			isCompressing = false;
		} catch (err) {
			console.error('Failed to create program:', err);
			isCompressing = false;
			alert('Failed to compress audio file. Please try a smaller file.');
		}
	}

	function handleProgramSaved() {
		loadPrograms();
	}

	function handleProgramDeleted(programId) {
		const filtered = programs.filter(p => p.id !== programId);
		localStorage.setItem('light-programs', JSON.stringify(filtered));
		programs = filtered;
	}

	function formatDate(isoString) {
		const date = new Date(isoString);
		return date.toLocaleDateString() + ' ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	}

	async function playProgram(program) {
		console.log('playProgram called with:', {
			id: program.id,
			cueCount: program.cues?.length,
			cues: program.cues
		});

		// Stop any currently playing program
		if (playingProgramId) {
			stopPlayback();
		}

		playingProgramId = program.id;

		// Sort cues by time
		const sortedCues = [...program.cues].sort((a, b) => a.time - b.time);
		console.log('Sorted cues:', sortedCues);

		// Send OSC to start Loopy Pro track
		if (program.loopyProTrack) {
			try {
				await fetch(`${API_URL}/osc`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({
						address: `/PlayStop/${program.loopyProTrack.padStart(2, '0')}`
					})
				});
			} catch (err) {
				console.error('Failed to send OSC:', err);
			}
		}

		// Schedule all cues
		sortedCues.forEach((cue, index) => {
			const timeoutId = setTimeout(async () => {
				console.log(`Triggering cue at ${cue.time}s: ${cue.label}`);

				// Send commands to all boards in this cue
				for (const boardId of cue.boards) {
					try {
						// Set preset if specified
						if (cue.preset > 0) {
							await fetch(`${API_URL}/board/${boardId}/preset`, {
								method: 'POST',
								headers: { 'Content-Type': 'application/json' },
								body: JSON.stringify({ preset: cue.preset })
							});
						} else {
							// Set color
							const rgb = hexToRgb(cue.color);
							await fetch(`${API_URL}/board/${boardId}/color`, {
								method: 'POST',
								headers: { 'Content-Type': 'application/json' },
								body: JSON.stringify({ r: rgb.r, g: rgb.g, b: rgb.b })
							});

							// Set effect
							await fetch(`${API_URL}/board/${boardId}/effect`, {
								method: 'POST',
								headers: { 'Content-Type': 'application/json' },
								body: JSON.stringify({ effect: cue.effect })
							});
						}

						// Set brightness
						await fetch(`${API_URL}/board/${boardId}/brightness`, {
							method: 'POST',
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify({ brightness: cue.brightness })
						});
					} catch (err) {
						console.error(`Failed to send commands to board ${boardId}:`, err);
					}
				}
			}, cue.time * 1000); // Convert to milliseconds

			activeTimeouts = [...activeTimeouts, timeoutId];
		});

		// Auto-stop after last cue + 1 second
		if (sortedCues.length > 0) {
			const lastCueTime = sortedCues[sortedCues.length - 1].time;
			const stopTimeoutId = setTimeout(() => {
				playingProgramId = null;
				activeTimeouts = [];
			}, (lastCueTime + 1) * 1000);
			activeTimeouts = [...activeTimeouts, stopTimeoutId];
		}
	}

	function stopPlayback() {
		// Clear all scheduled timeouts
		activeTimeouts.forEach(timeoutId => clearTimeout(timeoutId));
		activeTimeouts = [];
		playingProgramId = null;
	}

	function hexToRgb(hex) {
		const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
		return result ? {
			r: parseInt(result[1], 16),
			g: parseInt(result[2], 16),
			b: parseInt(result[3], 16)
		} : { r: 0, g: 0, b: 0 };
	}
</script>

<div class="sequencer-page">
	<!-- Thin Drop Zone - Always Visible -->
	<div
		class="thin-drop-zone"
		class:dragging={isDragging}
		role="button"
		tabindex="0"
		ondragover={handleDragOver}
		ondragleave={handleDragLeave}
		ondrop={handleDrop}
		onclick={() => document.getElementById('file-input-thin').click()}
	>
		<p class="drop-text">Drop WAV file here or click to browse</p>
		<input
			id="file-input-thin"
			type="file"
			accept="audio/*"
			style="display: none;"
			onchange={handleFileSelect}
		/>
	</div>

	<!-- All Programs Displayed Continuously -->
	{#if programs.length === 0 && !isCompressing}
		<div class="empty-state">
			<p class="empty-text">No light programs yet</p>
			<p class="empty-hint">Drop a WAV file above to create your first program</p>
		</div>
	{:else}
		<div class="programs-container">
			<!-- Compression Loading as Program Card -->
			{#if isCompressing}
				<div class="program-wrapper">
					<div class="compression-loading-card">
						<div class="spinner"></div>
						<p>Compressing audio file...</p>
						<p class="compression-hint">This may take 10-30 seconds for large files</p>
					</div>
				</div>
			{/if}

			<!-- Existing Programs -->
			{#each programs as program (program.id)}
				<div class="program-wrapper">
					<Program
						programId={program.id}
						initialData={program}
						onsave={handleProgramSaved}
						on:delete={(e) => handleProgramDeleted(e.detail)}
						{playingProgramId}
						onstop={stopPlayback}
						onplay={() => playProgram(program)}
					/>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	:global(body) {
		background-color: #0f0f0f;
		color: #e5e5e5;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
	}

	.sequencer-page {
		max-width: 1400px;
		margin: 0 auto;
		padding: 2rem;
		min-height: 100vh;
	}

	/* Programs Container Styles */
	.programs-container {
		display: flex;
		flex-direction: column;
		gap: 2rem;
		margin-top: 2rem;
	}

	.program-wrapper {
		/* No extra border or padding - Program component has its own styling */
	}

	.thin-drop-zone {
		border: 2px dashed #4b5563;
		border-radius: 12px;
		padding: 1.5rem;
		text-align: center;
		transition: all 0.3s ease;
		background-color: #1a1a1a;
		cursor: pointer;
	}

	.thin-drop-zone.dragging {
		border-color: #a855f7;
		background-color: #2a1a3a;
		transform: scale(1.01);
	}

	.thin-drop-zone:hover {
		border-color: #a855f7;
		background-color: #1f1f1f;
	}

	.drop-text {
		font-size: 1rem;
		color: #9ca3af;
		margin: 0;
		pointer-events: none;
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

	.empty-hint {
		font-size: 1rem;
		color: #4b5563;
		margin: 0;
	}

	.compression-loading-card {
		background-color: #1a1a1a;
		border-radius: 12px;
		border: 1px solid #2a2a2a;
		text-align: center;
		padding: 3rem 2rem;
		color: #e5e5e5;
	}

	.spinner {
		border: 4px solid #2a2a2a;
		border-top: 4px solid #a855f7;
		border-radius: 50%;
		width: 50px;
		height: 50px;
		animation: spin 1s linear infinite;
		margin: 0 auto 1rem;
	}

	@keyframes spin {
		0% { transform: rotate(0deg); }
		100% { transform: rotate(360deg); }
	}

	.compression-loading-card p {
		margin: 0.5rem 0;
		font-size: 1rem;
		color: #e5e5e5;
	}

	.compression-hint {
		font-size: 0.875rem !important;
		color: #9ca3af !important;
	}
</style>
