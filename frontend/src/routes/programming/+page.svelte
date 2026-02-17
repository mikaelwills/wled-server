<script lang="ts">
	import Program from '$lib/components/Program.svelte';
	import { API_URL } from '$lib/api';
	import { programs, programsLoading, programsError, resamplingProgress, setlists, activeSetlistId } from '$lib/stores/store';
	import { saveProgram, deleteProgram } from '$lib/db/programs-db';
	import { Program as ProgramModel } from '$lib/models/Program';
	import { createSetlist, activateSetlist, renameSetlist, deleteSetlist, cloneToSetlist } from '$lib/db/setlists-db';
	import { get } from 'svelte/store';

	let isDragging = $state(false);
	let isLoading = $state(false);
	let showNewSetlistInput = $state(false);
	let newSetlistName = $state('');
	let renameTimeout: ReturnType<typeof setTimeout> | null = null;
	let setlistMenuOpen = $state(false);
	let importDialogOpen = $state(false);
	let importingProgramId = $state<string | null>(null);

	let importablePrograms = $derived(
		$programs
			.filter(p => p.setlistId !== $activeSetlistId)
			.map(p => ({
				...p,
				setlistName: $setlists.find(s => s.id === p.setlistId)?.name || p.setlistId
			}))
			.sort((a, b) => a.songName.localeCompare(b.songName))
	);

	async function handleImportProgram(programId: string) {
		importingProgramId = programId;
		try {
			await cloneToSetlist(programId, $activeSetlistId);
		} finally {
			importingProgramId = null;
		}
	}

	let filteredPrograms = $derived(
		$programs
			.filter(p => p.setlistId === $activeSetlistId)
			.sort((a, b) => a.displayOrder - b.displayOrder)
	);

	let activeSetlist = $derived($setlists.find(s => s.id === $activeSetlistId));

	let activeResamplingProgress = $derived.by(() => {
		const progress = $resamplingProgress;
		return progress.backing || progress.guide || progress.click || progress.aux;
	});

	function handleDragOver(event: DragEvent) {
		event.preventDefault();
		isDragging = true;
	}

	function handleDragLeave() {
		isDragging = false;
	}

	function handleDrop(event: DragEvent) {
		event.preventDefault();
		isDragging = false;

		const files = event.dataTransfer?.files;
		if (files && files.length > 0) {
			createNewProgram(files[0]);
		}
	}

	function handleFileSelect(event: Event) {
		const input = event.target as HTMLInputElement;
		const files = input.files;
		if (files && files.length > 0) {
			createNewProgram(files[0]);
		}
		input.value = '';
	}

	async function compressAudio(file: File): Promise<string> {
		console.log('Compressing audio file to MP3...');

		try {
			// @ts-ignore
			if (!window.lamejs) {
				const script = document.createElement('script');
				script.src = '/lame.min.js';
				await new Promise((resolve, reject) => {
					script.onload = resolve;
					script.onerror = reject;
					document.head.appendChild(script);
				});
			}

			// @ts-ignore
			const Mp3Encoder = window.lamejs.Mp3Encoder;

			const arrayBuffer = await file.arrayBuffer();
			const audioContext = new AudioContext();
			const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

			const channels = audioBuffer.numberOfChannels;
			const sampleRate = audioBuffer.sampleRate;
			const samples = audioBuffer.length;

			let left, right;
			if (channels === 2) {
				left = audioBuffer.getChannelData(0);
				right = audioBuffer.getChannelData(1);
			} else {
				left = audioBuffer.getChannelData(0);
				right = left;
			}

			const leftInt16 = new Int16Array(samples);
			const rightInt16 = new Int16Array(samples);
			for (let i = 0; i < samples; i++) {
				leftInt16[i] = Math.max(-32768, Math.min(32767, left[i] * 32768));
				rightInt16[i] = Math.max(-32768, Math.min(32767, right[i] * 32768));
			}

			const mp3encoder = new Mp3Encoder(channels, sampleRate, 128);
			const mp3Data = [];

			const chunkSize = 1152;
			for (let i = 0; i < samples; i += chunkSize) {
				const leftChunk = leftInt16.subarray(i, i + chunkSize);
				const rightChunk = rightInt16.subarray(i, i + chunkSize);
				const mp3buf = mp3encoder.encodeBuffer(leftChunk, rightChunk);
				if (mp3buf.length > 0) {
					mp3Data.push(mp3buf);
				}
			}

			const mp3buf = mp3encoder.flush();
			if (mp3buf.length > 0) {
				mp3Data.push(mp3buf);
			}

			const mp3Blob = new Blob(mp3Data, { type: 'audio/mp3' });
			console.log(`Compressed: ${(file.size / 1024 / 1024).toFixed(2)}MB -> ${(mp3Blob.size / 1024 / 1024).toFixed(2)}MB (${((1 - mp3Blob.size / file.size) * 100).toFixed(1)}% reduction)`);

			const reader = new FileReader();
			return new Promise<string>((resolve, reject) => {
				reader.onloadend = () => resolve(reader.result as string);
				reader.onerror = reject;
				reader.readAsDataURL(mp3Blob);
			});
		} catch (err) {
			console.error('MP3 compression failed:', err);
			throw err;
		}
	}

	function dataURLToBlob(dataURL: string): Blob {
		const parts = dataURL.split(',');
		const mime = parts[0].match(/:(.*?);/)?.[1] || 'application/octet-stream';
		const bstr = atob(parts[1]);
		let n = bstr.length;
		const u8arr = new Uint8Array(n);
		while (n--) {
			u8arr[n] = bstr.charCodeAt(n);
		}
		return new Blob([u8arr], { type: mime });
	}

	async function importProgramFromJSON(file: File) {
		console.log('Importing program from JSON:', file.name);
		isLoading = true;

		try {
			const text = await file.text();
			const data = JSON.parse(text);

			if (!data.id || !data.audio_data) {
				throw new Error('Invalid program JSON: missing id or audio_data');
			}

			console.log('Parsed program:', data.song_name || data.id);

			const audioBlob = dataURLToBlob(data.audio_data);
			console.log('Extracted audio blob:', audioBlob.size, 'bytes');

			const audioDataURL = await new Promise<string>((resolve, reject) => {
				const reader = new FileReader();
				reader.onload = (e) => resolve(e.target?.result as string);
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

			const programData = {
				...data,
				audioId: audio_file,
				audio_data: undefined,
				setlistId: $activeSetlistId,
			};

			const program = ProgramModel.fromJson(programData);

			if (program) {
				program.setlistId = $activeSetlistId;
				await saveProgram(program);
				console.log('Program imported successfully:', program.songName);
			}
		} catch (err) {
			console.error('Failed to import program:', err);
			alert(`Failed to import program: ${err instanceof Error ? err.message : String(err)}`);
		} finally {
			isLoading = false;
		}
	}

	async function createNewProgram(file: File) {
		if (file.name.endsWith('.json')) {
			return importProgramFromJSON(file);
		}

		console.log('Creating new program with file:', file.name);

		isLoading = true;

		const timestamp = Date.now();
		const fileName = file.name;
		const baseFileName = fileName.replace(/\.[^/.]+$/, '');
		const programId = `${baseFileName}-${timestamp}`;

		try {
			let audioDataURL;

			audioDataURL = await new Promise((resolve, reject) => {
				const reader = new FileReader();
				reader.onload = (e) => resolve(e.target!.result);
				reader.onerror = reject;
				reader.readAsDataURL(file);
			});

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

			const newProgramData = {
				id: programId,
				songName: fileName.replace(/\.[^/.]+$/, ''),
				loopyProTrack: '',
				fileName: fileName,
				audioId: audio_file,
				cues: [],
				createdAt: new Date().toISOString(),
				setlistId: $activeSetlistId,
			};

			const newProgram = ProgramModel.fromJson(newProgramData);

			if (newProgram) {
				newProgram.setlistId = $activeSetlistId;
				await saveProgram(newProgram);
				console.log('Program saved with backend audio storage');
			}
		} catch (err) {
			console.error('Failed to create program:', err);
			alert('Failed to save audio file. Check console for details.');
		} finally {
			isLoading = false;
		}
	}

	async function handleSetlistChange(event: Event) {
		const select = event.target as HTMLSelectElement;
		await activateSetlist(select.value);
	}

	async function handleCreateSetlist() {
		if (!newSetlistName.trim()) return;
		await createSetlist(newSetlistName.trim());
		newSetlistName = '';
		showNewSetlistInput = false;
	}

	async function handleDeleteSetlist() {
		if (!activeSetlist || activeSetlist.id === 'default') return;
		if (!confirm(`Delete "${activeSetlist.name}"? Programs will be moved to Default Set.`)) return;
		await deleteSetlist(activeSetlist.id);
	}

</script>

<div class="sequencer-page" onclick={() => setlistMenuOpen = false}>
	<div class="setlist-bar">
		<div class="setlist-selector">
			<select value={$activeSetlistId} onchange={handleSetlistChange}>
				{#each $setlists as setlist}
					<option value={setlist.id}>{setlist.name}</option>
				{/each}
			</select>
		</div>
		{#if activeSetlist}
			<input
				class="setlist-rename-input"
				type="text"
				value={activeSetlist.name}
				oninput={(e) => {
					const val = (e.target as HTMLInputElement).value.trim();
					const id = activeSetlist!.id;
					if (renameTimeout) clearTimeout(renameTimeout);
					renameTimeout = setTimeout(() => {
						if (val && val !== activeSetlist!.name) {
							renameSetlist(id, val);
						}
					}, 250);
				}}
				onkeydown={(e) => {
					if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
				}}
				placeholder="Set name"
			/>
		{/if}
		<div class="spacer"></div>
		<div class="setlist-menu-wrapper">
			<button
				class="setlist-menu-btn"
				onclick={(e) => { e.stopPropagation(); setlistMenuOpen = !setlistMenuOpen; }}
				title="Setlist options"
			>⋯</button>
			{#if setlistMenuOpen}
				<div class="setlist-menu-dropdown">
					<button class="setlist-menu-item" onclick={() => { showNewSetlistInput = true; setlistMenuOpen = false; }}>New Set</button>
					<button class="setlist-menu-item" onclick={() => { importDialogOpen = true; setlistMenuOpen = false; }}>Import Program</button>
					{#if activeSetlist && activeSetlist.id !== 'default'}
						<button class="setlist-menu-item setlist-menu-item-danger" onclick={() => { handleDeleteSetlist(); setlistMenuOpen = false; }}>Delete Set</button>
					{/if}
				</div>
			{/if}
		</div>
		{#if showNewSetlistInput}
			<input
				class="setlist-name-input"
				type="text"
				bind:value={newSetlistName}
				onkeydown={(e) => e.key === 'Enter' && handleCreateSetlist()}
				placeholder="New set name"
			/>
			<button class="setlist-btn" onclick={handleCreateSetlist}>Create</button>
			<button class="setlist-btn" onclick={() => { showNewSetlistInput = false; newSetlistName = ''; }}>Cancel</button>
		{/if}
	</div>

	<div
		class="thin-drop-zone"
		class:dragging={isDragging}
		role="button"
		tabindex="0"
		ondragover={handleDragOver}
		ondragleave={handleDragLeave}
		ondrop={handleDrop}
		onclick={() => document.getElementById('file-input-thin')?.click()}
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

	{#if $programsLoading}
		<div class="empty-state">
			<p class="empty-text">Loading programs...</p>
		</div>
	{:else if $programsError}
		<div class="empty-state">
			<p class="empty-text" style="color: #ef4444;">{$programsError}</p>
		</div>
	{:else if filteredPrograms.length === 0 && !isLoading}
		<div class="empty-state">
			<p class="empty-text">No programs in this set</p>
			<p class="empty-hint">Drop a WAV file above to create your first program</p>
		</div>
	{:else}
		<div class="programs-container">
			{#if isLoading}
				<div class="compression-loading-card">
					{#if activeResamplingProgress?.active}
						<div class="progress-container">
							<div class="progress-bar" style="width: {(activeResamplingProgress.current / activeResamplingProgress.total) * 100}%"></div>
						</div>
						<p>Resampling audio...</p>
						<p class="compression-hint">{Math.round((activeResamplingProgress.current / activeResamplingProgress.total) * 100)}%</p>
					{:else}
						<div class="spinner"></div>
						<p>Saving program...</p>
						<p class="compression-hint">Processing audio file</p>
					{/if}
				</div>
			{/if}

			{#each filteredPrograms as program (program.id)}
				<Program program={program} />
			{/each}
		</div>
	{/if}
</div>

{#if importDialogOpen}
	<div class="modal-overlay" onclick={() => importDialogOpen = false}>
		<div class="import-modal" onclick={(e) => e.stopPropagation()}>
			<div class="import-modal-header">
				<h3>Import Program</h3>
				<button class="modal-close-btn" onclick={() => importDialogOpen = false}>&times;</button>
			</div>
			<div class="import-modal-body">
				{#if importablePrograms.length === 0}
					<p class="import-empty">No programs available to import.</p>
				{:else}
					{#each importablePrograms as prog}
						<div class="import-row">
							<div class="import-row-info">
								<span class="import-row-name">{prog.songName || 'Untitled'}</span>
								<span class="import-row-setlist">{prog.setlistName}</span>
							</div>
							<button
								class="import-row-btn"
								disabled={importingProgramId === prog.id}
								onclick={() => handleImportProgram(prog.id)}
							>{importingProgramId === prog.id ? 'Importing...' : 'Import'}</button>
						</div>
					{/each}
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.sequencer-page {
		max-width: 1400px;
		margin: 0 auto;
		padding: 2rem;
		min-height: 100vh;
	}

	.setlist-bar {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-bottom: 1rem;
		flex-wrap: wrap;
	}

	.setlist-selector select {
		background: #0c0c0c;
		color: #e5e5e5;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		padding: 0.5rem 2rem 0.5rem 1rem;
		font-size: 1rem;
		cursor: pointer;
		outline: none;
		-webkit-appearance: none;
		-moz-appearance: none;
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg width='10' height='6' viewBox='0 0 10 6' fill='none' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M1 1L5 5L9 1' stroke='%23666' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 0.75rem center;
	}

	.setlist-selector select:hover {
		border-color: #3a3a3a;
	}

	.spacer {
		flex: 1;
	}

	.setlist-rename-input {
		background: transparent;
		color: #888;
		border: 1px solid transparent;
		border-radius: 8px;
		padding: 0.5rem 1rem;
		font-size: 1rem;
		outline: none;
		width: 180px;
		transition: all 0.2s;
	}

	.setlist-rename-input:hover {
		border-color: #2a2a2a;
		color: #e5e5e5;
	}

	.setlist-rename-input:focus {
		border-color: #3a3a3a;
		color: #e5e5e5;
		background: #0c0c0c;
	}

	.setlist-name-input {
		background: #0c0c0c;
		color: #e5e5e5;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		padding: 0.5rem 1rem;
		font-size: 1rem;
		outline: none;
		width: 180px;
	}

	.setlist-name-input:focus {
		border-color: #555;
	}

	.setlist-btn {
		background: transparent;
		color: #888;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		padding: 0.5rem 1rem;
		font-size: 1rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	.setlist-btn:hover {
		color: #e5e5e5;
		border-color: #3a3a3a;
		background: #111;
	}

	.setlist-btn-danger:hover {
		color: #ef4444;
		border-color: #ef4444;
	}

	.setlist-menu-wrapper {
		position: relative;
	}

	.setlist-menu-btn {
		background: transparent;
		color: #555;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		padding: 0.5rem;
		font-size: 1.1rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 44px;
		height: 36px;
		box-sizing: border-box;
	}

	.setlist-menu-btn:hover {
		color: #888;
		border-color: #3a3a3a;
	}

	.setlist-menu-dropdown {
		position: absolute;
		top: 100%;
		right: 0;
		margin-top: 0.25rem;
		background: #111;
		border: 1px solid #2a2a2a;
		border-radius: 8px;
		overflow: hidden;
		z-index: 50;
		min-width: 150px;
	}

	.setlist-menu-item {
		display: block;
		width: 100%;
		padding: 0.6rem 1rem;
		background: transparent;
		border: none;
		color: #ccc;
		font-size: 0.9rem;
		cursor: pointer;
		text-align: left;
		transition: background 0.15s;
	}

	.setlist-menu-item:hover {
		background: #1a1a1a;
	}

	.setlist-menu-item-danger {
		color: #ef4444;
	}

	.setlist-menu-item-danger:hover {
		background: rgba(239, 68, 68, 0.1);
	}

	.programs-container {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin-top: 1rem;
	}

	.thin-drop-zone {
		border: 1px dashed rgba(255, 255, 255, 0.08);
		border-radius: 12px;
		padding: 0.75rem;
		text-align: center;
		transition: all 0.2s;
		background: #0c0c0c;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		cursor: pointer;
	}

	.thin-drop-zone.dragging {
		border-color: rgba(255, 255, 255, 0.15);
		background: #0e0e0e;
		border-style: solid;
	}

	.thin-drop-zone:hover {
		border-color: rgba(255, 255, 255, 0.1);
		background: #0e0e0e;
	}

	.drop-text {
		font-size: 1rem;
		color: #555;
		margin: 0;
		pointer-events: none;
	}

	.empty-state {
		text-align: center;
		padding: 4rem 2rem;
	}

	.empty-text {
		font-size: 1.5rem;
		color: #444;
		margin: 0 0 0.5rem 0;
	}

	.empty-hint {
		font-size: 1rem;
		color: #333;
		margin: 0;
	}

	.compression-loading-card {
		background: #0c0c0c;
		border-radius: 12px;
		border: 1px solid rgba(255, 255, 255, 0.03);
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		text-align: center;
		padding: 3rem 2rem;
		color: #888;
	}

	.spinner {
		border: 4px solid #1a1a1a;
		border-top: 4px solid #888;
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
		color: #888;
	}

	.compression-hint {
		font-size: 0.875rem !important;
		color: #555 !important;
	}

	.progress-container {
		width: 200px;
		height: 8px;
		background: #1a1a1a;
		border-radius: 4px;
		margin: 0 auto 1rem;
		overflow: hidden;
	}

	.progress-bar {
		height: 100%;
		background: linear-gradient(90deg, #8b5cf6, #a78bfa);
		border-radius: 4px;
		transition: width 0.1s ease-out;
	}

	.modal-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.import-modal {
		background: #111;
		border: 1px solid #2a2a2a;
		border-radius: 12px;
		width: 90%;
		max-width: 500px;
		max-height: 70vh;
		display: flex;
		flex-direction: column;
	}

	.import-modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem 1.25rem;
		border-bottom: 1px solid #1a1a1a;
	}

	.import-modal-header h3 {
		margin: 0;
		font-size: 1rem;
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
		color: #e5e5e5;
	}

	.import-modal-body {
		overflow-y: auto;
		padding: 0.5rem 0;
		scrollbar-width: none;
	}

	.import-modal-body::-webkit-scrollbar {
		display: none;
	}

	.import-empty {
		color: #666;
		text-align: center;
		padding: 2rem 1rem;
		margin: 0;
	}

	.import-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.6rem 1.25rem;
		transition: background 0.15s;
	}

	.import-row:hover {
		background: #1a1a1a;
	}

	.import-row-info {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}

	.import-row-name {
		color: #e5e5e5;
		font-size: 0.95rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.import-row-setlist {
		color: #555;
		font-size: 0.75rem;
	}

	.import-row-btn {
		background: transparent;
		color: #888;
		border: 1px solid #2a2a2a;
		border-radius: 6px;
		padding: 0.35rem 0.75rem;
		font-size: 0.85rem;
		cursor: pointer;
		transition: all 0.2s;
		flex-shrink: 0;
	}

	.import-row-btn:hover {
		color: #e5e5e5;
		border-color: #3a3a3a;
		background: #1a1a1a;
	}

	.import-row-btn:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
