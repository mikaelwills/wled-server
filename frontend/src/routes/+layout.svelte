<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import favicon from '$lib/assets/favicon.svg';
	import { initBoardsListener, cleanupBoardsListener, fetchPresets, fetchPerformancePresets, fetchPatternPresets } from '$lib/boards-db';
	import { initPrograms, cleanupPrograms } from '$lib/programs-db';
	import { initLoopyProSettings } from '$lib/loopy-db';
	import { initAudio, cleanupAudio } from '$lib/audio-db';
	import TimingMonitor from '$lib/TimingMonitor.svelte';
	import { toggleTimingMonitor } from '$lib/timing-store';
	import AudioHealthMonitor from '$lib/AudioHealthMonitor.svelte';
	import { toggleAudioHealthMonitor } from '$lib/audio-health-store';
	import { onResamplingProgress, type ResamplingProgress } from '$lib/sse';
	import { API_URL } from '$lib/api';

	let { children } = $props();

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 't' && e.ctrlKey) {
			e.preventDefault();
			toggleTimingMonitor();
		}
		if (e.key === 'a' && e.metaKey && e.shiftKey) {
			e.preventDefault();
			toggleAudioHealthMonitor();
		}
	}

	// Mobile menu state
	let mobileMenuOpen = $state(false);

	// Memory stats
	interface MemoryStats {
		track_count: number;
		memory_bytes: number;
		memory_mb: number;
	}
	let memoryStats = $state<MemoryStats | null>(null);
	let memoryPollInterval: ReturnType<typeof setInterval> | null = null;

	// Resampling progress (via SSE)
	let resamplingStatus = $state<ResamplingProgress | null>(null);
	let unsubscribeResampling: (() => void) | null = null;

	async function fetchMemoryStats() {
		try {
			const res = await fetch(`${API_URL}/audio/engine/memory`);
			if (res.ok) {
				memoryStats = await res.json();
			}
		} catch {
			memoryStats = null;
		}
	}

	// Toggle mobile menu
	function toggleMobileMenu() {
		mobileMenuOpen = !mobileMenuOpen;
	}

	// Close mobile menu when route changes
	$effect(() => {
		$page.url.pathname;
		mobileMenuOpen = false;
	});

	// Initialize data listeners on mount
	onMount(async () => {
		if (browser) {
			// Initialize boards SSE listener (waits for initial fetch to avoid race condition)
			await initBoardsListener();

			// Fetch presets from server (home use + performance + patterns)
			await fetchPresets();
			await fetchPerformancePresets();
			await fetchPatternPresets();

			// Initialize programs from API
			await initPrograms();

			// Initialize Loopy Pro settings
			await initLoopyProSettings();

			// Initialize audio (always loads - mute only affects playback, not loading)
			// Must run after programs are loaded
			await initAudio();

			// Subscribe to resampling progress via SSE
			unsubscribeResampling = onResamplingProgress((progress) => {
				resamplingStatus = progress.active ? progress : null;
			});

			// Start memory stats polling (only memory, resampling comes via SSE)
			await fetchMemoryStats();
			memoryPollInterval = setInterval(() => {
				fetchMemoryStats();
			}, 1000);
		}
	});

	// Cleanup listeners on destroy
	onDestroy(() => {
		if (browser) {
			cleanupBoardsListener();
			cleanupPrograms();
			cleanupAudio();
			if (memoryPollInterval) {
				clearInterval(memoryPollInterval);
			}
			if (unsubscribeResampling) {
				unsubscribeResampling();
			}
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<svelte:window onkeydown={handleKeydown} />

<TimingMonitor />
<AudioHealthMonitor />

<div class="app">
	<nav class="nav">
		<button class="hamburger" onclick={toggleMobileMenu} aria-label="Toggle menu">
			<span class="hamburger-line"></span>
			<span class="hamburger-line"></span>
			<span class="hamburger-line"></span>
		</button>

		<div class="nav-links" class:open={mobileMenuOpen}>
			<a href="/" class:active={$page.url.pathname === '/'}>Boards</a>
			<a href="/presets" class:active={$page.url.pathname === '/presets'}>Presets</a>
			<a href="/programming" class:active={$page.url.pathname === '/programming'}>Programming</a>
			<a href="/performance" class:active={$page.url.pathname === '/performance'}>Performance</a>
			<a href="/history" class:active={$page.url.pathname === '/history'}>History</a>
			<a href="/settings" class:active={$page.url.pathname === '/settings'}>Settings</a>
		</div>

		<div class="nav-status">
			{#if resamplingStatus?.active}
				<div class="resampling-status">
					<span class="resampling-label">Resampling</span>
					<span class="resampling-value">{resamplingStatus.current}/{resamplingStatus.total}</span>
				</div>
			{/if}

			{#if memoryStats}
				<div class="memory-stats">
					<span class="memory-value">{memoryStats.memory_mb.toFixed(1)} MB</span>
					<span class="memory-label">{memoryStats.track_count} tracks</span>
				</div>
			{/if}
		</div>
	</nav>

	<main>
		{@render children()}
	</main>
</div>

<style>
	:global(body) {
		margin: 0;
		padding: 0;
		background: radial-gradient(ellipse at 50% 0%, rgba(56, 89, 138, 0.04) 0%, transparent 50%), #0a0a0a;
		color: #e5e5e5;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
	}

	:global(html) {
		background-color: #0a0a0a;
	}

	.app {
		display: flex;
		flex-direction: column;
		min-height: 100vh;
		background: transparent;
	}

	.nav {
		background-color: #0a0a0a;
		border-bottom: 1px solid #1a1a1a;
		display: flex;
		gap: 0;
		padding: 0;
		position: relative;
	}

	.hamburger {
		display: none;
		flex-direction: column;
		justify-content: space-around;
		width: 2.5rem;
		height: 2.5rem;
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 0.5rem;
		z-index: 10;
		margin: 0.5rem;
	}

	.hamburger-line {
		width: 100%;
		height: 3px;
		background-color: #666;
		transition: all 0.3s;
		border-radius: 2px;
	}

	.nav-links {
		display: flex;
		gap: 0;
		flex: 1;
	}

	.nav a {
		color: #666;
		text-decoration: none;
		padding: 1rem 1rem;
		font-weight: 500;
		transition: all 0.2s;
		border-bottom: 2px solid transparent;
	}

	.nav a:hover {
		color: #fff;
		background-color: #111;
	}

	.nav a.active {
		color: #fff;
		border-bottom-color: #fff;
	}

	main {
		flex: 1;
	}

	.nav-status {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}

	.resampling-status {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 0.5rem 1rem;
		gap: 0.125rem;
		background: rgba(251, 191, 36, 0.1);
		border-radius: 4px;
	}

	.resampling-label {
		font-size: 0.65rem;
		color: #fbbf24;
		text-transform: uppercase;
		font-weight: 600;
	}

	.resampling-value {
		font-size: 0.9rem;
		font-weight: 600;
		color: #fbbf24;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
	}

	.memory-stats {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		justify-content: center;
		padding: 0.5rem 1rem;
		gap: 0.125rem;
	}

	.memory-value {
		font-size: 0.8rem;
		font-weight: 500;
		color: #888;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
	}

	.memory-label {
		font-size: 0.65rem;
		color: #555;
	}

	/* Mobile styles */
	@media (max-width: 768px) {
		.nav {
			z-index: 100;
			background-color: transparent;
			border-bottom: none;
		}

		.hamburger {
			display: flex;
		}

		.nav-links {
			position: absolute;
			top: 100%;
			left: 0;
			right: 0;
			flex-direction: column;
			background-color: #0a0a0a;
			border-bottom: 1px solid #1a1a1a;
			transform: translateY(-100%);
			opacity: 0;
			visibility: hidden;
			transition: all 0.3s ease-in-out;
			gap: 0;
			z-index: 100;
			box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
		}

		.nav-links.open {
			transform: translateY(0);
			opacity: 1;
			visibility: visible;
		}

		.nav a {
			padding: 1rem 1.5rem;
			border-bottom: 1px solid #1a1a1a;
			border-left: 3px solid transparent;
		}

		.nav a.active {
			border-bottom-color: #1a1a1a;
			border-left-color: #fff;
		}
	}
</style>
