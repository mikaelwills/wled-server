<script lang="ts">
	import { performancePresets, patternPresets } from '$lib/store';

	interface PresetPickerState {
		open: boolean;
		markerId: string | null;
		step: 'category' | 'color';
		selectedCategory: string | null;
	}

	let {
		open = $bindable(false),
		markerId = $bindable<string | null>(null),
		onSelect
	}: {
		open: boolean;
		markerId: string | null;
		onSelect: (presetName: string) => void;
	} = $props();

	let step = $state<'category' | 'color'>('category');
	let selectedCategory = $state<string | null>(null);

	const quickPresets = ['Off', 'Flash'];

	const allCategories = $derived(() => {
		const categoryMap = new Map<string, boolean>();
		$performancePresets.forEach(p => {
			if (quickPresets.includes(p.name)) return;
			const parts = p.name.split(' ');
			if (parts.length >= 2) {
				const category = parts.slice(0, -1).join(' ');
				categoryMap.set(category, false);
			}
		});
		$patternPresets.forEach(p => {
			const parts = p.name.split(' ');
			if (parts.length >= 2) {
				const category = parts.slice(0, -1).join(' ');
				categoryMap.set(category, true);
			}
		});
		return Array.from(categoryMap.entries())
			.map(([name, isPattern]) => ({ name, isPattern }))
			.sort((a, b) => a.name.localeCompare(b.name));
	});

	const categoryColors = $derived(() => {
		if (!selectedCategory) return [];
		const categoryInfo = allCategories().find(c => c.name === selectedCategory);
		const isPattern = categoryInfo?.isPattern ?? false;
		const presets = isPattern ? $patternPresets : $performancePresets;
		return presets
			.filter(p => p.name.startsWith(selectedCategory + ' '))
			.map(p => ({
				name: p.name,
				color: p.name.split(' ').pop() as string,
				isPattern
			}));
	});

	function selectCategory(category: string) {
		selectedCategory = category;
		step = 'color';
	}

	function selectPreset(presetName: string) {
		onSelect(presetName);
		close();
	}

	function close() {
		open = false;
		markerId = null;
		step = 'category';
		selectedCategory = null;
	}

	function goBack() {
		step = 'category';
		selectedCategory = null;
	}

	export function getColorStyle(colorName: string): string {
		const colorMap: Record<string, string> = {
			'Red': '#ef4444',
			'Orange': '#f97316',
			'Yellow': '#eab308',
			'Green': '#22c55e',
			'Cyan': '#06b6d4',
			'Blue': '#3b82f6',
			'Purple': '#a855f7',
			'Pink': '#ec4899',
			'White': '#ffffff',
			'Warm': '#fbbf24',
			'Cool': '#93c5fd'
		};
		return colorMap[colorName] || '#e5e5e5';
	}
</script>

{#if open}
	<div class="preset-picker-overlay" onclick={close}>
		<div class="preset-picker-modal" onclick={(e) => e.stopPropagation()}>
			{#if step === 'category'}
				<div class="preset-picker-header">
					<h3>Select Effect Type</h3>
					<button class="preset-picker-close" onclick={close}>✕</button>
				</div>
				<div class="preset-picker-quick">
					{#each quickPresets as preset}
						<button
							class="preset-quick-btn"
							onclick={() => selectPreset(preset)}
						>
							{preset}
						</button>
					{/each}
				</div>
				<div class="preset-picker-grid">
					{#each allCategories() as category}
						<button
							class="preset-category-btn"
							onclick={() => selectCategory(category.name)}
						>
							{category.name}
						</button>
					{/each}
				</div>
			{:else}
				<div class="preset-picker-header">
					<button class="preset-picker-back" onclick={goBack}>
						←
					</button>
					<h3>{selectedCategory}</h3>
					<button class="preset-picker-close" onclick={close}>✕</button>
				</div>
				<div class="preset-picker-grid colors">
					{#each categoryColors() as preset}
						<button
							class="preset-color-btn"
							style="color: {getColorStyle(preset.color)}"
							onclick={() => selectPreset(preset.name)}
						>
							{preset.color}
						</button>
					{/each}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.preset-picker-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background-color: rgba(0, 0, 0, 0.8);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 10000;
	}

	.preset-picker-modal {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.03);
		border-radius: 12px;
		box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
		min-width: 320px;
		max-width: 400px;
		max-height: 80vh;
		overflow: hidden;
	}

	.preset-picker-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem;
		background: transparent;
	}

	.preset-picker-header h3 {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		color: #fff;
		flex: 1;
		text-align: center;
	}

	.preset-picker-back {
		background: transparent;
		border: none;
		color: #888;
		font-size: 0.875rem;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		transition: all 0.2s;
	}

	.preset-picker-back:hover {
		background-color: #1a1a1a;
		color: #fff;
	}

	.preset-picker-close {
		background: transparent;
		border: none;
		color: #444;
		font-size: 1rem;
		cursor: pointer;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		transition: all 0.2s;
	}

	.preset-picker-close:hover {
		background-color: #1a1212;
		color: #c44;
	}

	.preset-picker-quick {
		display: flex;
		gap: 0.75rem;
		padding: 1rem 1rem 0 1rem;
	}

	.preset-quick-btn {
		flex: 1;
		background-color: transparent;
		border: 1px solid rgba(255, 255, 255, 0.03);
		color: #888;
		padding: 0.75rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.2s;
		text-align: center;
	}

	.preset-quick-btn:hover {
		background-color: rgba(255, 255, 255, 0.02);
		border-color: rgba(255, 255, 255, 0.05);
		color: #fff;
	}

	.preset-picker-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.75rem;
		padding: 1rem;
		max-height: 60vh;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
	}

	.preset-picker-grid::-webkit-scrollbar {
		display: none;
	}

	.preset-picker-grid.colors {
		grid-template-columns: repeat(3, 1fr);
	}

	.preset-category-btn {
		background-color: transparent;
		border: 1px solid rgba(255, 255, 255, 0.03);
		color: #888;
		padding: 1rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		text-align: center;
	}

	.preset-category-btn:hover {
		background-color: rgba(255, 255, 255, 0.02);
		border-color: rgba(255, 255, 255, 0.05);
		color: #fff;
	}

	.preset-color-btn {
		background-color: transparent;
		border: 1px solid rgba(255, 255, 255, 0.03);
		padding: 0.75rem;
		border-radius: 8px;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
		text-align: center;
	}

	.preset-color-btn:hover {
		background-color: rgba(255, 255, 255, 0.02);
		border-color: rgba(255, 255, 255, 0.05);
	}
</style>
