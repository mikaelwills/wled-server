<script>
	import { onMount } from 'svelte';
	import Program from '$lib/Program.svelte';
	import { API_URL } from '$lib/api';
	import { programs, programsLoading, programsError } from '$lib/store';
	import { initPrograms, saveProgram, deleteProgram } from '$lib/programs-db';
	import { Program as ProgramModel } from '$lib/models/Program';

	let isDragging = $state(false);
	let isCompressing = $state(false);

	onMount(async () => {
		// Initialize programs from API (page-specific data)
		// Note: boards and presets are initialized in +layout.svelte and shared across all pages
		await initPrograms();
	});

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

			// Convert audio file to base64 data URL (no compression)
			const reader = new FileReader();
			const audioDataURL = await new Promise((resolve, reject) => {
				reader.onloadend = () => resolve(reader.result);
				reader.onerror = reject;
				reader.readAsDataURL(file);
			});

			// Create Program using factory method
			const newProgramData = {
				id: `new-program-${timestamp}`,
				songName: fileName.replace(/\.[^/.]+$/, ''), // Remove extension
				loopyProTrack: '',
				fileName: fileName,
				audioData: audioDataURL, // Uncompressed audio as base64 data URL
				cues: [],
				createdAt: new Date().toISOString()
			};

			const newProgram = ProgramModel.fromJson(newProgramData);

			if (newProgram) {
				// Save through service layer
				saveProgram(newProgram);
				console.log('Program saved with uncompressed audio');
			}

			isCompressing = false;
		} catch (err) {
			console.error('Failed to create program:', err);
			isCompressing = false;
			alert('Failed to save audio file.');
		}
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
	{#if $programsLoading}
		<div class="empty-state">
			<p class="empty-text">Loading programs...</p>
		</div>
	{:else if $programsError}
		<div class="empty-state">
			<p class="empty-text" style="color: #ef4444;">{$programsError}</p>
		</div>
	{:else if $programs.length === 0 && !isCompressing}
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
			{#each $programs as program (program.id)}
				<div class="program-wrapper">
					<Program
						programId={program.id}
						initialData={program}
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
