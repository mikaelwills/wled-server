<script lang="ts">
	import { onMount } from 'svelte';
	import { presets } from '$lib/store';
	import { fetchPresets } from '$lib/boards-db';
	import { API_URL } from '$lib/api';

	let deleting: number | null = null;
	let confirmDelete: number | null = null;
	let collapsedGroups: Set<string> = new Set();
	let allCollapsed = false;

	$: groupedPresets = groupPresetsByType($presets);

	function groupPresetsByType(presetList: typeof $presets) {
		const groups: Record<string, typeof $presets> = {};

		for (const preset of presetList) {
			const parts = preset.name.split(' ');
			const groupName = parts.length > 1 ? parts[0] : 'Other';

			if (!groups[groupName]) {
				groups[groupName] = [];
			}
			groups[groupName].push(preset);
		}

		for (const group in groups) {
			groups[group].sort((a, b) => a.name.localeCompare(b.name));
		}

		return groups;
	}

	function toggleGroup(groupName: string) {
		if (collapsedGroups.has(groupName)) {
			collapsedGroups.delete(groupName);
		} else {
			collapsedGroups.add(groupName);
		}
		collapsedGroups = collapsedGroups;
	}

	function toggleAll() {
		const groupNames = Object.keys(groupedPresets);
		if (allCollapsed) {
			collapsedGroups = new Set();
		} else {
			collapsedGroups = new Set(groupNames);
		}
		allCollapsed = !allCollapsed;
	}

	onMount(async () => {
		await fetchPresets();
	});

	async function handleDeletePreset(presetId: number, presetName: string) {
		if (confirmDelete !== presetId) {
			confirmDelete = presetId;
			return;
		}

		deleting = presetId;

		try {
			const preset = $presets.find(p => p.id === presetId);
			if (!preset) {
				throw new Error('Preset not found');
			}

			const response = await fetch(`${API_URL}/presets/${presetId}`, {
				method: 'DELETE'
			});

			if (!response.ok) {
				throw new Error(`Failed to delete preset: ${response.statusText}`);
			}

			console.log(`✓ Deleted preset "${presetName}" from master file`);

			await fetchPresets();

			confirmDelete = null;
		} catch (err) {
			console.error('Failed to delete preset:', err);
			alert(`Failed to delete preset: ${err instanceof Error ? err.message : 'Unknown error'}`);
		} finally {
			deleting = null;
		}
	}
</script>

<div class="presets-page">
	<div class="header">
		<h1>WLED Presets</h1>
		<div class="header-right">
			<button class="toggle-all-btn" onclick={toggleAll}>
				{allCollapsed ? 'Expand All' : 'Collapse All'}
			</button>
			<div class="count">{$presets.length} / 250 slots</div>
		</div>
	</div>

	{#if $presets.length === 0}
		<div class="empty-state">
			<p>No presets found</p>
			<p class="hint">Create presets from the Boards page</p>
		</div>
	{:else}
		{#each Object.entries(groupedPresets).sort((a, b) => a[0].localeCompare(b[0])) as [groupName, groupPresets]}
			<div class="preset-group">
				<button class="group-header" onclick={() => toggleGroup(groupName)}>
					<span class="chevron" class:collapsed={collapsedGroups.has(groupName)}>▼</span>
					<span class="group-name">{groupName}</span>
					<span class="group-count">{groupPresets.length}</span>
				</button>
				{#if !collapsedGroups.has(groupName)}
					<div class="group-content">
						{#each groupPresets as preset}
							<div class="table-row">
								<div class="col-slot">{preset.id}</div>
								<div class="col-name">{preset.name}</div>
								<div class="col-actions">
									<button
										class="delete-btn"
										class:confirm={confirmDelete === preset.id}
										disabled={deleting === preset.id}
										onclick={() => handleDeletePreset(preset.id, preset.name)}
									>
										{#if deleting === preset.id}
											...
										{:else if confirmDelete === preset.id}
											Confirm?
										{:else}
											Delete
										{/if}
									</button>
								</div>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/each}
	{/if}
</div>

<style>
	.presets-page {
		padding: 1.5rem;
		max-width: 1000px;
		margin: 0 auto;
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1.5rem;
	}

	.header-right {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.toggle-all-btn {
		padding: 0.4rem 0.8rem;
		background: transparent;
		color: #888;
		border: 1px solid rgba(56, 89, 138, 0.2);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
		transition: all 0.2s;
	}

	.toggle-all-btn:hover {
		background: rgba(56, 89, 138, 0.1);
		color: #fff;
		border-color: rgba(56, 89, 138, 0.3);
	}

	h1 {
		color: #fff;
		font-size: 1.5rem;
		margin: 0;
		font-weight: 600;
	}

	.count {
		color: #444;
		font-size: 0.9rem;
	}

	.preset-group {
		background: linear-gradient(145deg, #0d1117 0%, #0b0d14 50%, #080a12 100%);
		border: 1px solid rgba(56, 89, 138, 0.15);
		border-radius: 8px;
		margin-bottom: 0.5rem;
		overflow: hidden;
	}

	.group-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		padding: 0.75rem 1rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		transition: background 0.2s;
	}

	.group-header:hover {
		background: rgba(56, 89, 138, 0.08);
	}

	.chevron {
		color: rgba(56, 89, 138, 0.5);
		font-size: 0.75rem;
		transition: transform 0.2s;
	}

	.chevron.collapsed {
		transform: rotate(-90deg);
	}

	.group-name {
		color: #999;
		font-size: 1rem;
		font-weight: 500;
	}

	.group-count {
		color: #333;
		font-size: 0.85rem;
		margin-left: auto;
	}

	.group-content {
		border-top: 1px solid rgba(56, 89, 138, 0.1);
	}

	.table-row {
		display: grid;
		grid-template-columns: 80px 1fr 120px;
		padding: 0.6rem 1rem;
		border-bottom: 1px solid rgba(56, 89, 138, 0.05);
		align-items: center;
		transition: background 0.2s;
	}

	.table-row:last-child {
		border-bottom: none;
	}

	.table-row:hover {
		background: rgba(56, 89, 138, 0.05);
	}

	.col-slot {
		color: #444;
		font-size: 0.9rem;
	}

	.col-name {
		color: #888;
		font-size: 0.95rem;
	}

	.col-actions {
		display: flex;
		justify-content: flex-end;
	}

	.delete-btn {
		padding: 0.4rem 0.8rem;
		background: transparent;
		color: #555;
		border: 1px solid rgba(56, 89, 138, 0.15);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.85rem;
		transition: all 0.2s;
		min-width: 70px;
	}

	.delete-btn:hover:not(:disabled) {
		background: rgba(180, 60, 60, 0.1);
		color: #c44;
		border-color: rgba(180, 60, 60, 0.3);
	}

	.delete-btn.confirm {
		background: rgba(180, 60, 60, 0.15);
		color: #e55;
		border-color: rgba(180, 60, 60, 0.4);
	}

	.delete-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.empty-state {
		text-align: center;
		padding: 3rem 2rem;
		color: #444;
	}

	.empty-state p {
		margin: 0.5rem 0;
	}

	.hint {
		font-size: 0.9rem;
		color: #333;
	}

	@media (max-width: 768px) {
		.presets-page {
			padding: 1rem;
		}

		.header {
			flex-direction: column;
			align-items: flex-start;
			gap: 0.75rem;
		}

		.header-right {
			width: 100%;
			justify-content: space-between;
		}

		.table-row {
			grid-template-columns: 60px 1fr 80px;
			padding: 0.5rem 0.75rem;
		}

		.delete-btn {
			font-size: 0.8rem;
			min-width: 60px;
			padding: 0.35rem 0.6rem;
		}
	}
</style>
