<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import favicon from '$lib/assets/favicon.svg';
	import { initBoardsListener, cleanupBoardsListener, fetchPresets } from '$lib/boards-db';
	import { initPrograms, cleanupPrograms } from '$lib/programs-db';

	let { children } = $props();

	// Mobile menu state
	let mobileMenuOpen = $state(false);

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

			// Fetch presets from server
			await fetchPresets();

			// Initialize programs from API
			initPrograms();
		}
	});

	// Cleanup listeners on destroy
	onDestroy(() => {
		if (browser) {
			cleanupBoardsListener();
			cleanupPrograms();
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="app">
	<nav class="nav">
		<button class="hamburger" onclick={toggleMobileMenu} aria-label="Toggle menu">
			<span class="hamburger-line"></span>
			<span class="hamburger-line"></span>
			<span class="hamburger-line"></span>
		</button>

		<div class="nav-links" class:open={mobileMenuOpen}>
			<a href="/" class:active={$page.url.pathname === '/'}>Boards</a>
			<a href="/sequencer" class:active={$page.url.pathname === '/sequencer'}>Programming</a>
			<a href="/performance" class:active={$page.url.pathname === '/performance'}>Performance</a>
			<a href="/settings" class:active={$page.url.pathname === '/settings'}>Settings</a>
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
		background-color: #1a1a1a;
		color: #e0e0e0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
	}

	:global(html) {
		background-color: #1a1a1a;
	}

	.app {
		display: flex;
		flex-direction: column;
		min-height: 100vh;
		background-color: #1a1a1a;
	}

	.nav {
		background-color: #1a1a1a;
		border-bottom: 1px solid #2a2a2a;
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
		background-color: #9ca3af;
		transition: all 0.3s;
		border-radius: 2px;
	}

	.nav-links {
		display: flex;
		gap: 0;
		flex: 1;
	}

	.nav a {
		color: #9ca3af;
		text-decoration: none;
		padding: 1rem 1.5rem;
		font-weight: 500;
		transition: all 0.2s;
		border-bottom: 2px solid transparent;
	}

	.nav a:hover {
		color: #e5e5e5;
		background-color: #2a2a2a;
	}

	.nav a.active {
		color: #a855f7;
		border-bottom-color: #a855f7;
	}

	main {
		flex: 1;
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
			background-color: #1a1a1a;
			border-bottom: 1px solid #2a2a2a;
			transform: translateY(-100%);
			opacity: 0;
			visibility: hidden;
			transition: all 0.3s ease-in-out;
			gap: 0;
			z-index: 100;
			box-shadow: 0 4px 6px rgba(0, 0, 0, 0.3);
		}

		.nav-links.open {
			transform: translateY(0);
			opacity: 1;
			visibility: visible;
		}

		.nav a {
			padding: 1rem 1.5rem;
			border-bottom: 1px solid #2a2a2a;
			border-left: 3px solid transparent;
		}

		.nav a.active {
			border-bottom-color: #2a2a2a;
			border-left-color: #a855f7;
		}
	}
</style>
