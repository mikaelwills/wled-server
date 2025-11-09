import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		sveltekit()
	],
	server: {
		host: '0.0.0.0',
		port: 3011
	},
	build: {
		rollupOptions: {
			onwarn(warning, warn) {
				// Ignore a11y and unused CSS warnings
				if (warning.code === 'a11y-click-events-have-key-events') return;
				if (warning.code === 'a11y-no-static-element-interactions') return;
				if (warning.code === 'a11y-label-has-associated-control') return;
				if (warning.code === 'a11y-no-noninteractive-element-interactions') return;
				if (warning.code === 'css-unused-selector') return;
				warn(warning);
			}
		}
	}
});
