<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import favicon from '$lib/assets/favicon.svg';
	import { initBoardsListener, cleanupBoardsListener, initPresets } from '$lib/boards-db';

	let { children } = $props();

	// Initialize data listeners on mount
	onMount(() => {
		if (browser) {
			// Initialize boards SSE listener
			initBoardsListener();

			// Initialize presets (static list)
			initPresets();
		}
	});

	// Cleanup listeners on destroy
	onDestroy(() => {
		if (browser) {
			cleanupBoardsListener();
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="app">
	<nav class="nav">
		<a href="/" class:active={$page.url.pathname === '/'}>Boards</a>
		<a href="/sequencer" class:active={$page.url.pathname === '/sequencer'}>Sequencer</a>
	</nav>

	<main>
		{@render children()}
	</main>
</div>

<style>
	:global(body) {
		margin: 0;
		padding: 0;
	}

	.app {
		display: flex;
		flex-direction: column;
		min-height: 100vh;
	}

	.nav {
		background-color: #1a1a1a;
		border-bottom: 1px solid #2a2a2a;
		display: flex;
		gap: 0;
		padding: 0;
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
</style>
