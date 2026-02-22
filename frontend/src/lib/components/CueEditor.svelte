<script lang="ts">
	import { boards, performancePresets } from '$lib/stores/store';
	import type { Cue } from '$lib/models/Cue';

	let {
		regionId,
		cue,
		onToggleBoardSelection,
		onOpenPresetPicker,
		onUpdateSyncRate,
		onDelete
	}: {
		regionId: string;
		cue: Cue;
		onToggleBoardSelection: (regionId: string, boardId: string) => void;
		onOpenPresetPicker: (regionId: string) => void;
		onUpdateSyncRate: (regionId: string, rate: number) => void;
		onDelete: (regionId: string) => void;
	} = $props();

	let dropdownOpen = $state(false);

	function formatTime(seconds: number) {
		const mins = Math.floor(seconds / 60);
		const secs = (seconds % 60).toFixed(2);
		return `${mins}:${secs.padStart(5, '0')}`;
	}

	function getBoardsLabel(selectedBoards: string[]) {
		if (selectedBoards.length === 0) return 'Select boards...';
		if (selectedBoards.length === 1) {
			const board = $boards.find(b => b.id === selectedBoards[0]);
			return board ? board.id : '1 selected';
		}
		return `${selectedBoards.length} selected`;
	}

	function toggleDropdown(e: MouseEvent) {
		e.stopPropagation();
		dropdownOpen = !dropdownOpen;
	}

	function handleClickOutside(e: MouseEvent) {
		if (!(e.target as Element).closest('.boards-dropdown-wrapper')) {
			dropdownOpen = false;
		}
	}

	$effect(() => {
		document.addEventListener('click', handleClickOutside);
		return () => document.removeEventListener('click', handleClickOutside);
	});
</script>

<div class="cue-editor">
	<div class="cue-info">
		<span class="cue-time">{formatTime(cue.time)}</span>
		<span class="cue-label">{cue.label}</span>
	</div>
	<div class="cue-controls">
		<div class="boards-dropdown-wrapper">
			<button
				class="boards-select-button"
				onclick={toggleDropdown}
			>
				{getBoardsLabel(cue.boards)}
				<span class="dropdown-arrow">▼</span>
			</button>
			{#if dropdownOpen}
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
										checked={cue.boards.includes(group.id)}
										onchange={() => onToggleBoardSelection(regionId, group.id)}
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
										checked={cue.boards.includes(board.id)}
										onchange={() => onToggleBoardSelection(regionId, board.id)}
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
			class:broken-preset={cue.presetName && !$performancePresets.some(p => p.name === cue.presetName)}
			onclick={() => onOpenPresetPicker(regionId)}
		>
			{cue.presetName || 'Select Preset'}
			<span class="dropdown-arrow">▼</span>
		</button>
		<div class="sync-rate-group" title="BPM sync rate">
			<button
				class="sync-rate-btn"
				class:active={cue.syncRate === 0.25}
				onclick={() => onUpdateSyncRate(regionId, 0.25)}
			>¼</button>
			<button
				class="sync-rate-btn"
				class:active={cue.syncRate === 0.5}
				onclick={() => onUpdateSyncRate(regionId, 0.5)}
			>½</button>
			<button
				class="sync-rate-btn"
				class:active={(cue.syncRate ?? 1) === 1}
				onclick={() => onUpdateSyncRate(regionId, 1)}
			>1</button>
			<button
				class="sync-rate-btn"
				class:active={cue.syncRate === 2}
				onclick={() => onUpdateSyncRate(regionId, 2)}
			>2</button>
			<button
				class="sync-rate-btn"
				class:active={cue.syncRate === 4}
				onclick={() => onUpdateSyncRate(regionId, 4)}
			>4</button>
		</div>

		<button class="btn-delete" onclick={() => onDelete(regionId)}>
			✕
		</button>
	</div>
</div>

<style>
	.cue-editor {
		padding: 0.5rem 1rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
		overflow: visible;
	}

	.cue-info {
		display: flex;
		gap: 1rem;
		align-items: center;
		flex: 1;
	}

	.cue-controls {
		display: flex;
		gap: 0.75rem;
		align-items: center;
		overflow: visible;
	}

	.cue-time {
		font-family: 'Courier New', monospace;
		font-size: 1.1rem;
		color: #888;
		font-weight: bold;
		min-width: 80px;
	}

	.cue-label {
		color: #888;
		font-size: 1rem;
		min-width: 120px;
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
</style>
