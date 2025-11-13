<script>
	import Program from '$lib/Program.svelte';
	import { API_URL } from '$lib/api';
	import { programs, programsLoading, programsError } from '$lib/store';
	import { saveProgram, deleteProgram } from '$lib/programs-db';
	import { Program as ProgramModel } from '$lib/models/Program';

	let isDragging = $state(false);
	let isLoading = $state(false);

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
		console.log('Compressing audio file to MP3...');

		try {
			// Load lamejs browser bundle if not already loaded
			// @ts-ignore - lamejs is loaded globally
			if (!window.lamejs) {
				const script = document.createElement('script');
				script.src = '/lame.min.js';
				await new Promise((resolve, reject) => {
					script.onload = resolve;
					script.onerror = reject;
					document.head.appendChild(script);
				});
			}

			// @ts-ignore - lamejs is loaded globally
			const Mp3Encoder = window.lamejs.Mp3Encoder;

			// Read file as ArrayBuffer
			const arrayBuffer = await file.arrayBuffer();

			// Decode audio using Web Audio API
			const audioContext = new AudioContext();
			const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

			// Get audio data as PCM samples
			const channels = audioBuffer.numberOfChannels;
			const sampleRate = audioBuffer.sampleRate;
			const samples = audioBuffer.length;

			// Convert to mono for MP3 encoding (more efficient)
			let left, right;
			if (channels === 2) {
				left = audioBuffer.getChannelData(0);
				right = audioBuffer.getChannelData(1);
			} else {
				left = audioBuffer.getChannelData(0);
				right = left; // Duplicate for mono
			}

			// Convert Float32Array to Int16Array for lamejs
			const leftInt16 = new Int16Array(samples);
			const rightInt16 = new Int16Array(samples);
			for (let i = 0; i < samples; i++) {
				leftInt16[i] = Math.max(-32768, Math.min(32767, left[i] * 32768));
				rightInt16[i] = Math.max(-32768, Math.min(32767, right[i] * 32768));
			}

			// Create MP3 encoder (128kbps for balance of quality and performance)
			const mp3encoder = new Mp3Encoder(channels, sampleRate, 128);
			const mp3Data = [];

			// Encode in chunks (1152 samples per chunk for MP3)
			const chunkSize = 1152;
			for (let i = 0; i < samples; i += chunkSize) {
				const leftChunk = leftInt16.subarray(i, i + chunkSize);
				const rightChunk = rightInt16.subarray(i, i + chunkSize);
				const mp3buf = mp3encoder.encodeBuffer(leftChunk, rightChunk);
				if (mp3buf.length > 0) {
					mp3Data.push(mp3buf);
				}
			}

			// Finish encoding
			const mp3buf = mp3encoder.flush();
			if (mp3buf.length > 0) {
				mp3Data.push(mp3buf);
			}

			// Create MP3 Blob
			const mp3Blob = new Blob(mp3Data, { type: 'audio/mp3' });
			console.log(`Compressed: ${(file.size / 1024 / 1024).toFixed(2)}MB -> ${(mp3Blob.size / 1024 / 1024).toFixed(2)}MB (${((1 - mp3Blob.size / file.size) * 100).toFixed(1)}% reduction)`);

			// Convert to base64 data URL
			const reader = new FileReader();
			return new Promise((resolve, reject) => {
				reader.onloadend = () => resolve(reader.result);
				reader.onerror = reject;
				reader.readAsDataURL(mp3Blob);
			});
		} catch (err) {
			console.error('MP3 compression failed:', err);
			throw err;
		}
	}

	/**
	 * Convert base64 data URL to Blob
	 */
	function dataURLToBlob(dataURL) {
		const parts = dataURL.split(',');
		const mime = parts[0].match(/:(.*?);/)[1];
		const bstr = atob(parts[1]);
		let n = bstr.length;
		const u8arr = new Uint8Array(n);
		while (n--) {
			u8arr[n] = bstr.charCodeAt(n);
		}
		return new Blob([u8arr], { type: mime });
	}

	/**
	 * Import program from downloaded JSON file (with embedded audio)
	 */
	async function importProgramFromJSON(file) {
		console.log('Importing program from JSON:', file.name);
		isLoading = true;

		try {
			// Read JSON file
			const text = await file.text();
			const data = JSON.parse(text);

			// Validate JSON has required fields
			if (!data.id || !data.audio_data) {
				throw new Error('Invalid program JSON: missing id or audio_data');
			}

			console.log('Parsed program:', data.song_name || data.id);

			// Extract embedded audio
			const audioBlob = dataURLToBlob(data.audio_data);
			console.log('Extracted audio blob:', audioBlob.size, 'bytes');

			// Upload audio to backend
			const audioDataURL = await new Promise((resolve, reject) => {
				const reader = new FileReader();
				reader.onload = (e) => resolve(e.target.result);
				reader.onerror = reject;
				reader.readAsDataURL(audioBlob);
			});

			const uploadResponse = await fetch(`${API_URL}/audio/${data.id}`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ data_url: audioDataURL })
			});

			if (!uploadResponse.ok) {
				throw new Error(`Failed to upload audio: ${uploadResponse.statusText}`);
			}

			const { audio_file } = await uploadResponse.json();
			console.log('Audio uploaded:', audio_file);

			// Create Program with audioId reference (remove embedded audio_data)
			const programData = {
				...data,
				audioId: audio_file,
				audio_data: undefined // Remove embedded audio
			};

			const program = ProgramModel.fromJson(programData);

			if (program) {
				await saveProgram(program);
				console.log('Program imported successfully:', program.songName);
			}
		} catch (err) {
			console.error('Failed to import program:', err);
			alert(`Failed to import program: ${err.message}`);
		} finally {
			isLoading = false;
		}
	}

	async function createNewProgram(file) {
		// Detect file type and route appropriately
		if (file.name.endsWith('.json')) {
			return importProgramFromJSON(file);
		}

		console.log('Creating new program with file:', file.name);

		isLoading = true; // Set to true immediately to show loading card

		const timestamp = Date.now();
		const fileName = file.name;
		const baseFileName = fileName.replace(/\.[^/.]+$/, ''); // Remove extension
		const programId = `${baseFileName}-${timestamp}`;

		try {
			let audioDataURL;

			// --- To use MP3 compression, comment out the "Raw Audio" block and uncomment the "Compressed Audio" line. ---

			// Option 1: Raw Audio (default)
			// audioDataURL = await new Promise((resolve, reject) => {
			// 	const reader = new FileReader();
			// 	reader.onload = (e) => resolve(e.target.result);
			// 	reader.onerror = reject;
			// 	reader.readAsDataURL(file);
			// });

			// Option 2: Compressed Audio
			audioDataURL = await compressAudio(file);


			// Upload audio to backend API
			console.log('Uploading audio to backend...');
			const uploadResponse = await fetch(`${API_URL}/audio/${programId}`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ data_url: audioDataURL })
			});

			if (!uploadResponse.ok) {
				throw new Error(`Failed to upload audio: ${uploadResponse.statusText}`);
			}

			const { audio_file } = await uploadResponse.json();
			console.log('Audio uploaded:', audio_file);

			// Create Program using factory method (with audioId reference)
			const newProgramData = {
				id: programId,
				songName: fileName.replace(/\.[^/.]+$/, ''), // Remove extension
				loopyProTrack: '',
				fileName: fileName,
				audioId: audio_file, // Reference to audio file on backend
				cues: [],
				createdAt: new Date().toISOString()
			};

			const newProgram = ProgramModel.fromJson(newProgramData);

			if (newProgram) {
				// Save program (without embedded audio) through service layer
				await saveProgram(newProgram);
				console.log('Program saved with backend audio storage');
			}
		} catch (err) {
			console.error('Failed to create program:', err);
			alert('Failed to save audio file. Check console for details.');
		} finally {
			isLoading = false; // Ensure it's reset after process completes or errors
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
		<p class="drop-text">Drop WAV file or JSON program here or click to browse</p>
		<input
			id="file-input-thin"
			type="file"
			accept="audio/*,.json"
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
	{:else if $programs.length === 0 && !isLoading}
		<div class="empty-state">
			<p class="empty-text">No light programs yet</p>
			<p class="empty-hint">Drop a WAV file above to create your first program</p>
		</div>
	{:else}
		<div class="programs-container">
			<!-- Loading Card at Top (new programs appear here) -->
			{#if isLoading}
				<div class="compression-loading-card">
					<div class="spinner"></div>
					<p>Saving program...</p>
					<p class="compression-hint">Processing audio file</p>
				</div>
			{/if}

			<!-- Programs (newest first) -->
			{#each $programs as program (program.id)}
				<Program program={program} />
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
