<script lang="ts">
	import { onMount } from 'svelte';
	import { performancePresets } from '$lib/store';
	import { fetchPerformancePresets } from '$lib/boards-db';

	let collapsedGroups: Set<string> = new Set();
	let allCollapsed = false;

	$: groupedPresets = groupPresetsByType($performancePresets);

	function groupPresetsByType(presetList: typeof $performancePresets) {
		const groups: Record<string, typeof $performancePresets> = {};

		for (const preset of presetList) {
			const groupName = preset.effect_type.charAt(0).toUpperCase() + preset.effect_type.slice(1);

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

	function rgbToHex(color: [number, number, number]): string {
		return `#${color.map(c => c.toString(16).padStart(2, '0')).join('')}`;
	}

	onMount(async () => {
		await fetchPerformancePresets();
	});
</script>

<div class="presets-page">
	<div class="header">
		<h1>Performance Presets</h1>
		<div class="header-right">
			<button class="toggle-all-btn" onclick={toggleAll}>
				{allCollapsed ? 'Expand All' : 'Collapse All'}
			</button>
			<div class="count">{$performancePresets.length} presets</div>
		</div>
	</div>

	{#if $performancePresets.length === 0}
		<div class="empty-state">
			<p>No performance presets found</p>
			<p class="hint">Add effect_presets to boards.toml</p>
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
								<div class="col-color">
									<span class="color-swatch" style="background-color: {rgbToHex(preset.color)}"></span>
								</div>
								<div class="col-name">{preset.name}</div>
								<div class="col-type">{preset.effect_type}</div>
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
		grid-template-columns: 40px 1fr 100px;
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

	.col-color {
		display: flex;
		align-items: center;
	}

	.color-swatch {
		width: 16px;
		height: 16px;
		border-radius: 4px;
		border: 1px solid rgba(56, 89, 138, 0.2);
	}

	.col-name {
		color: #888;
		font-size: 0.95rem;
	}

	.col-type {
		color: #444;
		font-size: 0.85rem;
		text-align: right;
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
			grid-template-columns: 32px 1fr 80px;
			padding: 0.5rem 0.75rem;
		}
	}
</style>
